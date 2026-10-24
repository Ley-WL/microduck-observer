//! Signed USTAR+gzip bundles. No package-provided scripts are ever executed.
use super::{cmd,ensure_idle,point,snapshot,write_status,ROOT,WORK};
use anyhow::{ensure,bail,Result};
use base64::{Engine,engine::general_purpose::STANDARD};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,fs,io::{Read,Write},path::{Path,PathBuf},process::{Command,Stdio},time::Duration};
use std::os::{fd::{AsRawFd,FromRawFd},unix::fs::{MetadataExt,OpenOptionsExt,PermissionsExt}};
const PRIVATE:&str="/var/lib/microduck-observer-update";
const UPLOADS:&str="/var/lib/microduck-observer/update-uploads";
const KEY:&str="/usr/local/share/microduck-observer/ota-public.pem";
const MAX:u64=128*1024*1024;
const FILE_MAX:u64=100*1024*1024;
pub fn recovery_pending()->bool {Path::new(PRIVATE).join("journal.json").exists()}
fn id(s:&str)->Result<uuid::Uuid>{let u=uuid::Uuid::parse_str(s)?;ensure!(u.to_string()==s,"无效包编号");Ok(u)}
fn private()->Result<PathBuf>{let p=PathBuf::from(PRIVATE);fs::create_dir_all(&p)?;fs::set_permissions(&p,fs::Permissions::from_mode(0o700))?;Ok(p)}
fn lock()->Result<fs::File>{let f=fs::OpenOptions::new().create(true).truncate(false).write(true).open(private()?.join("lock"))?;ensure!(unsafe{libc::flock(f.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0,"另一个更新请求正在处理");Ok(f)}
fn allowed(path:&str,component:&str)->bool{
    if path.is_empty()||path.len()>180||path.contains('\\')||path.split('/').any(|s|s.is_empty()||s=="."||s=="..")||!path.bytes().all(|b|b.is_ascii_alphanumeric()||b"/._-".contains(&b)){return false;}
    match component {
        "backend"=>matches!(path,"bin/microduck-observer"|"bin/microduck-tof")||path.starts_with("licenses/"),
        "frontend"=>path.starts_with("frontend/dist/"),
        "models"=>path.strip_prefix("models/").is_some_and(|n|!n.contains('/')&&(n.ends_with(".onnx")||n.ends_with(".metadata.json"))),_=>false
    }
}
fn validate(v:&Value)->Result<BTreeMap<String,(u64,String)>>{
    ensure!(v["schema"]=="microduck-observer-bundle/v1"&&v["architecture"]=="aarch64"&&v["updaterProtocol"]==1,"更新包格式或平台不兼容");
    let version=v["version"].as_str().unwrap_or("");ensure!(!version.is_empty()&&version.len()<=64&&version.bytes().all(|c|c.is_ascii_alphanumeric()||b".-_".contains(&c)),"版本名称无效");
    ensure!(v["versionCode"].as_u64().is_some_and(|n|n>0),"版本号无效");
    let components=v["components"].as_array().ok_or_else(||anyhow::anyhow!("缺少组件"))?;
    ensure!(!components.is_empty()&&components.len()<=3&&components.iter().all(|c|c=="backend"||c=="frontend"||c=="models"),"组件不合法");
    ensure!(components.iter().enumerate().all(|(i,c)|!components[..i].contains(c)),"重复组件");
    let files=v["files"].as_object().ok_or_else(||anyhow::anyhow!("缺少文件清单"))?;ensure!(!files.is_empty()&&files.len()<=2048,"文件数量越界");
    let mut out=BTreeMap::new();let mut total=0u64;
    for (p,f) in files {
        ensure!(components.iter().any(|c|allowed(p,c.as_str().unwrap())),"不允许更新路径：{p}");
        let size=f["size"].as_u64().filter(|n|*n>0&&*n<=FILE_MAX).ok_or_else(||anyhow::anyhow!("文件大小无效"))?;
        total+=size;ensure!(total<=MAX,"解压后文件超过128MiB");
        let sha=f["sha256"].as_str().unwrap_or("");ensure!(sha.len()==64&&sha.bytes().all(|b|b.is_ascii_hexdigit()&&!b.is_ascii_uppercase()),"SHA256无效");out.insert(p.clone(),(size,sha.into()));
    }
    if components.contains(&json!("backend")){ensure!(out.contains_key("bin/microduck-observer"),"后端缺少主程序");}
    if components.contains(&json!("frontend")){ensure!(out.contains_key("frontend/dist/index.html"),"前端缺少index.html");}
    if components.contains(&json!("models")) {
        ensure!(out.keys().any(|p|p.starts_with("models/")&&p.ends_with(".onnx")),"模型组件缺少ONNX文件");
        for p in out.keys().filter(|p|p.starts_with("models/")) {
            let peer=if let Some(stem)=p.strip_suffix(".onnx"){format!("{stem}.metadata.json")}else{format!("{}.onnx",p.strip_suffix(".metadata.json").unwrap())};
            ensure!(out.contains_key(&peer),"模型和metadata必须成对更新：{p}");
        }
    }
    Ok(out)
}
fn signed(work:&Path,bytes:&[u8],key:&Path)->Result<Value>{
    ensure!(bytes.len()<=512*1024,"清单过大");let e:Value=serde_json::from_slice(bytes)?;
    let payload=STANDARD.decode(e["payload"].as_str().unwrap_or(""))?;
    let sig=STANDARD.decode(e["signature"].as_str().unwrap_or(""))?;
    fs::write(work.join("signed.json"),&payload)?;fs::write(work.join("signature"),sig)?;
    cmd("/usr/bin/openssl",&["dgst","-sha256","-verify",key.to_str().unwrap(),"-signature",work.join("signature").to_str().unwrap(),work.join("signed.json").to_str().unwrap()])?;
    let v:Value=serde_json::from_slice(&payload)?;validate(&v)?;Ok(v)
}
fn octal(b:&[u8])->Result<u64>{let s=std::str::from_utf8(b)?.trim_matches(|c|c=='\0'||c==' ');ensure!(!s.is_empty()&&s.bytes().all(|b|(b'0'..=b'7').contains(&b)),"非法TAR数字");Ok(u64::from_str_radix(s,8)?)}
fn header(r:&mut impl Read)->Result<Option<(String,u64)>>{
    let mut b=[0u8;512];r.read_exact(&mut b)?;
    if b.iter().all(|b|*b==0){let mut second=[0u8;512];r.read_exact(&mut second)?;ensure!(second.iter().all(|b|*b==0),"TAR结束块损坏");return Ok(None);}
    let sum:u64=b.iter().enumerate().map(|(i,b)|if (148..156).contains(&i){32}else{*b as u64}).sum();
    ensure!(sum==octal(&b[148..156])?,"TAR头校验失败");
    ensure!(b[156]==0||b[156]==b'0',"更新包只允许普通文件，不允许链接/目录/扩展头");
    ensure!(&b[257..263]==b"ustar\0"&&b[345..500].iter().all(|b|*b==0),"仅支持简单USTAR包");
    let end=b[..100].iter().position(|b|*b==0).unwrap_or(100);let name=std::str::from_utf8(&b[..end])?.to_owned();
    Ok(Some((name,octal(&b[124..136])?)))
}
fn padding(r:&mut impl Read,size:u64)->Result<()>{let mut b=[0u8;512];r.read_exact(&mut b[..((512-size%512)%512) as usize])?;Ok(())}
fn extract(r:&mut impl Read,work:&Path,key:&Path)->Result<Value>{
    let (name,size)=header(r)?.ok_or_else(||anyhow::anyhow!("空更新包"))?;
    ensure!(name=="manifest.json"&&size<=512*1024,"首个文件必须是签名清单");
    let mut bytes=vec![0;size as usize];r.read_exact(&mut bytes)?;padding(r,size)?;
    let v=signed(work,&bytes,key)?;let mut pending=validate(&v)?;
    while let Some((name,size))=header(r)? {
        let p=name.strip_prefix("payload/").ok_or_else(||anyhow::anyhow!("未知TAR文件"))?;
        let (expected,sha)=pending.remove(p).ok_or_else(||anyhow::anyhow!("文件重复或不在签名清单内"))?;ensure!(size==expected,"文件大小不匹配");
        let dest=work.join("payload").join(p);fs::create_dir_all(dest.parent().unwrap())?;
        let mut out=fs::OpenOptions::new().create_new(true).write(true).open(&dest)?;
        let mut hash=Sha256::new();let mut remaining=size;let mut buffer=[0u8;65536];
        while remaining>0 {let n=(remaining as usize).min(buffer.len());r.read_exact(&mut buffer[..n])?;out.write_all(&buffer[..n])?;hash.update(&buffer[..n]);remaining-=n as u64;}
        out.sync_all()?;ensure!(hex::encode(hash.finalize())==sha,"文件SHA256不匹配：{p}");padding(r,size)?;
        if p.starts_with("models/")&&p.ends_with(".metadata.json"){let metadata:Value=serde_json::from_slice(&fs::read(&dest)?)?;ensure!(metadata.is_object(),"模型metadata必须是JSON对象");}
        if p.starts_with("bin/"){let b=fs::read(&dest)?;ensure!(b.len()>20&&&b[..6]==b"\x7fELF\x02\x01"&&u16::from_le_bytes([b[18],b[19]])==183,"后端不是ARM64程序");
            if p=="bin/microduck-observer"{ensure!(b.windows(14).any(|w|w==b"/api/v1/update"),"后端缺少网页更新接口，需先升级兼容版本");}}
    }
    ensure!(pending.is_empty(),"签名清单中的文件缺失");
    // tarfile adds a bounded zero pad; reject appended archives or gzip bombs.
    let mut tail=vec![];r.take(10241).read_to_end(&mut tail)?;ensure!(tail.len()<=10240&&tail.iter().all(|b|*b==0),"TAR尾部有多余数据");Ok(v)
}
struct Decoder(std::process::Child);
impl Drop for Decoder {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
fn verify_archive(archive:&Path,work:&Path,key:&Path)->Result<Value>{
    ensure!(fs::metadata(archive)?.len()<=MAX,"更新包过大");
    let mut decoder=Decoder(Command::new("/usr/bin/gzip").arg("-dc").arg(archive).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?);
    let v=extract(&mut decoder.0.stdout.take().unwrap(),work,key)?;ensure!(decoder.0.wait()?.success(),"gzip文件损坏");Ok(v)
}
pub fn verify_local(archive:&Path,key:&Path)->Result<Value>{
    let work=std::env::temp_dir().join(format!("observer-bundle-check-{}",uuid::Uuid::new_v4()));fs::create_dir(&work)?;
    let result=verify_archive(archive,&work,key);let _=fs::remove_dir_all(work);result
}
fn stage(id_str:&str)->Result<Value>{
    id(id_str)?;let _lock=lock()?;ensure!(!ota_busy(),"更新正在进行");
    let base=private()?;
    for entry in fs::read_dir(&base)? {let path=entry?.path();if path.file_name().is_some_and(|n|n.to_string_lossy().starts_with("staging-")){fs::remove_dir_all(path)?;}}
    // At most one prepared bundle: a new successful upload replaces it.
    let work=base.join(format!("staging-{id_str}"));fs::create_dir(&work)?;
    let result=(||->Result<Value>{
        let dir=fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW).open(UPLOADS)?;
        let name=std::ffi::CString::new(format!("{id_str}.tar.gz"))?;
        let fd=unsafe{libc::openat(dir.as_raw_fd(),name.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC)};ensure!(fd>=0,"上传文件无法读取");
        let mut source=unsafe{fs::File::from_raw_fd(fd)};let meta=source.metadata()?;ensure!(meta.is_file()&&meta.len()>0&&meta.len()<=MAX,"上传大小不合法");
        let archive=work.join("archive.gz");let mut out=fs::File::create(&archive)?;let n=std::io::copy(&mut (&mut source).take(MAX+1),&mut out)?;ensure!(n<=MAX,"上传文件发生变化");out.sync_all()?;drop(out);
        let v=verify_archive(&archive,&work,Path::new(KEY))?;
        let prepared=base.join("prepared");if prepared.exists(){fs::remove_dir_all(&prepared)?;}fs::remove_file(&archive)?;
        fs::write(work.join("manifest.json"),serde_json::to_vec(&v)?)?;fs::write(work.join("id"),id_str)?;fs::rename(&work,&prepared)?;
        Ok(json!({"id":id_str,"version":v["version"],"versionCode":v["versionCode"],"components":v["components"],"files":v["files"].as_object().unwrap().len(),"verified":true,"models":v["files"].as_object().unwrap().keys().filter(|p|p.starts_with("models/")&&p.ends_with(".onnx")).collect::<Vec<_>>()}))
    })();if result.is_err(){let _=fs::remove_dir_all(work);}result
}
fn ota_busy()->bool{super::ota::busy(&super::ota::status())}
fn prepared(id_str:&str)->Result<(PathBuf,Value)>{id(id_str)?;let p=private()?.join("prepared");ensure!(fs::read_to_string(p.join("id"))?==id_str,"更新包已被替换，请重新上传");let v:Value=serde_json::from_slice(&fs::read(p.join("manifest.json"))?)?;validate(&v)?;Ok((p,v))}
pub fn request(v:&Value)->Result<Value>{
    match v["op"].as_str(){
        Some("bundle-info")=>Ok(json!({"protocol":1,"signed":true,"maxBytes":MAX})),
        Some("bundle-stage")=>stage(v["id"].as_str().unwrap_or("")),
        Some("bundle-apply")=>{
            let _lock=lock()?;ensure!(!ota_busy(),"已有更新任务");let id_str=v["id"].as_str().unwrap_or("");let (_,m)=prepared(id_str)?;ensure_idle()?;
            let previous=fs::read(format!("{ROOT}/current/bundle.json")).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()).and_then(|v|v["versionCode"].as_u64()).unwrap_or(0);
            ensure!(m["versionCode"].as_u64().unwrap()>previous,"版本号必须高于当前更新包（回退由失败恢复机制负责）");
            write_status(json!({"phase":"queued","id":id_str,"version":m["version"]}))?;
            if let Err(e)=cmd("/usr/bin/systemd-run",&["--quiet","--collect","--unit=microduck-observer-bundle-job","--on-active=3s","--timer-property=AccuracySec=1s","--property=RuntimeMaxSec=300","--property=OnFailure=microduck-observer-update-recover.service","/usr/local/lib/microduck-observer-ota-helper","bundle-install",id_str]){
                write_status(json!({"phase":"failed","message":e.to_string()}))?;return Err(e);
            }
            Ok(json!({"accepted":true,"phase":"queued","id":id_str}))
        },_=>bail!("未知更新包操作")
    }
}
fn hashes(p:&Path)->Result<BTreeMap<PathBuf,String>>{
    fn walk(root:&Path,p:&Path,out:&mut BTreeMap<PathBuf,String>)->Result<()>{for e in fs::read_dir(p)?{let p=e?.path();let m=fs::symlink_metadata(&p)?;ensure!(!m.file_type().is_symlink(),"发布目录不允许符号链接");if m.is_dir(){walk(root,&p,out)?;}else {ensure!(m.is_file(),"发布目录含特殊文件");out.insert(p.strip_prefix(root)?.into(),hex::encode(Sha256::digest(fs::read(&p)?)));}}Ok(())}
    let mut out=BTreeMap::new();walk(p,p,&mut out)?;Ok(out)
}
struct Candidate {path:PathBuf,keep:bool}
impl Drop for Candidate {fn drop(&mut self){if !self.keep{let _=fs::remove_dir_all(&self.path);}}}
pub fn install(id_str:&str)->Result<Value>{
    let _lock=lock()?;
    // Serialize with the original signed-feed installer too.
    let old_lock=fs::OpenOptions::new().create(true).truncate(false).write(true).open(Path::new(WORK).join("install.lock"))?;
    ensure!(unsafe{libc::flock(old_lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0,"已有OTA安装任务");
    let (p,v)=prepared(id_str)?;let files=validate(&v)?;ensure_idle()?;
    let old=fs::canonicalize(format!("{ROOT}/current"))?;let releases=Path::new(ROOT).join("releases");ensure!(old.parent()==Some(releases.as_path()),"当前版本目录不合法");
    let old_hashes=hashes(&old)?;
    let required:u64=old_hashes.keys().map(|p|fs::metadata(old.join(p)).map(|m|m.len())).collect::<std::io::Result<Vec<_>>>()?.iter().sum::<u64>()+MAX;
    let root_c=std::ffi::CString::new(ROOT)?;let mut space=std::mem::MaybeUninit::<libc::statvfs>::uninit();
    ensure!(unsafe{libc::statvfs(root_c.as_ptr(),space.as_mut_ptr())}==0,"无法检查磁盘空间");let space=unsafe{space.assume_init()};
    ensure!((space.f_bavail as u64).saturating_mul(space.f_frsize as u64)>required,"磁盘空间不足以保留回退版本");
    let mut settings=BTreeMap::new();
    for e in fs::read_dir("/var/lib/microduck-observer")?{let p=e?.path();if p.is_file()&&p.extension().is_some_and(|s|s=="json"){settings.insert(p.clone(),fs::read(&p)?);}}
    let identity=Path::new("/var/lib/ducklink");if identity.exists(){for p in hashes(identity)?.keys(){let p=identity.join(p);settings.insert(p.clone(),fs::read(p)?);}}
    let next=releases.join(format!("bundle-{}-{id_str}",v["versionCode"]));ensure!(!next.exists(),"候选目录已存在");
    let mut candidate=Candidate{path:next.clone(),keep:false};
    cmd("/usr/bin/cp",&["-a",old.to_str().unwrap(),next.to_str().unwrap()])?;
    if v["components"].as_array().unwrap().contains(&json!("frontend")){fs::remove_dir_all(next.join("frontend/dist"))?;}
    for (path,(size,sha)) in &files {
        let data=fs::read(p.join("payload").join(path))?;ensure!(data.len() as u64==*size&&hex::encode(Sha256::digest(&data))==*sha,"已验证文件发生变化");
        let dest=next.join(path);fs::create_dir_all(dest.parent().unwrap())?;let mut f=fs::File::create(&dest)?;f.write_all(&data)?;f.sync_all()?;
        fs::set_permissions(dest,fs::Permissions::from_mode(if path.starts_with("bin/"){0o755}else{0o644}))?;
    }
    for (path,sha) in old_hashes {let s=path.to_str().unwrap();if files.contains_key(s)||(v["components"].as_array().unwrap().contains(&json!("frontend"))&&s.starts_with("frontend/dist/")){continue;}ensure!(hex::encode(Sha256::digest(fs::read(next.join(&path))?))==sha,"设备文件被改变");}
    fs::write(next.join("bundle.json"),serde_json::to_vec(&v)?)?;
    cmd("/usr/bin/timeout",&["45","/usr/sbin/runuser","-u","radxa","--","/usr/bin/env",&format!("MICRODUCK_POLICY_PATH={}/models/xgoduck_walk.onnx",next.display()),&format!("ORT_DYLIB_PATH={}/lib/libonnxruntime.so",next.display()),"MICRODUCK_GAINS_FILE=/var/lib/microduck-observer/model-gains.json",next.join("bin/microduck-observer").to_str().unwrap(),"--verify-model-bank"])?;
    ensure_idle()?;
    write_status(json!({"phase":"installing","id":id_str,"version":v["version"],"previousRelease":old,"release":next}))?;
    // Separate root-owned journal survives service interruption and a host reboot.
    journal(&old,&next,&settings)?;
    candidate.keep=true;
    let result=(||->Result<()>{
        cmd("/usr/bin/systemctl",&["stop","microduck-observer"])?;point(&next)?;cmd("/usr/bin/systemctl",&["start","microduck-observer"])?;
        let mut good=false;
        for _ in 0..60 {std::thread::sleep(Duration::from_millis(500));if let Ok(s)=snapshot(){if super::ota::idle(&s){let pid=cmd("/usr/bin/systemctl",&["show","-p","MainPID","--value","microduck-observer"])?;let pid=std::str::from_utf8(&pid)?.trim();if fs::canonicalize(format!("/proc/{pid}/exe"))?==next.join("bin/microduck-observer"){good=true;break;}}}}
        ensure!(good,"新服务未通过启动验收");
        let capability:Value=serde_json::from_slice(&cmd("/usr/bin/curl",&["--fail","--silent","--max-time","8","http://127.0.0.1:8877/api/v1/update"])?)?;ensure!(capability["protocol"]==1,"新服务不支持继续网页更新");
        for (path,(_,sha)) in &files {if let Some(asset)=path.strip_prefix("frontend/dist/"){let served=cmd("/usr/bin/curl",&["--fail","--silent","--max-time","5",&format!("http://127.0.0.1:8877/{asset}")])?;ensure!(hex::encode(Sha256::digest(served))==*sha,"网页HTTP资源验收失败：{asset}");}}
        for (path,bytes) in &settings {ensure!(fs::read(path)?==*bytes,"设备设置被修改");}
        for _ in 0..20{ensure_idle()?;std::thread::sleep(Duration::from_millis(100));}
        Ok(())
    })();
    if let Err(e)=result{rollback(&old)?;candidate.keep=false;bail!("{e}；已恢复旧服务");}
    fs::remove_file(private()?.join("journal.json"))?;
    Ok(json!({"phase":"success","id":id_str,"version":v["version"],"release":next,"previousRelease":old,"hardwareMotion":false}))
}
fn journal(old:&Path,next:&Path,settings:&BTreeMap<PathBuf,Vec<u8>>)->Result<()>{
    let mut saved=serde_json::Map::new();for (p,b) in settings {let m=fs::metadata(p)?;saved.insert(p.to_str().unwrap().into(),json!({"bytes":STANDARD.encode(b),"uid":m.uid(),"gid":m.gid(),"mode":m.mode()&0o777}));}
    let p=private()?.join("journal.json");let temp=p.with_extension("tmp");let mut f=fs::File::create(&temp)?;f.write_all(&serde_json::to_vec(&json!({"old":old,"next":next,"settings":saved}))?)?;f.sync_all()?;fs::rename(temp,p)?;fs::File::open(PRIVATE)?.sync_all()?;Ok(())
}
fn restore_settings()->Result<()>{
    let v:Value=serde_json::from_slice(&fs::read(private()?.join("journal.json"))?)?;
    for (path,value) in v["settings"].as_object().ok_or_else(||anyhow::anyhow!("配置备份缺失"))? {
        let p=Path::new(path);ensure!(p.starts_with("/var/lib/ducklink")||p.parent()==Some(Path::new("/var/lib/microduck-observer")),"配置回退路径无效");
        let temp=p.with_extension(format!("bundle-restore-{}",uuid::Uuid::new_v4()));let mut f=fs::OpenOptions::new().create_new(true).write(true).open(&temp)?;f.write_all(&STANDARD.decode(value["bytes"].as_str().unwrap_or(""))?)?;
        ensure!(unsafe{libc::fchown(f.as_raw_fd(),value["uid"].as_u64().unwrap() as u32,value["gid"].as_u64().unwrap() as u32)}==0,"恢复配置所有者失败");
        f.set_permissions(fs::Permissions::from_mode(value["mode"].as_u64().unwrap() as u32))?;f.sync_all()?;fs::rename(temp,p)?;
    }Ok(())
}
fn rollback(old:&Path)->Result<()>{let _=cmd("/usr/bin/systemctl",&["stop","microduck-observer"]);restore_settings()?;point(old)?;cmd("/usr/bin/systemctl",&["start","microduck-observer"])?;fs::remove_file(private()?.join("journal.json"))?;Ok(())}
pub fn recover(boot:bool)->Result<Value>{let _lock=lock()?;let p=private()?.join("journal.json");if !p.exists(){let s=super::ota::status();if s["id"].is_string()&&ota_busy(){write_status(json!({"phase":"failed","message":"更新在切换前中断，原服务保持"}))?;}return Ok(json!({"recovery":"not-needed"}));}write_status(json!({"phase":"recovering","message":"正在恢复中断的更新"}))?;let v:Value=serde_json::from_slice(&fs::read(&p)?)?;let old=fs::canonicalize(v["old"].as_str().unwrap_or(""))?;ensure!(old.parent()==Some(Path::new(ROOT).join("releases").as_path()),"回退目录不合法");if boot {restore_settings()?;point(&old)?;fs::remove_file(p)?;}else{rollback(&old)?;}let v=json!({"phase":"failed","message":"更新被中断，已恢复旧服务","previousRelease":old});write_status(v.clone())?;Ok(v)}

#[cfg(test)] mod tests {
    use super::*;
    fn manifest()->Value {json!({"schema":"microduck-observer-bundle/v1","architecture":"aarch64","updaterProtocol":1,"version":"test-1","versionCode":1,"components":["frontend"],"files":{"frontend/dist/index.html":{"size":1,"sha256":"a".repeat(64)}}})}
    #[test] fn only_selected_components_and_bounded_paths(){let v=manifest();assert!(validate(&v).is_ok());for path in ["../x","frontend/dist/../x","/etc/passwd","frontend/dist/a\\b","bin/microduck-observer","models/actor.onnx"]{let mut v=manifest();v["files"]=json!({path:{"size":1,"sha256":"a".repeat(64)}});assert!(validate(&v).is_err(),"{path}");}let mut v=manifest();v["files"]["frontend/dist/index.html"]["size"]=json!(MAX+1);assert!(validate(&v).is_err());}
    #[test] fn rejects_other_platforms_and_future_protocol(){for (k,x) in [("architecture",json!("x86_64")),("updaterProtocol",json!(2)),("components",json!(["backend","backend"])),("version",json!("../x"))]{let mut v=manifest();v[k]=x;assert!(validate(&v).is_err());}}
    #[test] fn malformed_tar_fails_before_any_extraction(){for data in [vec![0;512],vec![1;1024]]{assert!(header(&mut data.as_slice()).is_err());}}
}
