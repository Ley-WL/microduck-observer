//! Root socket helper: fixed signed feed, binary-only release, bounded rollback.
use anyhow::{bail,ensure,Result};
use base64::{Engine,engine::general_purpose::STANDARD};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs,path::{Path,PathBuf},process::Command,io::{Read,Write},time::Duration};
#[path="../observer_ota.rs"] mod ota;
#[path="../update_bundle.rs"] mod bundle;
const ROOT:&str="/home/radxa/microduck-observer";
const BASE:&str="http://8.133.210.70:6661/microduck/observer/";
const WORK:&str="/var/lib/microduck-observer/ota";
fn cmd(name:&str,args:&[&str])->Result<Vec<u8>>{let out=Command::new(name).args(args).output()?;ensure!(out.status.success(),"{name}执行失败");Ok(out.stdout)}
fn write_status(v:Value)->Result<()>{fs::create_dir_all(WORK)?;let p=Path::new(WORK).join("status.tmp");fs::write(&p,serde_json::to_vec(&v)?)?;fs::rename(p,ota::STATE)?;Ok(())}
fn download(url:&str,p:&Path,seconds:&str,max:&str)->Result<()>{cmd("/usr/bin/curl",&["--fail","--silent","--show-error","--max-time",seconds,"--max-filesize",max,"--proto","=http","--max-redirs","0","-o",p.to_str().unwrap(),url])?;Ok(())}
fn validate(v:&Value)->Result<()> {
    let code=v["versionCode"].as_u64().ok_or_else(||anyhow::anyhow!("无版本号"))?;
    ensure!(v["schema"]=="microduck-observer-ota/v1"&&v["architecture"]=="aarch64"&&code>0,"更新合同不匹配");
    let version=v["version"].as_str().unwrap_or("");ensure!(!version.is_empty()&&version.len()<32&&version.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'.'||c==b'-'),"版本格式错误");
    ensure!(v["url"]==format!("{BASE}observer-{code}.bin"),"更新来源错误");
    ensure!(v["size"].as_u64().is_some_and(|n|(100000..=100000000).contains(&n)),"更新大小错误");
    let sha=v["sha256"].as_str().unwrap_or("");ensure!(sha.len()==64&&sha.bytes().all(|c|c.is_ascii_hexdigit()),"SHA格式错误");Ok(())
}
fn verified_envelope(work:&Path, envelope:&[u8], key:&Path)->Result<Value>{
    let e:Value=serde_json::from_slice(envelope)?;
    let payload=STANDARD.decode(e["payload"].as_str().unwrap_or(""))?;let signature=STANDARD.decode(e["signature"].as_str().unwrap_or(""))?;
    fs::write(work.join("payload.json"),&payload)?;fs::write(work.join("signature.bin"),signature)?;
    cmd("/usr/bin/openssl",&["dgst","-sha256","-verify",key.to_str().unwrap(),"-signature",work.join("signature.bin").to_str().unwrap(),work.join("payload.json").to_str().unwrap()])?;
    let v:Value=serde_json::from_slice(&payload)?;validate(&v)?;Ok(v)
}
fn manifest()->Result<Value>{
    fs::create_dir_all(WORK)?;let work=Path::new(WORK).join(format!("manifest-{}",uuid::Uuid::new_v4()));fs::create_dir(&work)?;
    let result=(||{download(&format!("{BASE}latest.json"),&work.join("manifest.json"),"8","16384")?;
        verified_envelope(&work,&fs::read(work.join("manifest.json"))?,Path::new("/usr/local/share/microduck-observer/ota-public.pem"))})();
    let _=fs::remove_dir_all(&work);result
}
fn snapshot()->Result<Value>{Ok(serde_json::from_slice(&cmd("/usr/bin/curl",&["--fail","--silent","--max-time","3","http://127.0.0.1:8877/api/v1/snapshot"])?)?)}
fn current_code()->u64{fs::read(format!("{ROOT}/current/ota.json")).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()).and_then(|v|v["versionCode"].as_u64()).unwrap_or(0)}
fn ensure_idle()->Result<()>{ensure!(ota::idle(&snapshot()? ),"请先急停卸力，必须15颗在线失能");Ok(())}
fn point(path:&Path)->Result<()>{use std::os::unix::fs::symlink;let temp=Path::new(ROOT).join("ota-current");if temp.exists()||temp.symlink_metadata().is_ok(){fs::remove_file(&temp)?;}symlink(path,&temp)?;fs::rename(temp,Path::new(ROOT).join("current"))?;Ok(())}
fn model_hashes(path:&Path)->Result<Vec<(PathBuf,String)>>{let mut r=vec![];fn walk(root:&Path,p:&Path,r:&mut Vec<(PathBuf,String)>)->Result<()>{for entry in fs::read_dir(p)?{let p=entry?.path();if p.is_dir(){walk(root,&p,r)?;}else if p.is_file(){r.push((p.strip_prefix(root)?.to_owned(),hex::encode(Sha256::digest(fs::read(p)?))));}}Ok(())}walk(path,path,&mut r)?;r.retain(|(p,_)|p!=Path::new("bin/microduck-observer")&&p!=Path::new("ota.json"));Ok(r)}
fn install()->Result<Value>{
    use std::os::unix::fs::PermissionsExt;
    let lock=fs::OpenOptions::new().write(true).create(true).truncate(false).open(Path::new(WORK).join("install.lock"))?;
    use std::os::fd::AsRawFd;ensure!(unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0,"已有更新任务");
    write_status(json!({"phase":"downloading"}))?;ensure_idle()?;let v=manifest()?;
    ensure!(v["versionCode"].as_u64().unwrap()>current_code(),"已是最新版本或拒绝版本回退");
    let binary=Path::new(WORK).join("observer.bin");download(v["url"].as_str().unwrap(),&binary,"120","100000000")?;
    let bytes=fs::read(&binary)?;ensure!(bytes.len() as u64==v["size"].as_u64().unwrap()&&hex::encode(Sha256::digest(&bytes))==v["sha256"],"更新文件校验失败");
    ensure!(bytes.len()>20&&&bytes[..4]==b"\x7fELF"&&bytes[4]==2&&bytes[5]==1&&u16::from_le_bytes([bytes[18],bytes[19]])==183,"不是ARM64可执行文件");
    let old=fs::canonicalize(format!("{ROOT}/current"))?;ensure!(old.starts_with(format!("{ROOT}/releases")),"当前release不合法");
    let models=model_hashes(&old)?;
    let mut settings=vec![];
    fn settings_walk(p:&Path,out:&mut Vec<(PathBuf,String)>)->Result<()>{if !p.exists(){return Ok(());}for e in fs::read_dir(p)?{let p=e?.path();if p.is_dir(){settings_walk(&p,out)?;}else if p.is_file(){out.push((p.clone(),hex::encode(Sha256::digest(fs::read(&p)?))));}}Ok(())}
    settings_walk(Path::new("/var/lib/ducklink"),&mut settings)?;
    for e in fs::read_dir("/var/lib/microduck-observer")?{let p=e?.path();if p.is_file()&&p.extension().is_some_and(|v|v=="json"){settings.push((p.clone(),hex::encode(Sha256::digest(fs::read(&p)?))));}}

    let next=Path::new(ROOT).join("releases").join(format!("observer-{}-{}-{}",v["version"].as_str().unwrap(),v["versionCode"],uuid::Uuid::new_v4()));ensure!(!next.exists(),"候选release已存在");
    cmd("/usr/bin/cp",&["-a",old.to_str().unwrap(),next.to_str().unwrap()])?;
    let target=next.join("bin/microduck-observer");fs::copy(&binary,&target)?;fs::set_permissions(&target,fs::Permissions::from_mode(0o755))?;fs::write(next.join("ota.json"),serde_json::to_vec(&v)?)?;
    for (p,sha) in &models{ensure!(hex::encode(Sha256::digest(fs::read(next.join(p))?))==*sha,"模型被改变");}
    ensure_idle()?;write_status(json!({"phase":"installing","version":v["version"]}))?;
    cmd("/usr/bin/systemctl",&["stop","microduck-observer"])?;
    let result=(||->Result<()>{point(&next)?;cmd("/usr/bin/systemctl",&["start","microduck-observer"])?;
        let mut good=false;for _ in 0..60{std::thread::sleep(Duration::from_millis(500));if let Ok(s)=snapshot(){if ota::idle(&s)&&s["system"]["data"]["resources"]["board"]["serviceVersion"]==v["version"] {good=true;break;}}}ensure!(good,"新版本启动验收失败");
        for (p,sha) in &settings{ensure!(hex::encode(Sha256::digest(fs::read(p)?))==*sha,"设备配置被改变");}
        for _ in 0..20 {ensure_idle()?;std::thread::sleep(Duration::from_millis(100));}Ok(())})();
    if let Err(e)=result{let _=cmd("/usr/bin/systemctl",&["stop","microduck-observer"]);point(&old)?;cmd("/usr/bin/systemctl",&["start","microduck-observer"])?;bail!("{e}；已回滚");}
    Ok(json!({"phase":"success","version":v["version"],"release":next,"previousRelease":old,"hardwareMotion":false}))
}
fn request(v:&Value)->Result<Value>{
    if v["op"].as_str().is_some_and(|s|s.starts_with("bundle-")){return bundle::request(v);}
    ensure!(v.as_object().is_some_and(|o|o.len()==1),"非法请求");
    match v["op"].as_str(){Some("check")=>{let v=manifest()?;Ok(json!({"manifest":v,"currentVersionCode":current_code(),"available":v["versionCode"].as_u64().unwrap()>current_code()}))},Some("apply")=>{
        use std::os::fd::AsRawFd;let lock=fs::OpenOptions::new().write(true).create(true).truncate(false).open(Path::new(WORK).join("request.lock"))?;ensure!(unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0,"已有更新请求");ensure_idle()?;
        ensure!(!ota::busy(&ota::status()),"已有更新任务");
        write_status(json!({"phase":"queued"}))?;
        if let Err(e)=cmd("/usr/bin/systemd-run",&["--quiet","--collect","--unit=microduck-observer-ota-job","--on-active=3s","--timer-property=AccuracySec=1s","--property=RuntimeMaxSec=300","/usr/local/lib/microduck-observer-ota-helper","install"]){write_status(json!({"phase":"failed","message":e.to_string()}))?;return Err(e);}
        Ok(json!({"accepted":true,"phase":"queued"}))},_=>bail!("非法请求")}
}
fn main(){let mode=std::env::args().nth(1).unwrap_or_default();let result=match mode.as_str(){"install"=>install(),"bundle-verify"=>bundle::verify_local(Path::new(&std::env::args().nth(2).unwrap_or_default()),Path::new(&std::env::args().nth(3).unwrap_or_default())),"bundle-install"=>bundle::install(&std::env::args().nth(2).unwrap_or_default()),"bundle-recover"=>bundle::recover(false),"bundle-recover-boot"=>bundle::recover(true),_=>{let mut input=String::new();std::io::stdin().take(257).read_to_string(&mut input).and_then(|_|std::io::Result::Ok(())).map_err(anyhow::Error::from).and_then(|_|Ok(serde_json::from_str::<Value>(&input)?)).and_then(|v|request(&v))}};
    let failed=result.is_err();
    let v=match result{Ok(v)=>v,Err(e)=>json!({"phase":if mode=="bundle-install"&&bundle::recovery_pending(){"recovering"}else{"failed"},"error":e.to_string(),"message":e.to_string()})};
    if matches!(mode.as_str(),"install"|"bundle-install"){let _=write_status(v.clone());}println!("{v}");let _=std::io::stdout().flush();if failed&&!mode.is_empty(){std::process::exit(1);}}
#[cfg(test)]mod tests{use super::*;#[test]fn rejects_unsigned_contract_urls_and_platforms(){let v=json!({"schema":"microduck-observer-ota/v1","architecture":"aarch64","version":"0.9.1","versionCode":1,"url":format!("{BASE}observer-1.bin"),"size":100000,"sha256":"a".repeat(64)});assert!(validate(&v).is_ok());for (key,bad) in [("url",json!("http://localhost/payload")),("architecture",json!("x86_64")),("version",json!("../bad")),("sha256",json!("a")),("size",json!(100000001))]{let mut v=v.clone();v[key]=bad;assert!(validate(&v).is_err());}}}

#[cfg(test)]mod signature_tests{use super::*;
#[test]fn verifies_exact_signed_payload_and_rejects_tamper(){
 let dir=std::env::temp_dir().join(format!("ota-test-{}",uuid::Uuid::new_v4()));fs::create_dir(&dir).unwrap();
 let private=dir.join("key.pem");let public=dir.join("public.pem");
 cmd("/usr/bin/openssl",&["genpkey","-algorithm","RSA","-pkeyopt","rsa_keygen_bits:2048","-out",private.to_str().unwrap()]).unwrap();
 cmd("/usr/bin/openssl",&["pkey","-in",private.to_str().unwrap(),"-pubout","-out",public.to_str().unwrap()]).unwrap();
 let payload=serde_json::to_vec(&json!({"schema":"microduck-observer-ota/v1","architecture":"aarch64","version":"0.9.1","versionCode":1,"url":format!("{BASE}observer-1.bin"),"size":100000,"sha256":"a".repeat(64)})).unwrap();
 fs::write(dir.join("data"),&payload).unwrap();
 cmd("/usr/bin/openssl",&["dgst","-sha256","-sign",private.to_str().unwrap(),"-out",dir.join("sig").to_str().unwrap(),dir.join("data").to_str().unwrap()]).unwrap();
 let mut envelope=json!({"payload":STANDARD.encode(&payload),"signature":STANDARD.encode(fs::read(dir.join("sig")).unwrap())});
 assert!(verified_envelope(&dir,&serde_json::to_vec(&envelope).unwrap(),&public).is_ok());
 envelope["payload"]=json!(STANDARD.encode(b"{}"));assert!(verified_envelope(&dir,&serde_json::to_vec(&envelope).unwrap(),&public).is_err());
 envelope["payload"]=json!(STANDARD.encode(&payload));envelope["signature"]=json!(STANDARD.encode(b"forged"));assert!(verified_envelope(&dir,&serde_json::to_vec(&envelope).unwrap(),&public).is_err());
 fs::remove_dir_all(dir).unwrap();
}}
