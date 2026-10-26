//! Opt-in ToF capture. One child owns the ULD and I2C descriptor; disabled means no worker.
use crate::{App,check_write,error,telemetry::Shared};
use axum::{body::Bytes,extract::State,http::{HeaderMap,StatusCode},response::{IntoResponse,Response},routing::get,Json,Router};
use serde_json::{json,Value};
use std::{path::{Path,PathBuf},sync::{Arc,OnceLock},time::{Duration,Instant}};
use tokio::{process::{Child,Command},sync::{Mutex,oneshot},task::JoinHandle};
static TOF:OnceLock<Arc<Tof>>=OnceLock::new();
struct Worker {stop:oneshot::Sender<()>,task:JoinHandle<()>}
struct Tof {path:PathBuf,bus:String,helper:PathBuf,available:bool,shared:Shared,worker:Mutex<Option<Worker>>}
fn parse_enabled(v:&Value)->anyhow::Result<bool> {
    anyhow::ensure!(v.as_object().is_some_and(|o|o.len()==1),"仅接受 enabled 布尔值");
    v["enabled"].as_bool().ok_or_else(||anyhow::anyhow!("enabled 必须为布尔值"))
}
fn save(path:&Path,enabled:bool)->anyhow::Result<()> {
    use std::io::Write;
    let parent=path.parent().ok_or_else(||anyhow::anyhow!("ToF设置路径无效"))?;
    std::fs::create_dir_all(parent)?;
    let temp=path.with_extension("tmp");let mut f=std::fs::File::create(&temp)?;
    writeln!(f,"{}",json!({"enabled":enabled}))?;f.sync_all()?;drop(f);
    std::fs::rename(&temp,path)?;
    #[cfg(unix)] std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
impl Tof {
    fn status(&self)->Value {
        let mut v=self.shared.read().unwrap().tof_health();
        v["available"]=json!(self.available);v["device"]=json!(self.bus);
        v["sensor"]=json!("VL53L5CX");v["address"]=json!("0x29");v["rows"]=json!(8);v["cols"]=json!(8);v["targetHz"]=json!(15);
        v["streaming"]=json!(v["state"]=="streaming");v
    }
    async fn stop(w:&mut Option<Worker>) {
        if let Some(w)=w.take(){let _=w.stop.send(());if let Err(e)=w.task.await {eprintln!("ToF worker join failed: {e}");}}
    }
    async fn set(&self,enabled:bool)->anyhow::Result<Value> {
        let mut worker=self.worker.lock().await;
        anyhow::ensure!(!enabled || self.available,"当前设备未配置ToF驱动");
        save(&self.path,enabled)?;
        if !enabled {
            Self::stop(&mut worker).await;
            let mut s=self.shared.write().unwrap();s.latest.remove("tof");s.tof=json!({"enabled":false,"state":"disabled","error":"","hz":0,"capturePid":0});
        } else if worker.is_none() {
            self.shared.write().unwrap().tof=json!({"enabled":true,"state":"starting","error":"","hz":0,"capturePid":0});
            let (stop,rx)=oneshot::channel();let bus=self.bus.clone();let helper=self.helper.clone();let shared=self.shared.clone();
            *worker=Some(Worker{stop,task:tokio::spawn(capture(helper,bus,shared,rx))});
        }
        Ok(self.status())
    }
}
pub async fn init(hardware:bool,shared:Shared) {
    let helper=std::env::current_exe().unwrap_or_default().with_file_name("microduck-tof");
    let bus=std::env::var("MICRODUCK_TOF_BUS").unwrap_or("/dev/i2c-3".into());
    let path=PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE").unwrap_or("./state/calibration.json".into())).with_file_name("tof.json");
    let saved=match std::fs::read(&path) {Ok(b)=>serde_json::from_slice(&b).map_err(anyhow::Error::from).and_then(|v|parse_enabled(&v)),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(false),Err(e)=>Err(e.into())};
    let available=hardware && cfg!(target_os="linux") && helper.is_file() && Path::new(&bus).exists();
    shared.write().unwrap().tof=json!({"enabled":false,"state":"disabled","error":saved.as_ref().err().map(ToString::to_string).unwrap_or_default(),"hz":0,"capturePid":0});
    let tof=Arc::new(Tof{path,bus,helper,available,shared,worker:Mutex::new(None)});
    if saved.unwrap_or(false) && available {if let Err(e)=tof.set(true).await {tof.shared.write().unwrap().tof["error"]=json!(e.to_string());}}
    assert!(TOF.set(tof).is_ok(),"ToF already initialized");
}
pub async fn shutdown(){if let Some(t)=TOF.get(){Tof::stop(&mut *t.worker.lock().await).await;}}
pub fn router()->Router<App>{Router::new().route("/api/v1/tof",get(status).post(set))}
async fn status()->Response {match TOF.get(){Some(t)=>Json(t.status()).into_response(),None=>error(StatusCode::SERVICE_UNAVAILABLE,"ToF未初始化")}}
async fn set(State(app):State<App>,headers:HeaderMap,body:Bytes)->Response {
    let v=match check_write(&headers,&body,256){Ok(v)=>v,Err(e)=>return e};
    let enabled=match parse_enabled(&v){Ok(v)=>v,Err(e)=>return error(StatusCode::UNPROCESSABLE_ENTITY,&e.to_string())};
    let Some(t)=TOF.get().filter(|_|app.hardware) else {return error(StatusCode::FORBIDDEN,"模拟模式不可控制ToF硬件")};
    match t.set(enabled).await {Ok(v)=>Json(v).into_response(),Err(e)=>error(StatusCode::SERVICE_UNAVAILABLE,&e.to_string())}
}
async fn stop_child(child:&mut Child) {
    #[cfg(unix)] if let Some(id)=child.id(){unsafe{libc::kill(id as i32,libc::SIGTERM);}}
    if !matches!(tokio::time::timeout(Duration::from_secs(8),child.wait()).await,Ok(Ok(_))) {
        if let Err(e)=child.kill().await {eprintln!("ToF child cleanup failed: {e}");}
    }
}
#[cfg(unix)]
async fn read_frames(child:&mut Child,shared:&Shared)->anyhow::Result<()> {
    let mut reader=tokio::io::BufReader::new(child.stdout.take().ok_or_else(||anyhow::anyhow!("ToF stdout unavailable"))?);
    let mut previous=None;let mut times=std::collections::VecDeque::new();
    loop {
        let mut line=vec![];
        let wait=if previous.is_none(){40}else{3};
        let n=tokio::time::timeout(Duration::from_secs(wait),bounded_line(&mut reader,&mut line)).await??;
        anyhow::ensure!(n>0,"ToF采集进程已退出，请检查J5连接和I2C权限");
        let p:Value=serde_json::from_slice(&line)?;validate(&p)?;
        let seq=p["seq"].as_u64().unwrap();let ns=p["t_ns"].as_u64().unwrap();
        anyhow::ensure!(!previous.is_some_and(|(s,t)|seq<=s||ns<=t),"ToF帧序号或时间戳异常");
        times.push_back(ns);if times.len()>30 {times.pop_front();}
        let hz=if times.len()>1{(times.len()-1) as f64*1e9/(ns-times[0]) as f64}else{0.};
        let received=Instant::now();
        let captured=if let Some(ns)=p["captured_mono_ns"].as_u64(){
            let mut clock:libc::timespec=unsafe{std::mem::zeroed()};
            anyhow::ensure!(unsafe{libc::clock_gettime(libc::CLOCK_MONOTONIC,&mut clock)}==0,"ToF主机时钟不可用");
            let now_ns=clock.tv_sec as u64*1_000_000_000+clock.tv_nsec as u64;
            anyhow::ensure!(ns<=now_ns,"ToF采集时间来自未来");
            received.checked_sub(Duration::from_nanos(now_ns-ns))
        }else{None};
        let mut state=shared.write().unwrap();
        state.tof=json!({"enabled":true,"state":"streaming","error":"","hz":hz,"capturePid":child.id()});
        state.sample("tof",json!({"captureClockVerified":captured.is_some(),"rows":8,"cols":8,"distanceMm":p["distance_mm"],"status":p["status"],"sensorSeq":seq,"sensorTimeNs":ns,"hz":hz,"sequenceGap":previous.map(|(s,_)|seq-s-1).unwrap_or(0),"sensor":"VL53L5CX"}),true,captured.unwrap_or(received));
        previous=Some((seq,ns));
    }
}
#[cfg(not(unix))]
async fn read_frames(_: &mut Child,_:&Shared)->anyhow::Result<()>{anyhow::bail!("Linux required")}
async fn capture(helper:PathBuf,bus:String,shared:Shared,mut stop:oneshot::Receiver<()>) {
    let mut backoff=2u64;
    loop {
        let result=Command::new(&helper).arg(&bus).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).kill_on_drop(true).spawn();
        let error=match result {
            Ok(mut child)=>{
                let result=tokio::select!{_=&mut stop=>None,result=read_frames(&mut child,&shared)=>Some(result)};
                stop_child(&mut child).await;
                match result {None=>return,Some(Err(e))=>e.to_string(),Some(Ok(()))=>"ToF采集结束".into()}
            },Err(e)=>e.to_string()
        };
        {let mut s=shared.write().unwrap();s.tof=json!({"enabled":true,"state":"offline","error":error,"hz":0,"capturePid":0,"retrySeconds":backoff});}
        tokio::select!{_=&mut stop=>return,_=tokio::time::sleep(Duration::from_secs(backoff))=>{}}
        backoff=(backoff*2).min(60);
    }
}
#[cfg(unix)]
async fn bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    out: &mut Vec<u8>,
) -> std::io::Result<usize> {
    use tokio::io::AsyncBufReadExt;
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(out.len());
        };
        let end = chunk.iter().position(|c| *c == b'\n').map(|i| i + 1);
        let n = end.unwrap_or(chunk.len());
        if out.len() + n > 65536 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ToF frame exceeds 64KiB",
            ));
        };
        out.extend_from_slice(&chunk[..n]);
        reader.consume(n);
        if end.is_some() {
            return Ok(out.len());
        }
    }
}
fn validate(p: &Value) -> anyhow::Result<()> {
    if p["rows"] != 8 || p["cols"] != 8 {
        anyhow::bail!("Expected an 8x8 ToF frame")
    }
    for (key, max) in [("distance_mm", 65535), ("status", 255)] {
        let a = p[key]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid ToF {key}"))?;
        if a.len() != 64 || a.iter().any(|x| x.as_u64().map_or(true, |x| x > max)) {
            anyhow::bail!("Invalid ToF {key}")
        }
    }
    if p["seq"].as_u64().is_none() || p["t_ns"].as_u64().is_none() {
        anyhow::bail!("Invalid ToF timestamp")
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_are_explicit_and_persist_both_directions() {
        assert_eq!(parse_enabled(&json!({"enabled":false})).unwrap(),false);
        for bad in [json!({}),json!({"enabled":1}),json!({"enabled":true,"bus":"/dev/i2c-0"})] {assert!(parse_enabled(&bad).is_err());}
        let temp=tempfile::tempdir().unwrap();let file=temp.path().join("tof.json");
        for enabled in [true,false] {save(&file,enabled).unwrap();let v:Value=serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();assert_eq!(parse_enabled(&v).unwrap(),enabled);}
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn oversized_frame_is_rejected_without_unbounded_buffering() {
        let input=vec![b'x';65537];let mut reader=tokio::io::BufReader::new(input.as_slice());let mut output=vec![];
        assert!(bounded_line(&mut reader,&mut output).await.is_err());assert!(output.len()<=65536);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn disabling_capture_reaps_child_and_clears_last_frame() {
        use std::os::unix::fs::PermissionsExt;
        let temp=tempfile::tempdir().unwrap();let helper=temp.path().join("fake-sensor");
        std::fs::write(&helper,"#!/bin/sh\nexec sleep 60\n").unwrap();std::fs::set_permissions(&helper,std::fs::Permissions::from_mode(0o700)).unwrap();
        let shared=Arc::new(std::sync::RwLock::new(crate::telemetry::Telemetry::new(true)));
        shared.write().unwrap().sample("tof",json!({}),true,Instant::now());
        let t=Tof{path:temp.path().join("tof.json"),bus:"unused".into(),helper,available:true,shared:shared.clone(),worker:Mutex::new(None)};
        assert_eq!(t.set(true).await.unwrap()["enabled"],true);
        tokio::time::sleep(Duration::from_millis(80)).await;
        let status=tokio::time::timeout(Duration::from_secs(3),t.set(false)).await.unwrap().unwrap();
        assert_eq!(status["enabled"],false);assert_eq!(status["streaming"],false);assert!(t.worker.lock().await.is_none());assert!(!shared.read().unwrap().latest.contains_key("tof"));
        assert_eq!(t.set(false).await.unwrap()["state"],"disabled");
    }
    #[test]
    fn invalid_frames() {
        assert!(validate(&json!({"rows":8,"cols":8,"seq":1,"t_ns":2,"distance_mm":vec![0;64],"status":vec![5;64]})).is_ok());
        assert!(validate(&json!({"rows":8,"cols":8,"seq":1,"t_ns":2,"distance_mm":vec![0;63],"status":vec![5;64]})).is_err());
    }
}
