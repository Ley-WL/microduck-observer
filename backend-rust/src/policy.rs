use crate::{
    control::{self, Feedback},
    orientation,
    servos::Bus,
    telemetry::{epoch_ms, limit, Shared, IDS},
};
use anyhow::{bail, Result};
use nalgebra::Vector3;
use ort::{session::Session, value::Tensor};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub const ORDER: [u8; 14] = [20, 21, 22, 23, 24, 30, 31, 32, 33, 10, 11, 12, 13, 14];
#[derive(Debug)]
struct StaleFeedback(u8);
impl std::fmt::Display for StaleFeedback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{} 反馈过期", self.0)
    }
}
impl std::error::Error for StaleFeedback {}
const IMU_FRESH_MS: f64 = 150.;
const IMU_STOP_MS: f64 = 1000.;
#[derive(Debug)]
struct StaleImu { topic: &'static str, age_ms: f64 }
impl std::fmt::Display for StaleImu {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        write!(f,"IMU 数据缺失或过期（{}，age={}ms）",self.topic,self.age_ms)
    }
}
impl std::error::Error for StaleImu {}
fn imu_samples(shared:&Shared)->Result<BTreeMap<&'static str,Value>> {
    let state=shared.read().unwrap();
    let mut samples=BTreeMap::new();
    let mut stale:Option<StaleImu>=None;
    for topic in ["imu.orientation","imu.raw"] {
        let Some(sample)=state.latest.get(topic) else {stale=Some(StaleImu{topic,age_ms:f64::INFINITY});continue;};
        // Invalid data and session errors remain fatal; only missing/aged samples pause.
        if sample["valid"]!=true || sample["source"]!="hardware" {bail!("IMU 数据无效（{topic}）");}
        let age_ms=state.stamp(sample)["ageMs"].as_f64().unwrap_or(f64::INFINITY);
        if (!age_ms.is_finite() || !(0. ..IMU_FRESH_MS).contains(&age_ms)) && stale.as_ref().is_none_or(|old|age_ms>old.age_ms) {stale=Some(StaleImu{topic,age_ms});}
        samples.insert(topic,sample.clone());
    }
    if samples.len()==2 && samples["imu.orientation"]["bootId"]!=samples["imu.raw"]["bootId"] {bail!("IMU 会话不一致");}
    if let Some(stale)=stale {return Err(stale.into());}
    Ok(samples)
}
#[derive(Default)]
struct ImuPause { since:Option<Instant>, episodes:u64, recoveries:u64, last_age_ms:f64 }
impl ImuPause {
    fn assess(&mut self,result:Result<BTreeMap<&'static str,Value>>,now:Instant)->Result<bool> {
        match result {
            Ok(_) => {if self.since.take().is_some(){self.recoveries+=1;} Ok(true)}
            Err(error) if error.is::<StaleImu>() => {
                let stale=error.downcast_ref::<StaleImu>().unwrap();self.last_age_ms=stale.age_ms;
                let since=*self.since.get_or_insert_with(||{self.episodes+=1;now});
                if (stale.age_ms.is_finite() && stale.age_ms>=IMU_STOP_MS) || now.duration_since(since)>=Duration::from_millis(IMU_STOP_MS as u64) {
                    bail!("IMU 持续中断，停机卸力：{error}");
                }
                Ok(false)
            }
            Err(error)=>Err(error),
        }
    }
}
fn pause_drive(shared:&Shared) {
    let mut state=shared.write().unwrap();
    if let Some(drive)=state.drive.as_mut(){drive.twist=[0.;3];drive.updated=Instant::now();}
    state.pending_skill=None;state.pending_mouth=None;
    state.control["feedbackHolding"]=json!(true);
}
fn check_feedback_age(id: u8, received: f64, now: f64) -> Result<()> {
    if !(0.0..0.15).contains(&(now - received)) {
        return Err(StaleFeedback(id).into());
    }
    Ok(())
}
// Only sample absence/age enters the hold path; invalid data and real faults propagate.
fn observation_or_hold(result: Result<Vec<f32>>) -> Result<Option<Vec<f32>>> {
    match result {
        Ok(obs) => Ok(Some(obs)),
        Err(e) if e.is::<StaleFeedback>() || e.is::<StaleImu>() => Ok(None),
        Err(e) => Err(e),
    }
}
fn history_needs_reset(stamps: &[f64; 14], previous: &[f64; 14]) -> Result<bool> {
    let mut reset = false;
    for n in 0..14 {
        let dt = stamps[n] - previous[n];
        if !dt.is_finite() || dt < 0.0 {
            bail!("#{} 关节反馈时间倒退或无效", ORDER[n]);
        }
        reset |= dt > 0.25;
    }
    Ok(reset)
}
const NAMES: [&str; 14] = [
    "left_hip_yaw",
    "left_hip_roll",
    "left_hip_pitch",
    "left_knee",
    "left_ankle",
    "neck_pitch",
    "head_pitch",
    "head_yaw",
    "head_roll",
    "right_hip_yaw",
    "right_hip_roll",
    "right_hip_pitch",
    "right_knee",
    "right_ankle",
];
pub fn path() -> PathBuf {
    std::env::var("MICRODUCK_POLICY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../models/hd1910-head-v5.onnx"))
}
pub fn path_for(kind: &str) -> Result<PathBuf> {
    match kind {
        "sitstand_sit" | "sitstand_stand" => Ok(path().with_file_name("alpha_sitstand.onnx")),
        "stand" => Ok(path()),
        "walk" => Ok(path().with_file_name("hd1910-walk-v6-symmetry500.onnx")),
        "xgoduck" => Ok(path().with_file_name("xgoduck_walk.onnx")),
        "xgoduck_getup" | "xgoduck_pick" | "xgoduck_roulade" => Ok(path().with_file_name(format!("{kind}.onnx"))),
        _ => bail!("未知模型类型"),
    }
}
pub fn model_id(kind: &str) -> &'static str {
    match kind { "sitstand_sit" | "sitstand_stand" => "alpha_sitstand", "walk" => "hd1910-walk-v6-symmetry500", "xgoduck" => "xgoduck_walk",
        "xgoduck_getup" => "xgoduck_getup", "xgoduck_pick" => "xgoduck_pick",
        "xgoduck_roulade" => "xgoduck_roulade", _ => "hd1910-head-v5" }
}
pub fn is_xgoduck(kind: &str) -> bool { matches!(kind, "xgoduck" | "xgoduck_getup" | "xgoduck_pick" | "xgoduck_roulade" | "sitstand_sit" | "sitstand_stand") }
pub fn skill_kind(value: &Value) -> Result<&str> {
    match value.as_str() {
        Some("getup") => Ok("xgoduck_getup"), Some("pick") => Ok("xgoduck_pick"),
        Some("sit") => Ok("sitstand_sit"), Some("standup") => Ok("sitstand_stand"),
        Some("roulade") => Ok("xgoduck_roulade"), _ => bail!("动作须为getup/pick/roulade/sit/standup"),
    }
}
fn skill_twist(kind: &str, progress: f64) -> [f32;3] {
    if kind == "sitstand_sit" { [1.,0.,0.] } else if kind == "xgoduck_pick" {
        let phase = std::f64::consts::TAU * (progress / 4.).clamp(0., 1.);
        [phase.cos() as f32, phase.sin() as f32, 0.]
    } else { [0.;3] }
}
fn skill_complete(kind: &str, progress: f64, upright: f64) -> bool {
    match kind { "sitstand_stand" => progress >= 1., "xgoduck_pick" => progress >= 4., "xgoduck_roulade" => progress >= 1.9,
        "xgoduck_getup" => upright >= 1., _ => false }
}
fn tilt(obs: &[f32]) -> f64 {
    let norm = obs[3..6].iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt();
    (-(obs[5] as f64) / norm).clamp(-1.,1.).acos().to_degrees()
}
const FALL_TILT_DEG: f64 = 60.;
const FALL_IMMEDIATE_DEG: f64 = 90.;
const FALL_CONFIRM: Duration = Duration::from_millis(150);
fn walking_policy(kind: &str) -> bool { matches!(kind,"xgoduck"|"walk") }
fn body_gravity(sensors:&BTreeMap<&'static str,Value>,cal:&Value)->Result<Vector3<f64>> {
    let imu=&cal["imu"];
    if imu["initialized"]!=true || imu["mountingQuaternion"].is_null() {
        bail!("请先完成 IMU 位置和安装方向标定");
    }
    let mounting=orientation::rotation(&imu["mountingQuaternion"])?;
    let identity = json!([0, 0, 0, 1]);
    let target = orientation::rotation(imu.get("targetQuaternion").unwrap_or(&identity))?;
    let relative = orientation::rotation(&imu["quaternion"])?.transpose()
        * orientation::rotation(&sensors["imu.orientation"]["data"]["quaternion"])?;
    let body = target * mounting.transpose() * relative * mounting;
    let gravity = body.transpose() * Vector3::new(0., 0., -1.);
    Ok(gravity)
}
#[derive(Default)]
struct FallGuard { since:Option<Instant> }
impl FallGuard {
    fn check(&mut self,kind:&str,angle:Option<f64>,now:Instant)->bool {
        let Some(angle)=angle.filter(|a|walking_policy(kind) && a.is_finite() && *a>=FALL_TILT_DEG) else {
            self.since=None;return false;
        };
        let since=*self.since.get_or_insert(now);
        angle>=FALL_IMMEDIATE_DEG || now.duration_since(since)>=FALL_CONFIRM
    }
}
pub fn request_profile(body: &Value) -> Result<(&str, f32)> {
    let kind = match body.get("kind") {
        None => "stand",
        Some(value) => value.as_str().ok_or_else(|| anyhow::anyhow!("模型类型无效"))?,
    };
    path_for(kind)?;
    let speed = match body.get("speed") {
        None => if kind == "walk" { 0.2 } else { 0.0 },
        Some(value) => value.as_f64().ok_or_else(|| anyhow::anyhow!("行走速度无效"))?,
    };
    if !speed.is_finite() || !(0.0..=0.2).contains(&speed) || (kind != "walk" && speed != 0.0) {
        bail!("前进速度应在0–0.2m/s内，站立模型速度必须为0")
    }
    Ok((kind, speed as f32))
}
pub fn drive_twist(body: &Value) -> Result<[f32; 3]> {
    let twist=orientation::vector::<3>(&body["twist"])?;
    for (v,max) in twist.iter().zip([0.2,0.1,0.5]) {
        if !v.is_finite() || v.abs()>max { bail!("行走指令超出范围"); }
    }
    // Native closed-loop simulation: reverse -0.2 and left yaw +0.5 respond
    // weakly, while reverse -0.4 and both yaw signs at 1.0 form motion.
    // Apply once at the shared Web/BLE entry, preserving proportional zero.
    Ok([(if twist[0]<0.0 {twist[0]*2.0} else {twist[0]}) as f32,
        twist[1] as f32,(twist[2]*2.0) as f32])
}
pub fn metadata_for(kind: &str) -> Option<Value> {
    verified_metadata_for(kind).ok()
}
pub fn verify_fixture(path: &std::path::Path) -> Result<Value> {
    let fixture: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let (kind, speed) = request_profile(&fixture)?;
    let mut model = Model::open_for(kind, speed)?;
    let shared = std::sync::Arc::new(std::sync::RwLock::new(crate::telemetry::Telemetry::new(
        true,
    )));
    for key in ["imu.orientation", "imu.raw"] {
        shared.write().unwrap().sample(
            key,
            fixture["sensors"][key]["data"].clone(),
            true,
            Instant::now(),
        );
    }
    let received = crate::telemetry::monotonic();
    let feedback = fixture["feedback"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Invalid fixture"))?
        .iter()
        .map(|(key, row)| {
            let mut row = row.clone();
            row["_receivedMono"] = json!(received);
            Ok((key.parse::<u8>()?, row))
        })
        .collect::<Result<_>>()?;
    let obs = model.observe(&shared, &feedback, &fixture["calibration"])?;
    let expected = orientation::vector::<61>(&fixture["observation"])?;
    let observation_error = obs
        .iter()
        .zip(expected)
        .map(|(a, b)| (*a as f64 - b).abs())
        .reduce(f64::max)
        .unwrap();
    let (action, inference_ms) = model.infer(obs)?;
    let expected = orientation::vector::<14>(&fixture["action"])?;
    let action_error = action
        .iter()
        .zip(expected)
        .map(|(a, b)| (*a as f64 - b).abs())
        .reduce(f64::max)
        .unwrap();
    if observation_error >= 1e-6 || action_error >= 1e-5 {
        bail!("Policy parity failed: observation={observation_error},action={action_error}")
    }
    Ok(
        json!({"ok":true,"observationMaxAbsError":observation_error,"actionMaxAbsError":action_error,"inferenceMs":inference_ms,"policySha256":model.sha,"hardwareAccess":false}),
    )
}
fn verified_metadata_for(kind: &str) -> Result<Value> {
    let p = path_for(kind)?;
    let meta: Value = serde_json::from_slice(&std::fs::read(p.with_extension("metadata.json"))?)?;
    if meta["jointOrder"] != json!(NAMES) || meta["controlHz"] != 50 || meta["actorObservationDim"] != 61 || meta["actionDim"] != 14 {
        bail!("模型关节顺序或频率不匹配")
    }
    if kind == "walk" && meta["task"] != "Mjlab-Walk-Flat-MicroDuck-HD1910" { bail!("行走模型任务不匹配") }
    if kind.starts_with("sitstand_") && (meta["runtimeKind"]!="official_sitstand" || meta["task"]!="Mjlab-SitStand-Flat-MicroDuck") { bail!("官方坐站模型任务不匹配") }
    if is_xgoduck(kind) && !kind.starts_with("sitstand_") && meta["runtimeKind"] != kind && !(kind=="xgoduck" && meta["task"]=="Mjlab-Velocity-Flat-XgoDuck") { bail!("XgoDuck模型任务不匹配") }
    let sha = format!("{:x}", Sha256::digest(std::fs::read(p)?));
    if meta["policySha256"] != sha {
        bail!("模型 SHA256 不匹配")
    }
    orientation::vector::<14>(&meta["homeRadians"])?;
    Ok(meta)
}
/// The same HOME used by policy startup, without inference or IMU access.
pub fn home_targets_for(kind: &str) -> Result<BTreeMap<u8, f64>> {
    let meta = verified_metadata_for(kind)?;
    Ok(ORDER.into_iter().zip(orientation::vector::<14>(&meta["homeRadians"])?
        .map(|v| v as f32 as f64)).collect())
}
fn smooth_action(previous: &[f32; 14], raw: &[f32; 14]) -> [f32; 14] { smooth_skill("xgoduck",previous,raw) }
fn smooth_skill(kind:&str, previous:&[f32;14], raw:&[f32;14]) -> [f32;14] {
    let alpha=if kind=="xgoduck_roulade" {0.15} else {0.45};
    std::array::from_fn(|n|previous[n]*alpha+raw[n]*(1.-alpha))
}
fn arm_current<T: control::Transport>(bus: &mut T, ids: &[u8], feedback: &Feedback, gains:(u8,u8), cancel:&AtomicBool, halt:&AtomicBool) -> Result<()> {
    let guard=|| -> Result<()> {if cancel.load(Ordering::Acquire)||halt.load(Ordering::Acquire) {bail!("启动动作已取消");} Ok(())};
    guard()?;
    let mut positions=BTreeMap::new();
    for id in ids {
        control::validate(*id,feedback.get(id),false)?;
        let position=feedback[id]["position"].as_i64().ok_or_else(||anyhow::anyhow!("位置无效"))? as i32;
        control::encode(position)?;
        let (lo,hi,cap)=control::configuration(bus,*id)?;
        if cap==0 || cap>1000 || (hi>lo && (position<lo || position>hi)) {bail!("#{id} 硬件配置或当前位置不支持使能");}
        positions.insert(*id,position);
    }
    guard()?;
    control::goals(bus,&positions)?;
    control::profiles_with_gains(bus,ids,feedback,gains)?;
    let disabled=ids.iter().filter(|id|feedback[id]["torque"]!=1).map(|id|(*id,vec![1])).collect::<BTreeMap<_,_>>();
    guard()?;
    if !disabled.is_empty() {bus.sync(40,&disabled)?;}
    let enabled=bus.feedback(ids)?;
    for id in ids {
        control::validate(*id,enabled.get(id),true)?;
        if enabled[id]["kpRaw"]!=gains.0 || enabled[id]["kdRaw"]!=gains.1 {bail!("#{id} 临时增益未确认");}
    }
    // Reassert aligned targets last; never put torque-enable after a goal.
    guard()?;
    control::goals(bus,&positions)?;
    Ok(())
}
struct Model {
    kind: String,
    progress: f64,
    upright: f64,
    session: std::sync::Arc<std::sync::Mutex<Session>>,
    command_twist: [f32;3],
    home: [f32; 14],
    sha: String,
    previous: Option<([f64; 14], [f64; 14])>,
    velocity: [f64; 14],
    last_action: [f32; 14],
    filtered_action: [f32; 14],
    filtered_gyro: Option<(Vector3<f64>, f64)>,
    raw: Vec<f64>,
    sent: Vec<f64>,
    saturated: Vec<u8>,
}
fn infer_session(session: &mut Session, obs: Vec<f32>) -> Result<([f32;14],f64)> {
        let start = Instant::now();
        let tensor = Tensor::from_array(([1usize, 61], obs.into_boxed_slice()))?;
        let outputs = session.run(ort::inputs![tensor])?;
        let (_, data) = outputs[0].try_extract_tensor::<f32>()?;
        let action: [f32; 14] = data
            .try_into()
            .map_err(|_| anyhow::anyhow!("模型输出无效"))?;
        if action.iter().any(|v| !v.is_finite()) {
            bail!("模型输出无效")
        };
        Ok((action, start.elapsed().as_secs_f64() * 1000.))
    }
/// UART has one owner. Every 100Hz tick writes the latest target before reading.
fn policy_bus_cycle<T:control::Transport>(bus:&mut T,goals:&BTreeMap<u8,i32>)->Result<(Feedback,Value,f64,f64)> {
    let at=Instant::now();control::goals(bus,goals)?;
    let write_ms=at.elapsed().as_secs_f64()*1000.;
    let at=Instant::now();let (feedback,diagnostics)=control::poll_policy_tick(bus,&IDS);
    Ok((feedback,diagnostics,write_ms,at.elapsed().as_secs_f64()*1000.))
}
type InferenceJob = (u64, Instant, Vec<f32>);
type InferenceReply = (u64, Instant, Vec<f32>, Result<([f32;14],f64)>);
/// One replaceable pending observation and one result; no command backlog.
struct InferenceWorker {
    input: std::sync::Arc<(std::sync::Mutex<Option<InferenceJob>>,std::sync::Condvar)>,
    output: std::sync::Arc<std::sync::Mutex<Option<InferenceReply>>>,
    stop: std::sync::Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl InferenceWorker {
    fn new(session: std::sync::Arc<std::sync::Mutex<Session>>) -> Self {
        Self::with_infer(move |obs| infer_session(&mut session.lock().unwrap(),obs))
    }
    fn with_infer(mut infer: impl FnMut(Vec<f32>)->Result<([f32;14],f64)> + Send + 'static) -> Self {
        let input=std::sync::Arc::new((std::sync::Mutex::new(None::<InferenceJob>),std::sync::Condvar::new()));
        let output=std::sync::Arc::new(std::sync::Mutex::new(None));
        let stop=std::sync::Arc::new(AtomicBool::new(false));
        let (i,o,z)=(input.clone(),output.clone(),stop.clone());
        let thread=std::thread::Builder::new().name("policy-inference".into()).spawn(move || {
            loop {
                let mut slot=i.0.lock().unwrap();
                while slot.is_none() && !z.load(Ordering::Acquire) { slot=i.1.wait(slot).unwrap(); }
                if z.load(Ordering::Acquire) { break; }
                let (generation,at,obs)=slot.take().unwrap();drop(slot);
                let result=infer(obs.clone());
                *o.lock().unwrap()=Some((generation,at,obs,result));
            }
        }).expect("policy inference worker");
        Self { input,output,stop,thread:Some(thread) }
    }
    fn submit(&self,generation:u64,obs:Vec<f32>) {
        *self.input.0.lock().unwrap()=Some((generation,Instant::now(),obs));self.input.1.notify_one();
    }
    fn take(&self)->Option<InferenceReply> { self.output.lock().unwrap().take() }
    fn clear(&self) { self.input.0.lock().unwrap().take();self.output.lock().unwrap().take(); }
}
impl Drop for InferenceWorker {
    fn drop(&mut self) {
        { let _slot=self.input.0.lock().unwrap();self.stop.store(true,Ordering::Release);self.input.1.notify_one(); }
        if let Some(thread)=self.thread.take() { let _=thread.join(); }
    }
}
#[derive(Default)]
struct CycleCosts {
    started: Option<Instant>, reads:u64, writes:u64, inference_results:u64,
    missing_frames:u64, missing_responses:u64, checksum_errors:u64,
    sums: [f64;4], maxima: [f64;4], latest: [f64;4], result_age_ms:f64,
}
impl CycleCosts {
    fn read(&mut self,ms:f64) { self.started.get_or_insert_with(Instant::now);self.reads+=1;self.cost(0,ms); }
    fn cost(&mut self,index:usize,ms:f64) { self.latest[index]=ms;self.sums[index]+=ms;self.maxima[index]=self.maxima[index].max(ms); }
    fn value(&self)->Value {
        let names=["read","observe","write","cycleBusy"];
        let mut v=json!({"inferenceConcurrent":true,"feedbackReads":self.reads,"inferenceResults":self.inference_results,
            "missingFrames":self.missing_frames,"missingResponses":self.missing_responses,"checksumErrors":self.checksum_errors,
            "inferenceActualHz":self.started.map(|at| self.inference_results as f64/at.elapsed().as_secs_f64()),
            "feedbackActualHz":self.started.map(|at| if self.reads>1 {(self.reads-1) as f64/at.elapsed().as_secs_f64()} else {0.}),
            "inferenceResultAgeMs":control::rounded(self.result_age_ms,3)});
        for (i,name) in names.into_iter().enumerate() {
            let n=if i==2 {self.writes} else {self.reads};
            v[name]=json!({"lastMs":control::rounded(self.latest[i],3),"maxMs":control::rounded(self.maxima[i],3),"meanMs":if n>0 {control::rounded(self.sums[i]/n as f64,3)} else {0.}});
        }
        v
    }
}
impl Model {
    fn open() -> Result<Self> { Self::open_for("stand", 0.0) }
    fn open_for(kind: &str, speed: f32) -> Result<Self> {
        let p = path_for(kind)?;
        let meta = verified_metadata_for(kind)?;
        let sha = meta["policySha256"].as_str().unwrap().to_owned();
        let home = orientation::vector::<14>(&meta["homeRadians"])?.map(|v| v as f32);
        // Explicit path: library absence is an API error, never a loader panic.
        let runtime = std::env::var("ORT_DYLIB_PATH")
            .map_err(|_| anyhow::anyhow!("未配置 ORT_DYLIB_PATH 原生推理库"))?;
        if !std::path::Path::new(&runtime).is_file() {
            bail!("ONNX Runtime 原生库不存在")
        }
        ort::init_from(&runtime)?.commit();
        let mut session = Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .commit_from_file(p)?;
        let input = session
            .inputs()
            .first()
            .and_then(|i| i.dtype().tensor_shape())
            .map(|d| d.to_vec())
            .unwrap_or_default();
        let output = session
            .outputs()
            .first()
            .and_then(|i| i.dtype().tensor_shape())
            .map(|d| d.to_vec())
            .unwrap_or_default();
        if input != [1, 61] || output != [1, 14] {
            bail!("模型输入输出维度不匹配")
        }
        for _ in 0..12 {
            let tensor = Tensor::from_array(([1usize,61], vec![0f32;61].into_boxed_slice()))?;
            let outputs = session.run(ort::inputs![tensor])?;
            let (_, values) = outputs[0].try_extract_tensor::<f32>()?;
            if values.len()!=14 || values.iter().any(|value|!value.is_finite()) { bail!("模型预热输出无效") }
        }
        Ok(Self {
            kind: kind.to_owned(),
            progress: 0., upright: 0.,
            session: std::sync::Arc::new(std::sync::Mutex::new(session)),
            command_twist: if kind=="sitstand_sit" {[1.,0.,0.]} else {[speed,0.,0.]},
            home,
            sha,
            previous: None,
            velocity: [0.; 14],
            last_action: [0.; 14],
            filtered_action: [0.; 14],
            filtered_gyro: None,
            raw: vec![],
            sent: vec![],
            saturated: vec![],
        })
    }
    fn observe(&mut self, shared: &Shared, feedback: &Feedback, cal: &Value) -> Result<Vec<f32>> {
        let sensors=imu_samples(shared)?;
        let now=crate::telemetry::monotonic();
        let imu = &cal["imu"];
        if imu["initialized"] != true || imu["mountingQuaternion"].is_null() {
            bail!("请先完成 IMU 位置和安装方向标定")
        }
        let mounting = orientation::rotation(&imu["mountingQuaternion"])?;
        let gravity = body_gravity(&sensors,cal)?;
        let gyro = mounting.transpose()
            * Vector3::from(orientation::vector::<3>(
                &sensors["imu.raw"]["data"]["gyro"],
            )?);
        let mut angles = [0.; 14];
        let mut stamps = [0.; 14];
        for (n, id) in ORDER.iter().enumerate() {
            control::validate(*id, feedback.get(id), false)?;
            let f = &feedback[id];
            let received = f["_receivedMono"]
                .as_f64()
                .ok_or_else(|| anyhow::anyhow!("#{id} 反馈时间缺失"))?;
            check_feedback_age(*id, received, now)?;
            stamps[n] = received;
            let key = id.to_string();
            let reference = cal["joints"]["references"][&key]
                .as_f64()
                .ok_or_else(|| anyhow::anyhow!("#{id} 缺少位置标定"))?;
            let d = cal["joints"]["directions"]
                .get(&key)
                .and_then(Value::as_f64)
                .unwrap_or(-1.);
            if ![-1., 1.].contains(&d) {
                bail!("#{id} 缺少位置标定")
            };
            let raw =
                (f["position"].as_f64().unwrap() - reference) * std::f64::consts::TAU / 4096. * d;
            if is_xgoduck(&self.kind) {
                // Upstream observes calibrated encoder angles directly; XML
                // training ranges are not a feedback stop condition.
                angles[n] = raw;
                continue;
            }
            let (low, high) = limit(*id).unwrap();
            let k0 = ((low.to_radians() - raw) / std::f64::consts::TAU).ceil();
            let k1 = ((high.to_radians() - raw) / std::f64::consts::TAU).floor();
            if k0 != k1 {
                bail!("#{id} 编码位置不在关节角度范围内")
            };
            angles[n] = raw + k0 * std::f64::consts::TAU;
        }
        let reset_history = self.previous.as_ref().map(|(_, times)| history_needs_reset(&stamps, times)).transpose()?.unwrap_or(false);
        if reset_history {
            self.previous = None;
            self.velocity = [0.; 14];
            self.last_action = [0.; 14];
            self.filtered_action = [0.; 14];
            self.filtered_gyro = None;
        }
        let mut velocity = [0.; 14];
        for (n, id) in ORDER.iter().enumerate() {
            let direction = cal["joints"]["directions"].get(id.to_string())
                .and_then(Value::as_f64).unwrap_or(-1.);
            let raw = feedback[id]["velocityRaw"].as_f64()
                .ok_or_else(|| anyhow::anyhow!("#{id} 速度反馈缺失"))?;
            let measured = raw * 50. * std::f64::consts::TAU / 4096. * direction;
            velocity[n] = if let Some((_, times)) = self.previous {
                let dt = stamps[n] - times[n];
                if dt > 1e-6 {
                    let alpha = 0.4f64.powf(dt / 0.01);
                    alpha * self.velocity[n] + (1. - alpha) * measured
                } else { self.velocity[n] }
            } else { measured };
        }
        let gyro = if let Some((old, at)) = self.filtered_gyro {
            let dt = now - at;
            if dt > 0.1 { gyro } else {
                let alpha = 0.5f64.powf(dt.clamp(0.001, 0.1) / 0.01);
                old * alpha + gyro * (1. - alpha)
            }
        } else { gyro };
        self.filtered_gyro = Some((gyro, now));
        let mut obs = Vec::with_capacity(61);
        obs.extend(gyro.iter().map(|v| *v as f32));
        obs.extend(gravity.iter().map(|v| *v as f32));
        obs.extend(
            angles
                .iter()
                .zip(self.home)
                .map(|(a, h)| (a - h as f64) as f32),
        );
        obs.extend(velocity.map(|v| v as f32));
        obs.extend(self.last_action);
        let mut commands = [0f32; 13];
        commands[..3].copy_from_slice(&self.command_twist);
        obs.extend(commands);
        self.previous = Some((angles, stamps));
        self.velocity = velocity;
        if obs.len() != 61 || obs.iter().any(|v| !v.is_finite()) {
            bail!("模型观测无效")
        };
        Ok(obs)
    }
    fn infer(&mut self, obs: Vec<f32>) -> Result<([f32;14],f64)> {
        infer_session(&mut self.session.lock().unwrap(),obs)
    }

    fn targets(
        &mut self,
        action: [f32; 14],
        feedback: &Feedback,
        cal: &Value,
        limits: &BTreeMap<u8, (i32, i32)>,
    ) -> Result<BTreeMap<u8, i32>> {
        self.raw.clear();
        self.sent.clear();
        self.saturated.clear();
        let mut goals = BTreeMap::new();
        for (n, id) in ORDER.iter().enumerate() {
            let raw = (self.home[n] + action[n]) as f64;
            if is_xgoduck(&self.kind) {
                let (goal, sent, saturated) = donor_target(*id, raw, cal, limits[id])?;
                self.raw.push(raw.to_degrees());
                self.sent.push(sent.to_degrees());
                if saturated { self.saturated.push(*id); }
                goals.insert(*id, goal);
                continue;
            }
            let (low, high) = limit(*id).unwrap();
            let angle = raw.clamp(low.to_radians(), high.to_radians());
            self.raw.push(raw.to_degrees());
            self.sent.push(angle.to_degrees());
            if (raw - angle).abs() > 1e-6 {
                self.saturated.push(*id)
            }
            let (lo, hi) = limits[id];
            goals.insert(
                *id,
                control::target(
                    *id,
                    feedback[id]["position"].as_i64().unwrap() as i32,
                    cal,
                    lo,
                    hi,
                    angle,
                    true,
                )?,
            );
        }
        Ok(goals)
    }
}
// Match upstream's direct zero/sign conversion followed by encoder saturation.
// Keep this board's calibrated 4096-step conversion and stricter EEPROM bounds.
fn donor_target(id: u8, angle: f64, cal: &Value, hardware: (i32, i32)) -> Result<(i32, f64, bool)> {
    let key = id.to_string();
    let reference = cal["joints"]["references"][&key].as_f64()
        .filter(|v| v.is_finite()).ok_or_else(|| anyhow::anyhow!("#{id} 缺少位置标定"))?;
    let direction = cal["joints"]["directions"].get(&key).and_then(Value::as_f64).unwrap_or(-1.);
    if !angle.is_finite() || ![-1., 1.].contains(&direction) { bail!("#{id} 模型目标或标定无效"); }
    let (mut low, mut high) = (0, 4095);
    if hardware.1 > hardware.0 { low = low.max(hardware.0); high = high.min(hardware.1); }
    if low > high { bail!("#{id} 舵机硬件限位与编码范围不相交"); }
    let raw = (reference + angle * 4096. / std::f64::consts::TAU * direction).round_ties_even();
    if !raw.is_finite() { bail!("#{id} 模型目标编码无效"); }
    let bounded = raw.clamp(low as f64, high as f64);
    let sent = (bounded - reference) * std::f64::consts::TAU / 4096. * direction;
    Ok((bounded as i32, sent, bounded != raw))
}
pub fn execute(
    bus: &mut Bus,
    command: &Value,
    cancel: &AtomicBool,
    halt: &AtomicBool,
    shared: &Shared,
) -> Result<Value> {
    let (kind, speed) = request_profile(command)?;
    let mut model = Model::open_for(kind, speed)?;
    // Load and warm before any enable/goal write; transitions never load a graph on UART.
    let mut bank = BTreeMap::new();
    if is_xgoduck(kind) && command["shadow"] != true {
        for next in ["xgoduck", "xgoduck_getup", "xgoduck_pick", "xgoduck_roulade", "sitstand_sit", "sitstand_stand"] {
            if next != kind { bank.insert(next.to_owned(), Model::open_for(next, 0.)?); }
        }
    }
    let cal = &command["calibration"];
    let mut attempted = false;
    let mut stage = "preflight";
    let mut timing = control::Timing::default();
    let mut infer_ms = 0.;
    let mut last_obs = None;
    let mut feedback = Feedback::new();
    let mut previous = Feedback::new();
    let mut goals = BTreeMap::new();
    let costs=std::sync::Arc::new(std::sync::Mutex::new(CycleCosts::default()));
    let stats = |model: &Model, timing: &control::Timing, infer_ms: f64| -> Value {
        let mut v = timing.stats();
        v["fallDetection"]=json!({"enabled":walking_policy(&model.kind),"tiltDegrees":FALL_TILT_DEG,"confirmMs":FALL_CONFIRM.as_millis(),"immediateTiltDegrees":FALL_IMMEDIATE_DEG});
        v["commandTargetHz"]=json!(100);
        for (key,value) in json!({"policy":model_id(&model.kind),"kind":if is_xgoduck(kind) {"xgoduck"} else {kind},"requestedKind":kind,"activeSkill":model.kind,"skillProgressSeconds":model.progress,"posture":if model.kind=="sitstand_sit" {if model.progress>=2. {"seated"} else {"sitting"}} else if model.kind=="sitstand_stand" {"rising"} else {"standing"},"walkSpeedMps":speed,"policySha256":model.sha,"inferenceMs":control::rounded(infer_ms,3),"runtimeProfile":"xgoduck-schedule100-v1","homeCommandTargetHz":50,"feedbackReplyBudgetMs":6,"feedbackTargetHz":100,"inferenceTargetHz":50,"actionAlpha":if model.kind=="xgoduck_roulade" {0.15} else {0.45},"velocitySource":"servo-register","velocityAlphaAt100Hz":0.4,"kpRun":control::POLICY_GAINS.0,"kdRun":control::POLICY_GAINS.1,"kpHome":control::MANUAL_GAINS.0,"kdHome":control::MANUAL_GAINS.1,"saturatedIds":model.saturated,"rawTargetDegrees":model.raw,"sentTargetDegrees":model.sent}).as_object().unwrap(){v[key]=value.clone();}
        let s = shared.read().unwrap();
        let ages: BTreeMap<_, _> = ["imu.orientation", "imu.raw"]
            .into_iter()
            .filter_map(|key| {
                s.latest.get(key).map(|sample| {
                    (
                        key,
                        control::rounded(s.stamp(sample)["ageMs"].as_f64().unwrap_or(0.), 2),
                    )
                })
            })
            .collect();
        v["imuAgeMs"] = json!(ages);
        v["commandTwist"] = json!(model.command_twist);
        v["walkSpeedMps"] = json!(if model.kind.starts_with("sitstand_") {0.} else {model.command_twist[0]});
        v["driveSession"] = json!(s.drive.as_ref().map(|d|d.session.as_str()));
        v["cycleCosts"] = costs.lock().unwrap().value();
        v
    };
    let result = (|| -> Result<Value> {
        if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) {
            return Ok(json!({"state":"holding","action":"policy","message":"模型启动已取消"}));
        }
        control::status(
            shared,
            json!({"state":"preflight","action":"policy","policy":model_id(kind),"kind":kind,"message":format!("{}模型与实时传感器预检查",if kind=="walk" {"行走"} else {"站立"})}),
        );
        feedback = bus.read_feedback(&IDS)?;
        let initial_obs = model.observe(shared, &feedback, cal)?;
        if command["shadow"]!=true && walking_policy(kind) && tilt(&initial_obs)>=FALL_TILT_DEG {
            bail!("机身倾角{:.1}°，拒绝启动行走模型；请扶正或主动选择倒地起身",tilt(&initial_obs));
        }
        if matches!(kind,"xgoduck_pick"|"xgoduck_roulade"|"sitstand_sit"|"sitstand_stand") && tilt(&initial_obs)>55. {
            bail!("拾取/翻滚需要先处于直立平衡状态");
        }
        let mut limits = BTreeMap::new();
        for id in ORDER {
            let (low, high, _) = control::configuration(bus, id)?;
            limits.insert(id, (low, high));
        }
        goals = model.targets([0.; 14], &feedback, cal, &limits)?;
        if command["shadow"] == true {
            feedback = bus.read_feedback(&IDS)?;
            let obs = model.observe(shared, &feedback, cal)?;
            let (action, ms) = model.infer(obs)?;
            infer_ms = ms;
            let (compatible, reason) = match model.targets(smooth_skill(&model.kind,&model.filtered_action, &action), &feedback, cal, &limits) {
                Ok(g) => {
                    goals = g;
                    (true, String::new())
                }
                Err(e) => (false, e.to_string()),
            };
            let mut v = stats(&model, &timing, infer_ms);
            v["state"] = json!("shadow");
            v["action"] = json!("policy");
            v["message"] = json!("模型只读推理完成，未发运动指令");
            v["targets"] = json!(goals);
            v["targetDegrees"] = json!((0..14)
                .map(|n| ((model.home[n] + smooth_skill(&model.kind,&model.filtered_action, &action)[n]) as f64).to_degrees())
                .collect::<Vec<_>>());
            v["currentPoseTargetsCompatible"] = json!(compatible);
            v["reason"] = json!(reason);
            return Ok(v);
        }
        let home: BTreeMap<_, _> = ORDER
            .into_iter()
            .zip(model.home.map(|v| v as f64))
            .collect();
        stage = if matches!(kind,"xgoduck_getup"|"sitstand_stand") {"current-pose-enable"} else {"home-transition"};
        attempted = true;
        if matches!(kind,"xgoduck_getup"|"sitstand_stand") {
            // Fallen starts must not use the upright HOME transition.
            arm_current(bus,&ORDER,&feedback,control::MANUAL_GAINS,cancel,halt)?;
        } else {
            control::execute(bus,&json!({"action":"stand","calibration":cal,"standTargets":home}),cancel,shared)?;
        }
        if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) { return control::unload(bus); }
        // HOME completes with the manual gains. Switch only the model joints
        // after arrival; retain enabled state and HOME goals throughout.
        stage = "policy-profile";
        feedback = bus.read_feedback(&IDS)?;
        for id in ORDER { control::validate(id,feedback.get(&id),true)?; }
        control::profiles_with_gains(bus,&ORDER,&feedback,control::POLICY_GAINS)?;
        feedback = bus.read_feedback(&IDS)?;
        for id in ORDER {
            control::validate(id,feedback.get(&id),true)?;
            let (kp,kd)=control::POLICY_GAINS;
            if feedback[&id]["kpRaw"]!=kp || feedback[&id]["kdRaw"]!=kd {
                bail!("#{id} 模型增益设置未确认");
            }
        }
        model.previous = None;
        model.velocity = [0.; 14];
        model.last_action = [0.; 14];
        model.filtered_action = [0.; 14];
        model.filtered_gyro = None;
        goals=if matches!(kind,"xgoduck_getup"|"sitstand_stand") { ORDER.iter().map(|id|(*id,feedback[id]["position"].as_i64().unwrap() as i32)).collect() }
            else {model.targets([0.;14],&feedback,cal,&limits)?};
        let mouth_limits = if is_xgoduck(kind) {Some(control::configuration(bus,34)?)} else {None};
        let mut mouth_owned = false;
        if kind=="xgoduck_pick" { arm_current(bus,&[34],&feedback,control::MANUAL_GAINS,cancel,halt)?; mouth_owned=true; }
        let mut worker=InferenceWorker::new(model.session.clone());
        let mut generation=0u64;
        stage = "policy-loop";
        let mut clock = control::FrameClock::with_period(Instant::now(), Duration::from_millis(10));
        let mut next_inference = Instant::now();
        let mut coast = control::FeedbackCoast::default();
        let mut paused: Option<Instant> = None;
        let mut imu_pause=ImuPause::default();
        let mut fall_guard=FallGuard::default();
        let mut last_status = Instant::now() - Duration::from_secs(1);
        // Bounded timestamp ring keeps long running policies at constant memory.
        while !cancel.load(Ordering::Acquire) && !halt.load(Ordering::Acquire) {
            let cycle_start=Instant::now();
            let imu_result=imu_samples(shared);
            let fall_angle=match &imu_result {
                Ok(sensors)=>Some((-body_gravity(sensors,cal)?.z).clamp(-1.,1.).acos().to_degrees()),
                Err(_)=>None,
            };
            let imu_ready=imu_pause.assess(imu_result,cycle_start)?;
            // Fresh calibrated IMU is checked before accepting results or writing targets.
            if fall_guard.check(&model.kind,fall_angle,cycle_start) {
                stage="policy-fall";
                worker.clear();pause_drive(shared);model.command_twist=[0.;3];
                bus.diagnostics["fallDetection"]=json!({"tiltDegrees":fall_angle,"thresholdDegrees":FALL_TILT_DEG,"confirmMs":FALL_CONFIRM.as_millis(),"immediateThresholdDegrees":FALL_IMMEDIATE_DEG});
                bail!("检测到行走跌倒：机身倾角{:.1}°（≥60°持续150ms或≥90°），停止推理并卸力；扶正后需手动重新启动",fall_angle.unwrap());
            }
            if !imu_ready {
                pause_drive(shared);model.command_twist=[0.;3];
                if paused.is_none() {
                    generation+=1;worker.clear();paused=Some(cycle_start);
                    goals=ORDER.iter().map(|id|(*id,feedback[id]["position"].as_i64().unwrap() as i32)).collect();
                    if mouth_owned {goals.insert(34,feedback[&34]["position"].as_i64().unwrap() as i32);}
                }
            }
            if is_xgoduck(kind) {
                let twist=if model.kind=="xgoduck" {shared.read().unwrap().drive.as_ref().map(|d|d.current()).unwrap_or([0.;3])}
                    else {skill_twist(&model.kind,model.progress)};
                if twist!=model.command_twist {
                    model.command_twist=twist;
                    generation+=1; worker.clear(); next_inference=Instant::now();
                }
            }
            let mut accepted_action=None;
            let now=crate::telemetry::monotonic();
            let feedback_recent=ORDER.iter().all(|id|feedback.get(id).and_then(|r|r["_receivedMono"].as_f64()).is_some_and(|at|(0.0..0.150).contains(&(now-at))));
            if paused.is_none() && feedback_recent {
                if let Some((result_generation,submitted,result_obs,result))=worker.take() {
                    if result_generation==generation && submitted.elapsed()<Duration::from_millis(150) {
                        costs.lock().unwrap().result_age_ms=submitted.elapsed().as_secs_f64()*1000.;
                        let (action,ms)=result?;infer_ms=ms;last_obs=Some(result_obs);
                        let filtered=smooth_skill(&model.kind,&model.filtered_action,&action);
                        if model.kind=="xgoduck_getup" {
                            model.upright=if tilt(last_obs.as_ref().unwrap())<15. {model.upright+0.02} else {0.};
                        }
                        goals=model.targets(filtered,&feedback,cal,&limits)?;
                        accepted_action=Some((action,filtered));
                    }
                }
            }
            if worker.thread.as_ref().is_some_and(|t|t.is_finished()) { bail!("模型推理线程意外退出"); }
            if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) { break; }
            let mouth_request=if paused.is_none() && feedback_recent {shared.write().unwrap().pending_mouth.take()} else {None};
            if let Some(degrees)=mouth_request {
                if model.kind=="xgoduck" {
                    if !mouth_owned {arm_current(bus,&[34],&feedback,control::MANUAL_GAINS,cancel,halt)?;mouth_owned=true;}
                    shared.write().unwrap().mouth_degrees=degrees;
                }
            }
            if mouth_owned && paused.is_none() {
                let degrees = if model.kind=="xgoduck_pick" && model.progress<1.6 {30f64} else if model.kind=="xgoduck" {shared.read().unwrap().mouth_degrees} else {0.};
                let (lo,hi,_) = mouth_limits.unwrap();
                goals.insert(34,control::target(34,feedback[&34]["position"].as_i64().unwrap() as i32,cal,lo,hi,degrees.to_radians(),true)?);
            }
            let write_at=Instant::now();
            let (mut fresh,mut diagnostics,write_ms,read_ms)=policy_bus_cycle(bus,&goals)?;
            let confirmation_at = Instant::now();
            let fault_unconfirmed = control::confirm_policy_faults(&mut fresh, &mut diagnostics, |ids| {
                let rows = bus.read_fast(ids)?;
                Ok((rows, bus.diagnostics.clone()))
            })?;
            // Preserve the original bad read and its confirmation in fault evidence.
            bus.diagnostics = diagnostics.clone();
            let read_ms = read_ms + confirmation_at.elapsed().as_secs_f64()*1000.;
            timing.record(write_at);
            if let Some((action,filtered))=accepted_action {
                model.last_action=action;model.filtered_action=filtered;
                if model.kind!="xgoduck" {model.progress+=0.02;}
                previous=feedback.clone();costs.lock().unwrap().inference_results+=1;
            }
            {
                let mut cost=costs.lock().unwrap();cost.writes+=1;cost.cost(2,write_ms);cost.read(read_ms);
                let missing=IDS.iter().filter(|id|!fresh.contains_key(id)).count() as u64;
                cost.missing_frames+=u64::from(missing>0);cost.missing_responses+=missing;
                cost.checksum_errors+=diagnostics["checksumErrors"].as_u64().unwrap_or(0);
            }
            diagnostics["feedbackTargetHz"] = json!(100);
            // Real faults and bad supply are never hidden by the dropped-read grace.
            for (id, row) in &fresh { control::validate(*id, Some(row), ORDER.contains(id) || (*id==34 && mouth_owned))?; }
            let sampled = if fault_unconfirmed {
                // Do not bridge a possible hardware fault with historical feedback.
                coast.sample(&fresh, &IDS, Instant::now());
                None
            } else { coast.sample(&fresh, &IDS, Instant::now()) };
            bus.diagnostics = coast.diagnostics(diagnostics.clone(), sampled.is_none());
            if sampled.is_some() && paused.is_some_and(|since| since.elapsed() >= Duration::from_millis(200)) {
                model.previous = None;
                model.velocity = [0.; 14];
                model.last_action = [0.; 14];
        model.filtered_action = [0.; 14];
        model.filtered_gyro = None;
            }
            // Check actual per-servo timestamps at observation time, including the
            // few milliseconds between the coast decision and observation.
            let observe_start=Instant::now();
            let obs = match &sampled {
                Some(rows) => {
                    let result=model.observe(shared,rows,cal);
                    if result.as_ref().err().is_some_and(|e|e.is::<StaleImu>()) {
                        let error=result.unwrap_err();
                        imu_pause.assess(Err(error),Instant::now())?;
                        None
                    } else {let observed=observation_or_hold(result)?;if imu_ready {observed}else{None}}
                },
                None => None,
            };
            costs.lock().unwrap().cost(1,observe_start.elapsed().as_secs_f64()*1000.);
            bus.diagnostics = coast.diagnostics(diagnostics, obs.is_none());
            if sampled.is_some() && obs.is_none() {
                bus.diagnostics["feedbackHoldReason"] = json!(if imu_pause.since.is_some(){"imu-expired"}else{"feedback-expired"});
            }
            control::publish(shared, &fresh, &IDS, bus.diagnostics.clone());
            if obs.is_none() {
                model.upright=0.;
                if imu_pause.since.is_some(){pause_drive(shared);model.command_twist=[0.;3];}
                if paused.is_none() {
                    generation+=1; worker.clear();
                    if let Some(last) = &coast.last {
                        goals = ORDER.iter().map(|id| (*id,last[id]["position"].as_i64().unwrap() as i32)).collect();
                    }
                    paused = Some(Instant::now());
                }
                let mut v = stats(&model, &timing, infer_ms);
                v["state"] = json!("policy");
                v["action"] = json!("policy");
                v["policyPhase"] = json!(if imu_pause.since.is_some(){"imu-hold"}else{"feedback-hold"});
                v["imuPauseCount"]=json!(imu_pause.episodes);v["imuRecoveryCount"]=json!(imu_pause.recoveries);
                v["imuHoldAgeMs"]=json!(imu_pause.last_age_ms);
                v["imuFreshLimitMs"]=json!(IMU_FRESH_MS);v["imuStopAgeMs"]=json!(IMU_STOP_MS);
                v["feedbackHolding"] = json!(true);
                v["consecutiveFeedbackFailures"] = json!(coast.misses);
                v["skippedControlTicks"] = json!(clock.skipped);
                v["message"] = json!(if imu_pause.since.is_some(){"IMU 暂时过期，暂停推理并保持姿态；行走归零，等待新鲜数据"}else{"反馈持续缺失，策略暂停，保持最后测得姿态；等待完整反馈恢复"});
                control::status(shared, v);
                costs.lock().unwrap().cost(3,cycle_start.elapsed().as_secs_f64()*1000.);
                clock.wait();
                continue;
            }
            feedback = sampled.unwrap();
            if paused.take().is_some(){
                generation+=1;worker.clear();next_inference=Instant::now();
                let mut state=shared.write().unwrap();state.control["feedbackHolding"]=json!(false);
            }
            let obs = obs.unwrap();
            let now = Instant::now();
            if now >= next_inference {
                let requested = if is_xgoduck(kind) {
                    let mut state=shared.write().unwrap();let pending=state.pending_skill.take();
                    if pending.is_some() {state.control["activeSkill"]=json!("transitioning");}
                    pending
                } else {None};
                let next = requested.or_else(||skill_complete(&model.kind,model.progress,model.upright)
                    .then(||if model.kind!="sitstand_stand" && tilt(&obs)>55. {"xgoduck_getup".to_owned()} else {"xgoduck".to_owned()}));
                if let Some(next) = next.filter(|next|next!=&model.kind) {
                    if next!="xgoduck_getup" && tilt(&obs)>55. { bail!("当前姿态不支持拾取/翻滚或返回平衡"); }
                    shared.write().unwrap().mouth_degrees=0.;
                    if next=="xgoduck_pick" && !mouth_owned {
                        arm_current(bus,&[34],&feedback,control::MANUAL_GAINS,cancel,halt)?; mouth_owned=true;
                    }
                    let mut selected=bank.remove(&next).ok_or_else(||anyhow::anyhow!("动作模型未预加载"))?;
                    selected.previous=model.previous.take();selected.velocity=model.velocity;
                    selected.filtered_gyro=model.filtered_gyro.take();selected.last_action=[0.;14];
                    selected.filtered_action=[0.;14];selected.progress=0.;selected.upright=0.;selected.command_twist=skill_twist(&next,0.);
                    generation+=1;worker.clear();drop(worker);
                    let old=std::mem::replace(&mut model,selected);bank.insert(old.kind.clone(),old);
                    worker=InferenceWorker::new(model.session.clone());
                    let mut state=shared.write().unwrap();
                    if let Some(drive)=state.drive.as_mut() {drive.twist=[0.;3];drive.updated=Instant::now();}
                    // Model-relative angle deltas and previous actions must be rebuilt.
                    drop(state);
                    let transitioned=model.observe(shared,&feedback,cal)?;
                    worker.submit(generation,transitioned);
                    next_inference=now+Duration::from_millis(20);
                    let mut status=stats(&model,&timing,infer_ms);
                    status["state"]=json!("policy");status["action"]=json!("policy");status["feedbackHolding"]=json!(false);
                    status["policyPhase"]=json!("running");control::status(shared,status);
                    clock.wait();
                    continue;
                }
                while next_inference <= now { next_inference += Duration::from_millis(20); }
                worker.submit(generation,obs);
            }
            let tick=Instant::now();
            if last_status.elapsed() >= Duration::from_millis(200) {
                let mut v = stats(&model, &timing, infer_ms);
                v["state"] = json!("policy");
                v["imuPauseCount"]=json!(imu_pause.episodes);v["imuRecoveryCount"]=json!(imu_pause.recoveries);
                v["imuFreshLimitMs"]=json!(IMU_FRESH_MS);v["imuStopAgeMs"]=json!(IMU_STOP_MS);
                v["action"] = json!("policy");
                v["policyPhase"] = json!(if coast.misses > 0 { "coasting" } else { "running" });
                v["feedbackHolding"] = json!(false);
                v["consecutiveFeedbackFailures"] = json!(coast.misses);
                v["skippedControlTicks"] = json!(clock.skipped);
                v["message"] = json!(if model.kind=="sitstand_sit" {"官方坐下模型维持坐姿，点站起恢复行走"} else if model.kind=="sitstand_stand" {"官方站起模型过渡中"} else if model.kind=="xgoduck_getup" {"XgoDuck 起身中，直立稳定后返回零速度平衡"} else if model.kind=="xgoduck_pick" {"XgoDuck 拾取中"} else if model.kind=="xgoduck_roulade" {"XgoDuck 翻滚中"} else if is_xgoduck(kind) { if model.command_twist==[0.;3] {"XgoDuck 原地平衡，按住方向键行走"} else {"XgoDuck 按住行走中"} } else if kind=="walk" { "v6 模型行走中" } else { "v5 模型站立维持中" });
                control::status(shared, v);
                last_status = tick;
            }
            costs.lock().unwrap().cost(3,cycle_start.elapsed().as_secs_f64()*1000.);
            clock.wait();
        }
        let mut v = if cancel.load(Ordering::Acquire) {
            control::unload(bus)?
        } else {
            json!({"state":"holding","message":"模型已停止，保持最后目标；可全部失能卸力"})
        };
        v["action"] = json!("policy");
        let stats = stats(&model, &timing, infer_ms);
        for (k, x) in stats.as_object().unwrap() {
            v[k] = x.clone();
        }
        Ok(v)
    })();
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            if !attempted {
                bail!("{e}；只读预检查未通过，未发运动指令")
            }
            let failed_read = bus.diagnostics.clone();
            let trace = bus.trace.clone();
            let sensors = {
                let s = shared.read().unwrap();
                json!({"imu.orientation":s.latest.get("imu.orientation"),"imu.raw":s.latest.get("imu.raw"),"readerDiagnostics":s.imu_health()})
            };
            let off = control::unload(bus).unwrap_or_else(
                |e| json!({"state":"failed","message":format!("全部失能未确认：{e}")}),
            );
            let evidence = json!({"error":e.to_string(),"stage":stage,"eventTimeMs":epoch_ms(),"calibrationRevision":cal["revision"],"stats":stats(&model,&timing,infer_ms),"lastInferenceObservation":last_obs,"lastInferenceSensors":sensors,"imuCalibration":cal["imu"],"jointCalibration":cal["joints"],"feedback":feedback,"previousFeedback":previous,"goals":goals,"failedRead":failed_read,"serialTrace":trace,"unload":off});
            let saved = (|| -> Result<()> {
                let path = PathBuf::from(
                    std::env::var("MICRODUCK_CALIBRATION_FILE")
                        .unwrap_or("./state/calibration.json".into()),
                );
                let destination = path.parent().unwrap().join("policy-failure-latest.json");
                let tmp = destination.with_extension("tmp");
                std::fs::write(&tmp, serde_json::to_vec_pretty(&evidence)?)?;
                std::fs::rename(tmp, destination)?;
                Ok(())
            })();
            bail!(
                "{e}；stage={stage}；本帧缺失{}，读取{}ms，校验错误{}，收到{}字节；诊断{}；{}",
                failed_read["missingIds"],
                failed_read["elapsedMs"],
                failed_read["checksumErrors"],
                failed_read["rxBytes"],
                if saved.is_ok() {
                    "已保存"
                } else {
                    "保存失败"
                },
                off["message"].as_str().unwrap_or("")
            )
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fall_guard_confirms_tilt_and_stops_severe_falls_immediately() {
        let now=Instant::now();let mut guard=FallGuard::default();
        assert!(!guard.check("xgoduck",Some(65.),now));
        assert!(!guard.check("xgoduck",Some(65.),now+Duration::from_millis(149)));
        assert!(guard.check("xgoduck",Some(65.),now+Duration::from_millis(150)));
        let mut guard=FallGuard::default();
        assert!(guard.check("walk",Some(90.),now));
        assert!(guard.check("xgoduck",Some(180.),now));
    }
    #[test]
    fn fall_guard_resets_after_recovery_missing_imu_and_skill_switches() {
        let now=Instant::now();let mut guard=FallGuard::default();
        for reset in [Some(59.),None,Some(f64::NAN)] {
            assert!(!guard.check("xgoduck",Some(65.),now));
            assert!(!guard.check("xgoduck",reset,now+Duration::from_millis(100)));
            assert!(!guard.check("xgoduck",Some(65.),now+Duration::from_millis(200)));
            guard=FallGuard::default();
        }
        for skill in ["xgoduck_getup","xgoduck_roulade","xgoduck_pick","sitstand_sit","sitstand_stand","stand"] {
            assert!(!guard.check(skill,Some(180.),now));
            assert!(!guard.check(skill,Some(180.),now+Duration::from_secs(1)));
        }
        assert!(!guard.check("xgoduck",Some(65.),now+Duration::from_secs(2)));
    }
    #[test]
    fn fall_angle_uses_shared_reference_and_ignores_horizontal_yaw() {
        let reference=nalgebra::UnitQuaternion::from_euler_angles(0.2,-0.3,0.4);
        let q=|q:nalgebra::UnitQuaternion<f64>|json!([q.i,q.j,q.k,q.w]);
        let cal=json!({"imu":{"initialized":true,"mountingQuaternion":[0,0,0,1],"quaternion":q(reference)}});
        for (rotation,expected) in [(nalgebra::UnitQuaternion::from_euler_angles(0.,0.,2.),0.),
            (nalgebra::UnitQuaternion::from_euler_angles(70f64.to_radians(),0.,0.),70.),
            (nalgebra::UnitQuaternion::from_euler_angles(0.,-95f64.to_radians(),0.),95.)] {
            let sensors=BTreeMap::from([("imu.orientation",json!({"data":{"quaternion":q(reference*rotation)}}))]);
            let gravity=body_gravity(&sensors,&cal).unwrap();
            let angle=(-gravity.z).clamp(-1.,1.).acos().to_degrees();
            assert!((angle-expected).abs()<1e-5,"{angle} vs {expected}");
        }
    }
    #[test]
    fn drive_compensation_preserves_forward_zero_and_proportional_signs() {
        for (input,expected) in [([0.,0.,0.],[0.,0.,0.]),([0.2,0.,0.],[0.2,0.,0.]),
            ([-0.2,0.,0.],[-0.4,0.,0.]),([-0.1,0.1,0.25],[-0.2,0.1,0.5]),
            ([0.,0.,0.5],[0.,0.,1.]),([0.,0.,-0.5],[0.,0.,-1.])] {
            assert_eq!(drive_twist(&json!({"twist":input})).unwrap(),expected);
        }
        for input in [[-0.201,0.,0.],[0.201,0.,0.],[0.,0.101,0.],[0.,0.,0.501],[0.,0.,-0.501]] {
            assert!(drive_twist(&json!({"twist":input})).is_err());
        }
    }
    #[test]
    fn donor_limits_encoder_not_training_joint_angle() {
        let cal=json!({"joints":{"references":{"10":2079},"directions":{}}});
        let (goal,sent,saturated)=donor_target(10,15.297f64.to_radians(),&cal,(0,4095)).unwrap();
        assert_eq!(goal,1905);assert!(sent.to_degrees()>12.6051);assert!(!saturated);
        let (goal,_,saturated)=donor_target(10,400f64.to_radians(),&cal,(0,4095)).unwrap();
        assert_eq!(goal,0);assert!(saturated); // no nearest-turn wrapping
        assert_eq!(donor_target(10,400f64.to_radians(),&cal,(1000,3000)).unwrap().0,1000);
        assert!(donor_target(10,f64::NAN,&cal,(0,4095)).is_err());
        assert!(donor_target(10,0.,&cal,(5000,6000)).is_err());
        assert!(control::angle_request(&json!(10),&json!(15.297)).is_err());
    }
    #[test]
    fn imu_pause_recovers_but_never_accepts_stale_for_inference() {
        let mut pause=ImuPause::default();let start=Instant::now();
        let stale=||Err(StaleImu{topic:"imu.orientation",age_ms:234.}.into());
        assert!(!pause.assess(stale(),start).unwrap());
        assert!(!pause.assess(stale(),start+Duration::from_millis(300)).unwrap());
        assert!(pause.assess(Ok(BTreeMap::new()),start+Duration::from_millis(350)).unwrap());
        assert_eq!(pause.episodes,1);assert_eq!(pause.recoveries,1);
        assert!(pause.assess(Err(StaleImu{topic:"imu.raw",age_ms:1000.}.into()),start).is_err());
        let mut absent=ImuPause::default();
        let missing=||Err(StaleImu{topic:"imu.raw",age_ms:f64::INFINITY}.into());
        assert!(!absent.assess(missing(),start).unwrap());
        assert!(absent.assess(missing(),start+Duration::from_secs(1)).is_err());
        assert!(ImuPause::default().assess(Err(anyhow::anyhow!("IMU 数据无效")),start).is_err());
        assert!(observation_or_hold(Err(StaleImu{topic:"imu.raw",age_ms:234.}.into())).unwrap().is_none());
    }
    #[test]
    fn imu_pause_cancels_old_drive_and_queued_skills() {
        let shared=std::sync::Arc::new(std::sync::RwLock::new(crate::telemetry::Telemetry::new(true)));
        shared.write().unwrap().drive=Some(crate::telemetry::DriveCommand{session:"s".into(),sequence:4,twist:[0.1,0.,0.],updated:Instant::now()});
        shared.write().unwrap().pending_skill=Some("xgoduck_pick".into());
        pause_drive(&shared);
        let state=shared.read().unwrap();assert_eq!(state.drive.as_ref().unwrap().current(),[0.;3]);
        assert_eq!(state.drive.as_ref().unwrap().sequence,4);assert!(state.pending_skill.is_none());
        assert_eq!(state.control["feedbackHolding"],true);
    }

    #[test]
    fn official_posture_flag_is_not_velocity_and_sit_has_no_timeout() {
        assert_eq!(skill_twist("sitstand_sit",0.),[1.,0.,0.]);
        assert_eq!(skill_twist("sitstand_stand",0.),[0.;3]);
        assert!(!skill_complete("sitstand_sit",1000.,1.));
        assert!(!skill_complete("sitstand_stand",0.98,0.));
        assert!(skill_complete("sitstand_stand",1.,0.));
        assert_eq!(skill_kind(&json!("sit")).unwrap(),"sitstand_sit");
        assert_eq!(skill_kind(&json!("standup")).unwrap(),"sitstand_stand");
    }

    #[test]
    fn donor_skills_have_separate_model_paths_and_never_accept_walk_speed() {
        for kind in ["xgoduck_getup","xgoduck_pick","xgoduck_roulade"] {
            assert_eq!(request_profile(&json!({"kind":kind})).unwrap(),(kind,0.));
            assert_eq!(path_for(kind).unwrap().file_name().unwrap().to_str().unwrap(),format!("{kind}.onnx"));
            assert!(request_profile(&json!({"kind":kind,"speed":0.1})).is_err());
        }
        assert!(skill_kind(&json!("../getup")).is_err());
        assert_eq!(skill_kind(&json!("getup")).unwrap(),"xgoduck_getup");
    }
    #[test]
    fn donor_action_phase_completion_and_upright_recovery_are_independent() {
        assert!(!skill_complete("xgoduck_pick",3.98,0.));
        assert!(skill_complete("xgoduck_pick",4.,0.));
        assert!(!skill_complete("xgoduck_roulade",1.88,0.));
        assert!(skill_complete("xgoduck_roulade",1.9,0.));
        assert!(!skill_complete("xgoduck_getup",30.,0.98));
        assert!(skill_complete("xgoduck_getup",2.,1.));
        assert_eq!(skill_twist("xgoduck_getup",0.),[0.;3]);
        let pick=skill_twist("xgoduck_pick",1.);
        assert!(pick[0].abs()<1e-6 && (pick[1]-1.).abs()<1e-6);
        let mut obs=vec![0.;61];obs[5]=-1.;assert!(tilt(&obs)<1e-6);
        obs[5]=1.;assert!((tilt(&obs)-180.).abs()<1e-6);
        assert!((smooth_skill("xgoduck_roulade",&[0.;14],&[1.;14])[0]-0.85).abs()<1e-6);
    }
    #[test]
    fn getup_and_mouth_enable_align_before_enable_and_never_write_eeprom() {
        struct Wire { row:Value, writes:Vec<u8> }
        impl control::Transport for Wire {
            fn calibrate(&mut self,_:u8,_:u16)->Result<()> {bail!("unused")}
            fn feedback(&mut self,ids:&[u8])->Result<Feedback> {Ok(ids.iter().map(|id|(*id,self.row.clone())).collect())}
            fn read(&mut self,_:u8,address:u8,_:u8)->Result<Vec<u8>> {
                assert_eq!(address,0);let mut c=vec![0;40];c[..2].copy_from_slice(&[3,46]);
                c[11..13].copy_from_slice(&4095u16.to_le_bytes());c[16..18].copy_from_slice(&1000u16.to_le_bytes());c[33]=4;Ok(c)
            }
            fn write(&mut self,_:u8,address:u8,_:&[u8])->Result<()> {assert!(address>=40);self.writes.push(address);Ok(())}
            fn sync(&mut self,address:u8,values:&BTreeMap<u8,Vec<u8>>)->Result<()> {
                assert!(address>=40);self.writes.push(address);
                if address==40 {self.row["torque"]=json!(1);}
                if address==50 {let value=values.values().next().unwrap();self.row["kpRaw"]=json!(value[0]);self.row["kdRaw"]=json!(value[1]);}
                Ok(())
            }
        }
        let row=json!({"fault":0,"voltage":7.3,"position":2048,"torque":0,"kpRaw":6,"kdRaw":20,"accelerationRaw":0,"speedLimitRaw":0});
        let mut wire=Wire {row,writes:vec![]};
        let stop=AtomicBool::new(true);let halt=AtomicBool::new(false);
        let feedback=Feedback::from([(34,wire.row.clone())]);
        assert!(arm_current(&mut wire,&[34],&feedback,(32,40),&stop,&halt).is_err());
        assert!(wire.writes.is_empty());stop.store(false,Ordering::Release);
        arm_current(&mut wire,&[34],&feedback,(32,40),&stop,&halt).unwrap();
        assert_eq!(wire.writes,vec![42,50,40,42]);
        wire.writes.clear();let feedback=Feedback::from([(34,wire.row.clone())]);
        arm_current(&mut wire,&[34],&feedback,(32,40),&stop,&halt).unwrap();
        assert_eq!(wire.writes,vec![42,42]);
    }
    #[test]
    fn each_policy_tick_writes_latest_target_before_read_even_without_new_inference() {
        struct Wire { events:Vec<&'static str>, goals:Vec<BTreeMap<u8,Vec<u8>>> }
        impl control::Transport for Wire {
            fn calibrate(&mut self,_:u8,_:u16)->Result<()> { bail!("unused") }
            fn feedback(&mut self,_:&[u8])->Result<Feedback> { bail!("must use policy read") }
            fn policy_tick_feedback(&mut self,ids:&[u8])->Result<Feedback> {
                assert_eq!(ids,&IDS);self.events.push("read");Ok(Feedback::new())
            }
            fn read(&mut self,_:u8,_:u8,_:u8)->Result<Vec<u8>> { bail!("unused") }
            fn write(&mut self,_:u8,_:u8,_:&[u8])->Result<()> { bail!("must use sync") }
            fn sync(&mut self,address:u8,goals:&BTreeMap<u8,Vec<u8>>)->Result<()> {
                assert_eq!(address,42);self.events.push("write");self.goals.push(goals.clone());Ok(())
            }
        }
        let mut wire=Wire { events:vec![],goals:vec![] };
        let mut goals=BTreeMap::from([(12,2000)]);
        policy_bus_cycle(&mut wire,&goals).unwrap();
        policy_bus_cycle(&mut wire,&goals).unwrap();
        goals.insert(12,2002);policy_bus_cycle(&mut wire,&goals).unwrap();
        assert_eq!(wire.events,vec!["write","read","write","read","write","read"]);
        assert_eq!(wire.goals[0],wire.goals[1]);
        assert_ne!(wire.goals[1],wire.goals[2]);
        assert_eq!(wire.goals[2][&12],control::goal_payload(2002).unwrap());
    }
    #[test]
    fn inference_worker_replaces_pending_input_and_preserves_result_identity() {
        let (started_tx,started_rx)=std::sync::mpsc::channel();
        let (resume_tx,resume_rx)=std::sync::mpsc::channel();
        let mut first=true;
        let worker=InferenceWorker::with_infer(move |obs| {
            if first { first=false;started_tx.send(()).unwrap();resume_rx.recv_timeout(Duration::from_secs(2)).unwrap(); }
            Ok(([obs[0];14],0.1))
        });
        worker.submit(1,vec![1.]);
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.submit(2,vec![2.]);worker.submit(3,vec![3.]);
        assert_eq!(worker.input.0.lock().unwrap().as_ref().unwrap().0,3);
        resume_tx.send(()).unwrap();
        let deadline=Instant::now()+Duration::from_secs(2);
        loop {
            if let Some((generation,_,obs,result))=worker.take() {
                if generation==3 { assert_eq!(obs,vec![3.]);assert_eq!(result.unwrap().0,[3.;14]);break; }
                assert_eq!(generation,1);
            }
            assert!(Instant::now()<deadline);std::thread::yield_now();
        }
        drop(worker);
    }
    #[test]
    fn inference_error_is_delivered_and_idle_worker_shuts_down() {
        let worker=InferenceWorker::with_infer(|_|bail!("inference failed"));
        worker.submit(7,vec![0.]);
        let deadline=Instant::now()+Duration::from_secs(2);
        loop {
            if let Some((generation,_,_,result))=worker.take() {
                assert_eq!(generation,7);assert_eq!(result.unwrap_err().to_string(),"inference failed");break;
            }
            assert!(Instant::now()<deadline);std::thread::yield_now();
        }
        drop(worker);
    }
    #[test]
    fn model_selection_and_speed_are_explicit_and_bounded() {
        assert_eq!(request_profile(&json!({})).unwrap(), ("stand", 0.0));
        assert_eq!(request_profile(&json!({"kind":"walk"})).unwrap(), ("walk", 0.2));
        assert_eq!(request_profile(&json!({"kind":"walk","speed":0.1})).unwrap(), ("walk", 0.1));
        for body in [json!({"kind":"../walk"}),json!({"kind":null}),json!({"kind":"walk","speed":-0.1}),json!({"kind":"walk","speed":0.21}),json!({"kind":"walk","speed":"0.1"}),json!({"speed":0.1})] {
            assert!(request_profile(&body).is_err());
        }
        assert_eq!(path_for("walk").unwrap().file_name().unwrap(), "hd1910-walk-v6-symmetry500.onnx");
        assert_ne!(path_for("walk").unwrap(), path_for("stand").unwrap());
    }
    #[test]
    fn xgoduck_action_filter_keeps_raw_history_separate() {
        let raw=[1.;14];
        let first=smooth_action(&[0.;14],&raw);
        assert!((first[0]-0.55).abs()<1e-6);
        let second=smooth_action(&first,&raw);
        assert!((second[13]-0.7975).abs()<1e-6);
        assert_eq!(raw,[1.;14]);
        assert_eq!(smooth_action(&[0.;14],&[0.;14]),[0.;14]);
    }
    #[test]
    fn restored_fresh_feedback_resets_history_based_on_sample_gap() {
        let previous = [99.338777883; 14];
        // Recovered read at 99.604s: sample gap >250ms but pause <200ms.
        assert!(history_needs_reset(&[99.604;14], &previous).unwrap());
        assert!(!history_needs_reset(&previous, &previous).unwrap());
        assert!(!history_needs_reset(&[99.36;14], &previous).unwrap());
        let mut one_long = [99.36;14]; one_long[13] = 99.604;
        assert!(history_needs_reset(&one_long, &previous).unwrap());
        assert!(history_needs_reset(&[99.3;14], &previous).is_err());
        assert!(history_needs_reset(&[f64::NAN;14], &previous).is_err());
    }
    #[test]
    fn third_coasted_frame_expiring_during_observation_enters_hold() {
        let start = Instant::now();
        let rows = Feedback::from([(20, json!({"position":100,"_receivedMono":582.656195015}))]);
        let mut coast = control::FeedbackCoast::default();
        coast.sample(&rows, &[20], start);
        for ms in [55, 110, 148] {
            assert!(coast.sample(&Feedback::new(), &[20], start + Duration::from_millis(ms)).is_some());
        }
        assert_eq!(coast.misses, 3);
        assert!(check_feedback_age(20, 582.656195015, 582.804731994).is_ok());
        let expired = check_feedback_age(20, 582.656195015, 582.807).map(|_| vec![]);
        assert_eq!(observation_or_hold(expired).unwrap(), None);
        assert_eq!(coast.diagnostics(json!({}), true)["feedbackHolding"], true);
        assert_eq!(observation_or_hold(Ok(vec![1.0])).unwrap(), Some(vec![1.0]));
        assert!(observation_or_hold(Err(anyhow::anyhow!("电压异常"))).is_err());
    }
    #[test]
    #[ignore = "requires Python generated fixture and native ONNX Runtime"]
    fn python_parity() {
        let fixture: Value = serde_json::from_slice(
            &std::fs::read(std::env::var("MICRODUCK_POLICY_PARITY_FIXTURE").unwrap()).unwrap(),
        )
        .unwrap();
        let mut model = Model::open().unwrap();
        let shared = std::sync::Arc::new(std::sync::RwLock::new(crate::telemetry::Telemetry::new(
            true,
        )));
        for key in ["imu.orientation", "imu.raw"] {
            shared.write().unwrap().sample(
                key,
                fixture["sensors"][key]["data"].clone(),
                true,
                Instant::now(),
            );
        }
        let received = crate::telemetry::monotonic();
        let feedback = fixture["feedback"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, row)| {
                let mut row = row.clone();
                row["_receivedMono"] = json!(received);
                (key.parse::<u8>().unwrap(), row)
            })
            .collect();
        let obs = model
            .observe(&shared, &feedback, &fixture["calibration"])
            .unwrap();
        let expected = fixture["observation"].as_array().unwrap();
        let err = obs
            .iter()
            .zip(expected)
            .map(|(a, b)| (*a as f64 - b.as_f64().unwrap()).abs())
            .reduce(f64::max)
            .unwrap();
        assert!(err < 1e-6, "Observation max error {err}");
        let (action, _) = model.infer(obs).unwrap();
        let err = action
            .iter()
            .zip(fixture["action"].as_array().unwrap())
            .map(|(a, b)| (*a as f64 - b.as_f64().unwrap()).abs())
            .reduce(f64::max)
            .unwrap();
        assert!(err < 1e-5, "ONNX max error {err}");
    }
}
