//! Opt-in camera capture owned by Observer. No second server or interpreter.
use axum::{body::{Body, Bytes}, extract::State, http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response}, routing::get, Json, Router};
use serde_json::{json, Value};
use std::{path::{Path, PathBuf}, process::Stdio, sync::{Arc, OnceLock,
    atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use tokio::{io::AsyncReadExt, process::{Child, ChildStdout, Command},
    sync::{Mutex, Semaphore, oneshot, watch}, task::JoinHandle};
use crate::{App, check_write, error};

const MAX_FRAME: usize = 1024 * 1024;
static CAMERA: OnceLock<Arc<Camera>> = OnceLock::new();

#[derive(Clone)]
struct Frame {
    enabled: bool,
    jpeg: Bytes,
    seq: u64,
    at: Instant,
    device: String,
    error: String,
    capture_pid: u32,
    encoder_pid: u32,
}
impl Default for Frame {
    fn default() -> Self { Self { enabled:false, jpeg:Bytes::new(), seq:0,
        at:Instant::now(), device:String::new(), error:String::new(), capture_pid:0, encoder_pid:0 } }
}
struct Worker { cancel: oneshot::Sender<()>, task: JoinHandle<()> }
struct Camera {
    path: PathBuf,
    available: bool,
    closing: AtomicBool,
    preview: AtomicBool,
    vision: AtomicBool,
    managed: AtomicBool,
    worker: Mutex<Option<Worker>>,
    state: watch::Sender<Frame>,
    viewers: Arc<Semaphore>,
}

fn parse_enabled(value: &Value) -> anyhow::Result<bool> {
    anyhow::ensure!(value.as_object().is_some_and(|v|v.len()==1), "Expected only enabled boolean");
    value["enabled"].as_bool().ok_or_else(||anyhow::anyhow!("Expected enabled boolean"))
}
fn read_enabled(path: &Path) -> anyhow::Result<bool> {
    match std::fs::read(path) {
        Ok(bytes) => parse_enabled(&serde_json::from_slice(&bytes)?),
        Err(e) if e.kind()==std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn save_enabled(path: &Path, enabled: bool) -> anyhow::Result<()> {
    use std::io::Write;
    let parent=path.parent().ok_or_else(||anyhow::anyhow!("Invalid camera state path"))?;
    std::fs::create_dir_all(parent)?;
    let temp=path.with_extension("tmp");
    let mut file=std::fs::File::create(&temp)?;
    writeln!(file,"{}",json!({"enabled":enabled}))?;
    file.sync_all()?;
    std::fs::rename(temp,path)?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

impl Camera {
    fn new(path: PathBuf, available: bool) -> Self {
        let mut frame=Frame::default();
        match read_enabled(&path) {
            Ok(enabled) => frame.enabled=available && enabled,
            Err(e) => { frame.error=format!("摄像头设置读取失败：{e}"); eprintln!("{}",frame.error); }
        }
        let preview=AtomicBool::new(frame.enabled);
        let (state,_)=watch::channel(frame);
        Self { path,available,closing:AtomicBool::new(false),worker:Mutex::new(None),
            preview,vision:AtomicBool::new(false),managed:AtomicBool::new(false),state,viewers:Arc::new(Semaphore::new(8)) }
    }
    fn status(&self) -> Value {
        let f=self.state.borrow();
        json!({"available":self.available,"enabled":self.preview.load(Ordering::Acquire),"captureEnabled":f.enabled,"autonomousUse":self.vision.load(Ordering::Acquire),"autonomousManaged":self.managed.load(Ordering::Acquire),
            "streaming":!f.jpeg.is_empty() && f.at.elapsed()<Duration::from_secs(5),
            "error":f.error,"device":f.device,"transport":"mjpeg","width":640,"height":480,
            "targetFps":10.5,"sequence":f.seq,"capturePid":f.capture_pid,"encoderPid":f.encoder_pid,
            "encoder":"rockchip-mpp","processing":"rga+fixed-wb"})
    }
    async fn stop_worker(worker: &mut Option<Worker>) {
        if let Some(w)=worker.take() {
            let _=w.cancel.send(());
            // Capture task reaps both children before acknowledging shutdown.
            if let Err(e)=w.task.await { eprintln!("camera worker ended unexpectedly: {e}"); }
        }
    }
    fn spawn_worker(&self) -> Worker {
        let (cancel,rx)=oneshot::channel();
        let state=self.state.clone();
        Worker { cancel,task:tokio::spawn(async move { capture(state,rx).await; }) }
    }
    async fn set(&self, enabled: bool) -> anyhow::Result<Value> {
        let mut worker=self.worker.lock().await;
        anyhow::ensure!(self.available && !self.closing.load(Ordering::Acquire),"摄像头控制不可用");
        let path=self.path.clone();
        tokio::task::spawn_blocking(move || save_enabled(&path,enabled)).await??;
        self.preview.store(enabled,Ordering::Release);
        if !capture_needed(enabled,self.vision.load(Ordering::Acquire),self.managed.load(Ordering::Acquire)) {
            Self::stop_worker(&mut worker).await;
            self.state.send_modify(|s|{s.enabled=false;s.error.clear();s.jpeg=Bytes::new();});
        } else {
            if worker.as_ref().is_some_and(|w|w.task.is_finished()) { Self::stop_worker(&mut worker).await; }
            self.state.send_modify(|s|{s.enabled=true;});
            if worker.is_none() { *worker=Some(self.spawn_worker()); }
        }
        Ok(self.status())
    }
    async fn shutdown(&self) {
        self.closing.store(true,Ordering::Release);
        Self::stop_worker(&mut *self.worker.lock().await).await;
        self.state.send_modify(|s|s.jpeg=Bytes::new());
    }
}

pub async fn init(hardware: bool) {
    let available=hardware && std::env::var("MICRODUCK_CAMERA_CONTROL").as_deref()==Ok("1");
    let path=std::env::var_os("MICRODUCK_CAMERA_SETTINGS").map(PathBuf::from)
        .unwrap_or_else(||PathBuf::from("/var/lib/microduck-observer/camera.json"));
    let camera=Arc::new(Camera::new(path,available));
    if camera.state.borrow().enabled { *camera.worker.lock().await=Some(camera.spawn_worker()); }
    assert!(CAMERA.set(camera).is_ok(),"Camera already initialized");
}
pub fn available() -> bool { CAMERA.get().is_some_and(|c|c.available) }
// A temporary consumer must never change the user's persisted preview preference.
pub async fn vision_lease(enabled:bool,managed:bool)->anyhow::Result<()> {
    let Some(c)=CAMERA.get() else {anyhow::ensure!(!enabled,"摄像头未初始化");return Ok(());};
    lease_managed(c,enabled,managed).await
}
fn capture_needed(preview:bool,vision:bool,managed:bool)->bool {vision || (preview && !managed)}
#[cfg(test)]
async fn lease(c:&Camera,enabled:bool)->anyhow::Result<()> {lease_managed(c,enabled,false).await}
async fn lease_managed(c:&Camera,enabled:bool,managed:bool)->anyhow::Result<()> {
    if !enabled && !c.vision.load(Ordering::Acquire) && c.managed.load(Ordering::Acquire)==managed{return Ok(());}
    if c.closing.load(Ordering::Acquire){anyhow::ensure!(!enabled,"摄像头已关闭");return Ok(());}
    anyhow::ensure!(!enabled || c.available,"摄像头不可用");
    let mut worker=c.worker.lock().await;
    let changed=c.vision.swap(enabled,Ordering::AcqRel)!=enabled;
    let changed=(c.managed.swap(managed,Ordering::AcqRel)!=managed) || changed;
    let active=capture_needed(c.preview.load(Ordering::Acquire),enabled,managed);
    if !active {Camera::stop_worker(&mut worker).await;c.state.send_modify(|s|{s.enabled=false;s.jpeg=Bytes::new();});}
    else if changed {if worker.as_ref().is_some_and(|w|w.task.is_finished()){Camera::stop_worker(&mut worker).await;}
        c.state.send_modify(|s|s.enabled=true);if worker.is_none(){*worker=Some(c.spawn_worker());}}
    if enabled {let frame=c.state.borrow();anyhow::ensure!(frame.error.is_empty(),"{}",frame.error);}
    Ok(())
}
pub async fn shutdown() { if let Some(c)=CAMERA.get() { c.shutdown().await; } }

pub fn router() -> Router<App> {
    Router::new().route("/api/v1/camera",get(status).post(set))
        .route("/api/v1/camera/stream.mjpg",get(stream))
}
async fn status() -> Response {
    match CAMERA.get() { Some(c)=>Json(c.status()).into_response(),
        None=>error(StatusCode::SERVICE_UNAVAILABLE,"摄像头尚未初始化") }
}
async fn set(State(app):State<App>, headers:HeaderMap, body:Bytes) -> Response {
    let value=match check_write(&headers,&body,256) {Ok(v)=>v,Err(e)=>return e};
    let enabled=match parse_enabled(&value) {Ok(v)=>v,Err(e)=>return error(StatusCode::UNPROCESSABLE_ENTITY,&e.to_string())};
    let Some(c)=CAMERA.get().filter(|c|c.available && app.hardware) else {
        return error(StatusCode::FORBIDDEN,"当前设备未启用摄像头控制");
    };
    match c.set(enabled).await {Ok(v)=>Json(v).into_response(),
        Err(e)=>error(StatusCode::SERVICE_UNAVAILABLE,&format!("摄像头设置保存或应用失败：{e}"))}
}
async fn stream() -> Response {
    let Some(c)=CAMERA.get() else {return error(StatusCode::SERVICE_UNAVAILABLE,"摄像头不可用")};
    if c.status()["streaming"]!=true {return error(StatusCode::SERVICE_UNAVAILABLE,"摄像头未出帧")}
    let permit=match c.viewers.clone().try_acquire_owned() {Ok(p)=>p,
        Err(_)=>return error(StatusCode::SERVICE_UNAVAILABLE,"观看连接已满")};
    let chunks=futures::stream::unfold((c.state.subscribe(),permit,None),|(mut rx,permit,mut last)|async move {
        loop {
            let frame=rx.borrow_and_update().clone();
            if !frame.enabled || frame.jpeg.is_empty() || frame.at.elapsed()>Duration::from_secs(5) {return None}
            if last!=Some(frame.seq) {
                last=Some(frame.seq);
                let mut chunk=format!("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",frame.jpeg.len()).into_bytes();
                chunk.extend_from_slice(&frame.jpeg);chunk.extend_from_slice(b"\r\n");
                return Some((Ok::<_,std::convert::Infallible>(Bytes::from(chunk)),(rx,permit,last)))
            }
            if !matches!(tokio::time::timeout(Duration::from_secs(6),rx.changed()).await,Ok(Ok(()))) {return None}
        }
    });
    ([("content-type","multipart/x-mixed-replace; boundary=frame"),("cache-control","no-store")],
        Body::from_stream(chunks)).into_response()
}

async fn command(program:&str,args:&[&str]) -> anyhow::Result<String> {
    let result=tokio::time::timeout(Duration::from_secs(5),Command::new(program).args(args)
        .kill_on_drop(true).output()).await??;
    anyhow::ensure!(result.status.success(),"{}: {}",program,String::from_utf8_lossy(&result.stderr));
    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}
async fn prepare() -> anyhow::Result<String> {
    let device=std::fs::read_dir("/sys/class/video4linux")?.filter_map(Result::ok)
        .find(|e|e.file_name().to_string_lossy().starts_with("video") &&
            std::fs::read_to_string(e.path().join("name")).is_ok_and(|s|s.trim()=="rkisp_mainpath"))
        .map(|e|format!("/dev/{}",e.file_name().to_string_lossy()))
        .ok_or_else(||anyhow::anyhow!("未识别到摄像头采集设备，请检查排线和设备树"))?;
    let mut selected=None;
    for entry in std::fs::read_dir("/dev")?.filter_map(Result::ok) {
        let name=entry.file_name().to_string_lossy().into_owned();
        if !name.strip_prefix("media").is_some_and(|v|!v.is_empty()&&v.bytes().all(|b|b.is_ascii_digit())) {continue}
        let path=format!("/dev/{name}");
        let topology=command("/usr/bin/media-ctl",&["-d",&path,"-p"]).await?;
        if topology.lines().any(|l|l.split_whitespace().collect::<Vec<_>>()==["model","rkisp0"]) {
            let sensor=topology.lines().find(|l|l.contains(": m")&&l.contains("_imx219 "))
                .and_then(|l|l.split_once(": ")).and_then(|(_,l)|l.split_once(" ("))
                .map(|(name,_)|name.to_owned()).ok_or_else(||anyhow::anyhow!("IMX219未绑定，请检查传感器连接"))?;
            selected=Some((path,sensor));break
        }
    }
    let (media,sensor)=selected.ok_or_else(||anyhow::anyhow!("未发现rkisp0媒体设备"))?;
    for fmt in [format!("\"{sensor}\":0[fmt:SRGGB10_1X10/3280x2464]"),
        "\"rkisp-isp-subdev\":0[fmt:SRGGB10_1X10/3280x2464 crop:(0,0)/3280x2464]".into(),
        "\"rkisp-isp-subdev\":2[fmt:YUYV8_2X8/3280x2464 crop:(0,0)/3280x2464]".into()] {
        command("/usr/bin/media-ctl",&["-d",&media,"--set-v4l2",&fmt]).await?;
    }
    // Keep the entire sensor field of view; ISP performs the hardware downscale.
    command("/usr/bin/v4l2-ctl",&["-d",&device,"--set-fmt-video=width=640,height=480,pixelformat=NV12"]).await?;
    command("/usr/bin/v4l2-ctl",&["-d",&device,"--set-selection=target=crop,left=0,top=0,width=3280,height=2464"]).await?;
    command("/usr/bin/v4l2-ctl",&["-d",&device,"--set-ctrl=exposure=1499,analogue_gain=2048"]).await?;
    Ok(device)
}
async fn stderr_tail(mut pipe:impl tokio::io::AsyncRead+Unpin) -> String {
    let mut tail=Vec::new();let mut buf=[0;1024];
    loop {match pipe.read(&mut buf).await {Ok(0)=>break,Ok(n)=>{tail.extend_from_slice(&buf[..n]);
        if tail.len()>2048 {tail.drain(..tail.len()-2048);}},Err(e)=>{eprintln!("camera stderr: {e}");break}}}
    String::from_utf8_lossy(&tail).trim().to_owned()
}
struct Pipeline { capture:Child, encoder:Child, output:ChildStdout,
    capture_log:JoinHandle<String>, encoder_log:JoinHandle<String> }
impl Pipeline {
    fn spawn(device:&str) -> anyhow::Result<Self> {
        let executable=std::env::current_exe()?;
        let encoder_path=executable.parent().ok_or_else(||anyhow::anyhow!("Invalid executable directory"))?
            .join("microduck-camera-mpp");
        anyhow::ensure!(encoder_path.is_file(),"MPP camera helper is missing: {}",encoder_path.display());
        let mut capture=Command::new("/usr/bin/v4l2-ctl")
            .args(["-d",device,"--stream-mmap=3","--stream-to=-"])
            .stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()?;
        let input:Stdio=capture.stdout.take().unwrap().try_into()?;
        let mut encoder=Command::new(encoder_path)
            .stdin(input).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()?;
        let output=encoder.stdout.take().unwrap();
        let capture_log=tokio::spawn(stderr_tail(capture.stderr.take().unwrap()));
        let encoder_log=tokio::spawn(stderr_tail(encoder.stderr.take().unwrap()));
        Ok(Self {capture,encoder,output,capture_log,encoder_log})
    }
    async fn stop(&mut self) -> String {
        let _=self.encoder.kill().await;let _=self.capture.kill().await;
        let _=self.encoder.wait().await;let _=self.capture.wait().await;
        let a=(&mut self.encoder_log).await.unwrap_or_default();
        let b=(&mut self.capture_log).await.unwrap_or_default();
        if !a.is_empty() {a} else {b}
    }
}
fn extract_frames(buffer:&mut Vec<u8>) -> anyhow::Result<Vec<Bytes>> {
    anyhow::ensure!(buffer.len()<=MAX_FRAME,"Camera frame exceeds size limit");
    let mut frames=Vec::new();
    while let Some(end)=buffer.windows(2).position(|v|v==[0xff,0xd9]) {
        anyhow::ensure!(buffer.starts_with(&[0xff,0xd8]),"Invalid JPEG frame");
        frames.push(Bytes::copy_from_slice(&buffer[..end+2]));buffer.drain(..end+2);
    }
    Ok(frames)
}
async fn capture(state:watch::Sender<Frame>,mut cancel:oneshot::Receiver<()>) {
    state.send_modify(|s|{s.error.clear();s.jpeg=Bytes::new();});
    let device=tokio::select! {biased;_= &mut cancel=>return,result=prepare()=>match result {
        Ok(d)=>d,Err(e)=>{let message=e.to_string();eprintln!("camera prepare: {message}");state.send_modify(|s|s.error=message);return}
    }};
    let mut pipeline=match Pipeline::spawn(&device) {Ok(p)=>p,Err(e)=>{
        eprintln!("camera spawn: {e}");state.send_modify(|s|s.error=e.to_string());return}};
    state.send_modify(|s|{s.device=device.clone();s.capture_pid=pipeline.capture.id().unwrap_or(0);s.encoder_pid=pipeline.encoder.id().unwrap_or(0);});
    let mut buffer=Vec::with_capacity(65536);let mut chunk=[0;16384];let mut last=Instant::now();
    let mut watchdog=tokio::time::interval(Duration::from_secs(2));
    // STREAMON resets full-resolution exposure to 500 lines. Use a different
    // pre-stream value so V4L2 does not skip the post-start write as unchanged.
    let mut exposure_applied=false;
    let failure:Option<String>='capture: loop {
        tokio::select! {biased;
            _=&mut cancel=>break None,
            _=watchdog.tick()=>if last.elapsed()>Duration::from_secs(8) {break Some("摄像头取帧超时，请检查连接后重新开启".into())},
            result=pipeline.output.read(&mut chunk)=>match result {
                Ok(0)=>break Some("摄像头采集进程已退出".into()),
                Err(e)=>break Some(format!("摄像头读取失败：{e}")),
                Ok(n)=>{buffer.extend_from_slice(&chunk[..n]);match extract_frames(&mut buffer) {
                    Ok(frames)=>for frame in frames {
                        if !exposure_applied {
                            if let Err(e)=command("/usr/bin/v4l2-ctl",&["-d",&device,"--set-ctrl=exposure=1500"]).await {
                                break 'capture Some(format!("摄像头曝光设置失败：{e}"));
                            }
                            exposure_applied=true;last=Instant::now();continue;
                        }
                        last=Instant::now();state.send_modify(|s|{s.jpeg=frame;s.seq+=1;s.at=last;});
                    },
                    Err(e)=>break Some(e.to_string()),
                }}
            }
        }
    };
    let tail=pipeline.stop().await;
    state.send_modify(|s|{s.jpeg=Bytes::new();s.capture_pid=0;s.encoder_pid=0;
        if let Some(failure)=failure {s.error=if tail.is_empty(){failure}else{format!("{failure}: {tail}")};eprintln!("camera: {}",s.error);}});
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_off_and_both_choices_persist() {
        let tmp=tempfile::tempdir().unwrap();let p=tmp.path().join("camera.json");
        assert!(!read_enabled(&p).unwrap());
        for choice in [true,false] {save_enabled(&p,choice).unwrap();assert_eq!(read_enabled(&p).unwrap(),choice);}
    }
    #[test]
    fn rejects_ambiguous_requests_and_invalid_saved_state() {
        for v in [json!(null),json!([]),json!({}),json!({"enabled":"false"}),json!({"enabled":1}),json!({"enabled":true,"cmd":"id"})] {
            assert!(parse_enabled(&v).is_err());
        }
        let tmp=tempfile::tempdir().unwrap();let p=tmp.path().join("camera.json");
        std::fs::write(&p,"broken").unwrap();
        let c=Camera::new(p,true);assert_eq!(c.status()["enabled"],false);assert_ne!(c.status()["error"],"");
    }
    #[test]
    fn fragmented_frames_and_size_limits() {
        let mut b=vec![0xff,0xd8,1,2,0xff];assert!(extract_frames(&mut b).unwrap().is_empty());
        b.extend_from_slice(&[0xd9,0xff,0xd8,3,0xff,0xd9]);
        let f=extract_frames(&mut b).unwrap();assert_eq!(f.len(),2);assert!(b.is_empty());
        assert!(extract_frames(&mut vec![1,2,0xff,0xd9]).is_err());
        assert!(extract_frames(&mut vec![0;MAX_FRAME+1]).is_err());
    }
    #[tokio::test]
    async fn shutdown_rejects_future_enable_without_rewriting_settings() {
        let tmp=tempfile::tempdir().unwrap();let p=tmp.path().join("camera.json");save_enabled(&p,true).unwrap();
        let c=Camera::new(p.clone(),true);c.shutdown().await;
        assert!(c.set(false).await.is_err());assert!(read_enabled(&p).unwrap());
    }
    #[tokio::test]
    async fn failed_save_does_not_change_state() {
        let tmp=tempfile::tempdir().unwrap();let p=tmp.path().join("not-a-directory");std::fs::write(&p,"x").unwrap();
        let c=Camera::new(p.join("camera.json"),true);
        assert!(c.set(true).await.is_err());assert_eq!(c.status()["enabled"],false);
    }
    #[tokio::test]
    async fn autonomous_camera_lease_preserves_preview_choice_and_reaps_owned_capture() {
        for preview in [false,true] {
            let tmp=tempfile::tempdir().unwrap();let p=tmp.path().join("camera.json");save_enabled(&p,preview).unwrap();let c=Camera::new(p.clone(),true);
            let (cancel,rx)=oneshot::channel();*c.worker.lock().await=Some(Worker{cancel,task:tokio::spawn(async move{let _=rx.await;})});
            lease(&c,true).await.unwrap();assert_eq!(read_enabled(&p).unwrap(),preview);assert_eq!(c.status()["captureEnabled"],true);assert_eq!(c.status()["enabled"],preview);
            lease(&c,false).await.unwrap();assert_eq!(read_enabled(&p).unwrap(),preview);assert_eq!(c.worker.lock().await.is_some(),preview);
            c.shutdown().await;
        }
    }
    #[tokio::test]
    async fn managed_non_observation_reaps_capture_even_with_preview_and_restores_preference(){
        let tmp=tempfile::tempdir().unwrap();let p=tmp.path().join("camera.json");save_enabled(&p,true).unwrap();let c=Camera::new(p.clone(),true);
        let (cancel,rx)=oneshot::channel();*c.worker.lock().await=Some(Worker{cancel,task:tokio::spawn(async move{let _=rx.await;})});
        lease_managed(&c,true,true).await.unwrap();assert_eq!(c.status()["captureEnabled"],true);
        lease_managed(&c,false,true).await.unwrap();assert!(c.worker.lock().await.is_none());assert_eq!(c.status()["captureEnabled"],false);
        assert!(read_enabled(&p).unwrap());assert_eq!(c.status()["autonomousManaged"],true);
        c.set(true).await.unwrap();assert!(c.worker.lock().await.is_none());assert_eq!(c.status()["captureEnabled"],false);
        // Supply an inert worker so restoring the preview does not touch real hardware.
        let (cancel,rx)=oneshot::channel();*c.worker.lock().await=Some(Worker{cancel,task:tokio::spawn(async move{let _=rx.await;})});
        lease_managed(&c,false,false).await.unwrap();assert_eq!(c.status()["captureEnabled"],true);assert_eq!(c.status()["autonomousManaged"],false);
        assert!(read_enabled(&p).unwrap());c.shutdown().await;
    }
    #[tokio::test]
    async fn camera_absence_does_not_block_managed_navigation(){
        let tmp=tempfile::tempdir().unwrap();let c=Camera::new(tmp.path().join("camera.json"),false);
        lease_managed(&c,false,true).await.unwrap();assert!(c.worker.lock().await.is_none());
        assert!(lease_managed(&c,true,true).await.is_err());lease_managed(&c,false,false).await.unwrap();
        assert!(!capture_needed(true,false,true));assert!(capture_needed(true,false,false));assert!(capture_needed(false,true,true));
    }
    #[tokio::test]
    async fn preview_disable_during_autonomy_is_saved_but_does_not_kill_its_camera() {
        let tmp=tempfile::tempdir().unwrap();let p=tmp.path().join("camera.json");save_enabled(&p,true).unwrap();let c=Camera::new(p.clone(),true);
        let (cancel,rx)=oneshot::channel();*c.worker.lock().await=Some(Worker{cancel,task:tokio::spawn(async move{let _=rx.await;})});
        lease(&c,true).await.unwrap();c.set(false).await.unwrap();assert!(!read_enabled(&p).unwrap());assert!(c.worker.lock().await.is_some());
        lease(&c,false).await.unwrap();assert!(c.worker.lock().await.is_none());assert_eq!(c.status()["captureEnabled"],false);
    }
}
