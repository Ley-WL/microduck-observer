//! Privileged Wi-Fi adapter. Same bounded Unix protocol as the former Python service.
//! PSKs only pass over the wpa control socket, never argv or logs.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, fs, io::Write, os::unix::{fs::PermissionsExt, net::UnixDatagram}, path::PathBuf, sync::{Arc, Mutex}, time::{Duration, Instant}};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CONFIG: &str = "/etc/netplan/30-microduck-wifi.yaml";
fn public_error(error:&anyhow::Error) -> String {
    // Do not serialize parser/I/O errors: YAML errors can quote credential values.
    let text=error.to_string();
    let allowed=["网络名称格式错误","网络名称需为 1–32 字节，且不能包含控制字符","Wi-Fi 操作未成功，请稍后重试","主板 Wi-Fi 配置结构不兼容，需要维护","保存网络失败，原配置已保留","不支持的 Wi-Fi 操作","密码格式错误","暂不支持该加密方式","Wi-Fi 密码应为 8–63 位英文、数字或符号","此网络尚未保存","请先切换或断开此网络，再忘记","此网络认证方式暂不支持","创建网络失败","连接超时，请检查密码、信号或路由器","最多保存 24 个网络，请先忘记不用的网络","正在切换网络，请等待结果"];
    if allowed.contains(&text.as_str()) {text} else {"Wi-Fi 服务暂不可用".into()}
}
fn ssid(v: &Value) -> Result<String> {
    let s = v.as_str().context("网络名称格式错误")?;
    if s.is_empty() || s.len()>32 || s.chars().any(|c| (c as u32)<32) { bail!("网络名称需为 1–32 字节，且不能包含控制字符"); }
    Ok(s.to_owned())
}
fn decode(s: &str) -> String {
    let b=s.as_bytes(); let mut out=Vec::new(); let mut i=0;
    while i<b.len() {
        if b[i]==b'\\' && i+3<b.len() && b[i+1]==b'x' {
            if let Ok(x)=u8::from_str_radix(&String::from_utf8_lossy(&b[i+2..i+4]),16) { out.push(x); i+=4; continue; }
        }
        if b[i]==b'\\' && i+1<b.len() && matches!(b[i+1],b'\\'|b'"') { i+=1; }
        out.push(b[i]); i+=1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn wpa(command: &str) -> Result<String> {
    let path=PathBuf::from(format!("/run/ducklink-wifi/wpa-{}",uuid::Uuid::new_v4()));
    wpa_at(command, &path, std::path::Path::new("/run/wpa_supplicant/wlan0"))
}
fn wpa_at(command: &str, path:&std::path::Path, endpoint:&std::path::Path) -> Result<String> {
    struct Cleanup(PathBuf); impl Drop for Cleanup {fn drop(&mut self) {let _=fs::remove_file(&self.0);}}
    let _cleanup=Cleanup(path.to_owned());
    let socket=UnixDatagram::bind(&path)?; socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?; socket.connect(endpoint)?;
    socket.send(command.as_bytes())?; let mut buf=[0;65536]; let n=socket.recv(&mut buf)?;
    let result=String::from_utf8_lossy(&buf[..n]).trim().to_owned();
    if result.starts_with("FAIL") || result.starts_with("UNKNOWN") {bail!("Wi-Fi 操作未成功，请稍后重试");} Ok(result)
}
fn config() -> Result<Yaml> {
    let data:Yaml=serde_yaml::from_slice(&fs::read(CONFIG)?)?;
    if !data["network"]["wifis"]["wlan0"]["access-points"].is_mapping() {bail!("主板 Wi-Fi 配置结构不兼容，需要维护");} Ok(data)
}
fn saved(data:&Yaml) -> &serde_yaml::Mapping {data["network"]["wifis"]["wlan0"]["access-points"].as_mapping().unwrap()}
fn status() -> Result<Value> {
    let text=wpa("STATUS")?;
    let fields:BTreeMap<_,_>=text.lines().filter_map(|s|s.split_once('=')).collect();
    Ok(json!({"connected":fields.get("wpa_state")==Some(&"COMPLETED"),"ssid":decode(fields.get("ssid").copied().unwrap_or("")),"ip":fields.get("ip_address").unwrap_or(&""),"state":fields.get("wpa_state").unwrap_or(&"UNKNOWN")}))
}
fn networks() -> Result<Vec<(String,String)>> {
    Ok(wpa("LIST_NETWORKS")?.lines().skip(1).filter_map(|s|{let f:Vec<_>=s.split('\t').collect(); if f.len()>=4 && f[0].parse::<u32>().is_ok() {Some((f[0].into(),decode(f[1])))} else {None}}).collect())
}
async fn generate(executable:&str) -> Result<()> {
    let mut cmd=tokio::process::Command::new(executable); cmd.arg("generate").kill_on_drop(true);
    let out=tokio::time::timeout(Duration::from_secs(10),cmd.output()).await??;
    if !out.status.success() {bail!("保存网络失败，原配置已保留");} Ok(())
}
fn atomic(config:&std::path::Path, raw:&[u8]) -> Result<()> {
    let path=config.with_extension("ducklink-tmp");
    use std::os::unix::fs::OpenOptionsExt;
    let mut f=fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)?;
    f.set_permissions(fs::Permissions::from_mode(0o600))?; f.write_all(raw)?; f.sync_all()?; fs::rename(path,config)?;
    fs::File::open(config.parent().context("config directory missing")?)?.sync_all()?; Ok(())
}
async fn persist(data:&Yaml) -> Result<()> {
    persist_at(data,std::path::Path::new(CONFIG),"/usr/sbin/netplan").await
}
async fn persist_at(data:&Yaml,path:&std::path::Path,generator:&str) -> Result<()> {
    let old=fs::read(path)?; atomic(path,serde_yaml::to_string(data)?.as_bytes())?;
    if generate(generator).await.is_err() {atomic(path,&old)?; let _=generate(generator).await; bail!("保存网络失败，原配置已保留");} Ok(())
}
#[derive(Clone)] struct Job { busy:bool, value:Value, scan_until:Instant }
fn view(job:&Arc<Mutex<Job>>) -> Result<Value> {
    let data=config()?; let aps=saved(&data); let mut seen:BTreeMap<String,Value>=BTreeMap::new();
    for line in wpa("SCAN_RESULTS")?.lines().skip(1) {
        let f:Vec<_>=line.splitn(5,'\t').collect(); if f.len()!=5 {continue;}
        let name=decode(f[4]); if name.is_empty() || name.len()>32 {continue;}
        let personal=f[3].contains("PSK"); let secured=["WPA","RSN","WEP","SAE","OWE","EAP"].iter().any(|s|f[3].contains(s));
        let signal=f[2].parse::<i32>().unwrap_or(-100);
        let row=json!({"ssid":name,"signal":signal,"security":if personal {"WPA/WPA2"} else if secured {"不支持的加密"} else {"开放网络"},"secured":secured,"supported":personal || !secured,"saved":aps.contains_key(Yaml::String(name.clone()))});
        if seen.get(&name).and_then(|v|v["signal"].as_i64()).unwrap_or(-1000)<signal as i64 {seen.insert(name,row);}
    }
    let mut rows:Vec<_>=seen.into_values().collect(); rows.sort_by_key(|v|-v["signal"].as_i64().unwrap_or(-100)); rows.truncate(18);
    let j=job.lock().unwrap();
    Ok(json!({"current":status()?,"networks":rows,"saved":aps.keys().filter_map(|v|v.as_str()).take(24).collect::<Vec<_>>(),"job":j.value,"scanning":Instant::now()<j.scan_until}))
}
struct Request {op:String, name:String, password:String, security:String, use_saved:bool}
fn validate(body:&Value) -> Result<Request> {
    let op=body["op"].as_str().unwrap_or(""); if !matches!(op,"connect"|"forget"|"disconnect") {bail!("不支持的 Wi-Fi 操作");}
    let name=if op=="disconnect" {String::new()} else {ssid(&body["ssid"])?};
    let password=body.get("password").unwrap_or(&Value::Null); let password=if password.is_null() && body.get("password").is_none() {""} else {password.as_str().context("密码格式错误")?};
    let security=body["security"].as_str().unwrap_or("WPA/WPA2"); let use_saved=body["saved"]==true;
    if op=="connect" && !use_saved {
        if !matches!(security,"WPA/WPA2"|"开放网络") {bail!("暂不支持该加密方式");}
        if security!="开放网络" && (!(8..=63).contains(&password.len()) || !password.bytes().all(|b|(32..=126).contains(&b))) {bail!("Wi-Fi 密码应为 8–63 位英文、数字或符号");}
    }
    Ok(Request{op:op.into(),name,password:password.into(),security:security.into(),use_saved})
}
async fn work(mut r:Request) -> Result<String> {
    let mut data=config()?; let key=Yaml::String(r.name.clone());
    let mut new_id:Option<String>=None; let mut old_id=None; let mut rollback=false;
    let result:Result<String>=async {
        match r.op.as_str() {
            "disconnect" => {wpa("DISCONNECT")?; Ok("已断开 Wi-Fi，蓝牙仍可管理网络".into())},
            "forget" => {
                data["network"]["wifis"]["wlan0"]["access-points"].as_mapping_mut().unwrap().remove(&key);
                persist(&data).await?; for (number,name) in networks()? {if name==r.name {wpa(&format!("REMOVE_NETWORK {number}"))?;}} Ok("已忘记网络".into())
            },
            _ => {
                let old=status()?; old_id=networks()?.into_iter().find(|(_,name)|Some(name.as_str())==old["ssid"].as_str()).map(|v|v.0);
                if r.use_saved {
                    let entry=&saved(&data)[&key]; if entry.get("auth").is_some() {bail!("此网络认证方式暂不支持");}
                    r.password=entry["password"].as_str().unwrap_or("").into(); r.security=if r.password.is_empty() {"开放网络"} else {"WPA/WPA2"}.into();
                }
                let id=wpa("ADD_NETWORK")?; if id.parse::<u32>().is_err() {bail!("创建网络失败");} new_id=Some(id.clone());
                wpa(&format!("SET_NETWORK {id} ssid {}",hex::encode(r.name.as_bytes())))?;
                wpa(&format!("SET_NETWORK {id} scan_ssid 1"))?;
                if r.security=="开放网络" {wpa(&format!("SET_NETWORK {id} key_mgmt NONE"))?;} else {
                    let psk=if r.password.len()==64 && r.password.bytes().all(|b|b.is_ascii_hexdigit()) {r.password.clone()} else {let mut out=[0;32]; pbkdf2::pbkdf2_hmac::<sha1::Sha1>(r.password.as_bytes(),r.name.as_bytes(),4096,&mut out);hex::encode(out)};
                    wpa(&format!("SET_NETWORK {id} key_mgmt WPA-PSK"))?; wpa(&format!("SET_NETWORK {id} psk {psk}"))?;
                }
                rollback=true; wpa(&format!("SELECT_NETWORK {id}"))?; let deadline=Instant::now()+Duration::from_secs(40);
                loop {let s=status()?; if s["connected"]==true && s["ssid"]==r.name && s["ip"].as_str().is_some_and(|v|!v.is_empty()) {break;} if Instant::now()>=deadline {bail!("连接超时，请检查密码、信号或路由器");} tokio::time::sleep(Duration::from_secs(1)).await;}
                if !r.use_saved {
                    let aps=data["network"]["wifis"]["wlan0"]["access-points"].as_mapping_mut().unwrap();
                    let mut entry=aps.get(&key).and_then(Yaml::as_mapping).cloned().unwrap_or_default(); entry.remove(Yaml::String("auth".into()));
                    if r.security!="开放网络" {entry.insert(Yaml::String("password".into()),Yaml::String(r.password.clone()));} else {entry.remove(Yaml::String("password".into()));}
                    aps.insert(key.clone(),Yaml::Mapping(entry)); if aps.len()>24 {bail!("最多保存 24 个网络，请先忘记不用的网络");} persist(&data).await?;
                }
                for (number,name) in networks()? {if name==r.name && number!=id {wpa(&format!("REMOVE_NETWORK {number}"))?;}}
                wpa("ENABLE_NETWORK all")?; rollback=false; Ok("已连接并保存网络".into())
            }
        }
    }.await;
    if result.is_err() {if let Some(id)=new_id {let _=wpa(&format!("REMOVE_NETWORK {id}")); if rollback {let _=wpa(&old_id.map(|id|format!("SELECT_NETWORK {id}")).unwrap_or("ENABLE_NETWORK all".into())); let _=wpa("RECONNECT");}}}
    result
}
fn request(body:Value,job:Arc<Mutex<Job>>) -> Result<Value> {
    if body["op"]=="status" {return view(&job);}
    let mut j=job.lock().unwrap(); if j.busy {bail!("正在切换网络，请等待结果");}
    if body["op"]=="scan" {if Instant::now()>=j.scan_until {wpa("SCAN")?; j.scan_until=Instant::now()+Duration::from_secs(6);} return Ok(json!({"accepted":true}));}
    let r=validate(&body)?; let data=config()?; let aps=saved(&data);
    if (r.use_saved || r.op=="forget") && !aps.contains_key(Yaml::String(r.name.clone())) {bail!("此网络尚未保存");}
    if r.op=="forget" {let s=status()?; if s["ssid"]==r.name && s["connected"]==true {bail!("请先切换或断开此网络，再忘记");}}
    j.busy=true; j.value=json!({"state":"running","message":if r.op=="connect" {"正在连接网络…"} else {"正在处理…"},"ssid":r.name}); drop(j);
    tokio::spawn(async move {let name=r.name.clone(); let result=work(r).await; let mut j=job.lock().unwrap(); j.busy=false;
        j.value=match result {Ok(msg)=>json!({"state":"success","message":msg,"ssid":name}),Err(error)=>json!({"state":"failed","message":public_error(&error),"ssid":name})};});
    Ok(json!({"accepted":true}))
}
fn uid(user:&str) -> Result<u32> {let out=std::process::Command::new("/usr/bin/id").args(["-u",user]).output()?; if !out.status.success() {bail!("unknown control user");} Ok(std::str::from_utf8(&out.stdout)?.trim().parse()?)}
#[tokio::main]
async fn main() -> Result<()> {
    let legacy=uid("ducklink")?; let control=uid(&std::env::var("DUCKLINK_CONTROL_USER").unwrap_or("ducklink".into()))?;
    let group=std::process::Command::new("/usr/bin/id").args(["-g","ducklink"]).output()?; let gid:u32=std::str::from_utf8(&group.stdout)?.trim().parse()?;
    let path="/run/ducklink-wifi/control.sock"; let _=fs::remove_file(path);
    let server=tokio::net::UnixListener::bind(path)?; fs::set_permissions(path,fs::Permissions::from_mode(0o660))?;
    let c=std::ffi::CString::new(path)?; if unsafe{libc::chown(c.as_ptr(),0,gid)}!=0 {bail!("socket ownership failed");}
    if status().is_ok_and(|s|s["connected"]!=true) {let _=wpa("RECONFIGURE");let _=wpa("RECONNECT");}
    let job=Arc::new(Mutex::new(Job{busy:false,value:json!({"state":"idle","message":""}),scan_until:Instant::now()}));
    loop {let (mut client,_)=server.accept().await?; let peer=client.peer_cred()?.uid(); if ![0,legacy,control].contains(&peer) {continue;}
        let job=job.clone(); tokio::spawn(async move {
            let mut buf=Vec::new(); let read=tokio::time::timeout(Duration::from_secs(2),async {while buf.len()<2048 {let b=client.read_u8().await?;buf.push(b);if b==b'\n' {break;}} Ok::<_,std::io::Error>(())}).await;
            let reply=if matches!(read,Ok(Ok(()))) {match serde_json::from_slice::<Value>(&buf).context("无效请求").and_then(|body|request(body,job)) {Ok(v)=>v,Err(error)=>json!({"error":public_error(&error)})}} else {json!({"error":"无效请求"})};
            let _=tokio::time::timeout(Duration::from_secs(2),client.write_all(format!("{reply}\n").as_bytes())).await;
        });
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn ssid_encoding() {assert_eq!(decode(r"a\x20\xe4\xb8\xad"),"a 中"); assert!(ssid(&json!("a\n")).is_err()); assert!(ssid(&json!("中".repeat(11))).is_err());}
    #[test] fn credentials_validation() {assert!(validate(&json!({"op":"connect","ssid":"test","password":"abcdefgh"})).is_ok()); for v in [json!({"op":"connect","ssid":"test","password":"short"}),json!({"op":"connect","ssid":"test","password":true}),json!({"op":"connect","ssid":"test","security":"EAP"}),json!({"op":"shell"})] {assert!(validate(&v).is_err());}}
    #[test] fn standard_psk_vector() {let mut out=[0;32];pbkdf2::pbkdf2_hmac::<sha1::Sha1>(b"password",b"IEEE",4096,&mut out);assert_eq!(hex::encode(out),"f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e");}
    #[test] fn errors_do_not_expose_yaml_secrets() {assert_eq!(public_error(&anyhow::anyhow!("password: secret-value")),"Wi-Fi 服务暂不可用");assert_eq!(public_error(&anyhow::anyhow!("密码格式错误")),"密码格式错误");}
    #[test] fn wpa_transport_and_cleanup() {
        let tmp=tempfile::tempdir().unwrap();let endpoint=tmp.path().join("server");let local=tmp.path().join("client");
        let server=UnixDatagram::bind(&endpoint).unwrap();
        let thread=std::thread::spawn(move || {let mut buf=[0;64];let(n,peer)=server.recv_from(&mut buf).unwrap();assert_eq!(&buf[..n],b"STATUS");server.send_to(b"wpa_state=COMPLETED\n",peer.as_pathname().unwrap()).unwrap();});
        assert_eq!(wpa_at("STATUS",&local,&endpoint).unwrap(),"wpa_state=COMPLETED");thread.join().unwrap();assert!(!local.exists());
    }
    #[tokio::test] async fn failed_generate_restores_bytes_and_permissions() {
        let tmp=tempfile::tempdir().unwrap();let path=tmp.path().join("wifi.yaml");let old=b"# keep original\nnetwork: {}\n";fs::write(&path,old).unwrap();
        let data:Yaml=serde_yaml::from_str("network: {version: 2}").unwrap();
        assert!(persist_at(&data,&path,"/bin/false").await.is_err());assert_eq!(fs::read(&path).unwrap(),old);assert_eq!(fs::metadata(&path).unwrap().permissions().mode()&0o777,0o600);
        persist_at(&data,&path,"/bin/true").await.unwrap();assert_eq!(serde_yaml::from_slice::<Yaml>(&fs::read(&path).unwrap()).unwrap(),data);
    }
}
