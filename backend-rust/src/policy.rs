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
fn check_feedback_age(id: u8, received: f64, now: f64) -> Result<()> {
    if !(0.0..0.15).contains(&(now - received)) {
        return Err(StaleFeedback(id).into());
    }
    Ok(())
}
// Only servo age expiration enters the hold path; other sensor/fault errors propagate.
fn observation_or_hold(result: Result<Vec<f32>>) -> Result<Option<Vec<f32>>> {
    match result {
        Ok(obs) => Ok(Some(obs)),
        Err(e) if e.is::<StaleFeedback>() => Ok(None),
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
        .unwrap_or_else(|_| PathBuf::from("../debug-server/models/hd1910-head-v5.onnx"))
}
pub fn path_for(kind: &str) -> Result<PathBuf> {
    match kind {
        "stand" => Ok(path()),
        "walk" => Ok(path().with_file_name("hd1910-walk-v6-symmetry500.onnx")),
        "xgoduck" => Ok(path().with_file_name("xgoduck_walk.onnx")),
        _ => bail!("未知模型类型"),
    }
}
pub fn model_id(kind: &str) -> &'static str {
    match kind { "walk" => "hd1910-walk-v6-symmetry500", "xgoduck" => "xgoduck_walk", _ => "hd1910-head-v5" }
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
    Ok(twist.map(|v|v as f32))
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
    if kind == "xgoduck" && meta["task"] != "Mjlab-Velocity-Flat-XgoDuck" { bail!("XgoDuck模型任务不匹配") }
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
fn smooth_action(previous: &[f32; 14], raw: &[f32; 14]) -> [f32; 14] {
    std::array::from_fn(|n| previous[n] * 0.45 + raw[n] * 0.55)
}
struct Model {
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
            session: std::sync::Arc::new(std::sync::Mutex::new(session)),
            command_twist: [speed,0.,0.],
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
        let s = shared.read().unwrap();
        let now = crate::telemetry::monotonic();
        let mut sensors = BTreeMap::new();
        for key in ["imu.orientation", "imu.raw"] {
            let sample = s
                .latest
                .get(key)
                .ok_or_else(|| anyhow::anyhow!("IMU 数据缺失或过期（{key}）"))?;
            let age = s.stamp(sample)["ageMs"].as_f64().unwrap_or(f64::INFINITY);
            if sample["valid"] != true
                || sample["source"] != "hardware"
                || !(0.0..150.).contains(&age)
            {
                bail!("IMU 数据缺失或过期（{key}，age={age}ms）")
            };
            sensors.insert(key, sample.clone());
        }
        if sensors["imu.orientation"]["bootId"] != sensors["imu.raw"]["bootId"] {
            bail!("IMU 会话不一致")
        }
        drop(s);
        let imu = &cal["imu"];
        if imu["initialized"] != true || imu["mountingQuaternion"].is_null() {
            bail!("请先完成 IMU 位置和安装方向标定")
        }
        let mounting = orientation::rotation(&imu["mountingQuaternion"])?;
        let identity = json!([0, 0, 0, 1]);
        let target = orientation::rotation(imu.get("targetQuaternion").unwrap_or(&identity))?;
        let relative = orientation::rotation(&imu["quaternion"])?.transpose()
            * orientation::rotation(&sensors["imu.orientation"]["data"]["quaternion"])?;
        let body = target * mounting.transpose() * relative * mounting;
        let gravity = body.transpose() * Vector3::new(0., 0., -1.);
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
pub fn execute(
    bus: &mut Bus,
    command: &Value,
    cancel: &AtomicBool,
    halt: &AtomicBool,
    shared: &Shared,
) -> Result<Value> {
    let (kind, speed) = request_profile(command)?;
    let mut model = Model::open_for(kind, speed)?;
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
        v["commandTargetHz"]=json!(100);
        for (key,value) in json!({"policy":model_id(kind),"kind":kind,"walkSpeedMps":speed,"policySha256":model.sha,"inferenceMs":control::rounded(infer_ms,3),"runtimeProfile":"xgoduck-schedule100-v1","homeCommandTargetHz":50,"feedbackReplyBudgetMs":6,"feedbackTargetHz":100,"inferenceTargetHz":50,"actionAlpha":0.45,"velocitySource":"servo-register","velocityAlphaAt100Hz":0.4,"kpRun":control::POLICY_GAINS.0,"kdRun":control::POLICY_GAINS.1,"kpHome":control::MANUAL_GAINS.0,"kdHome":control::MANUAL_GAINS.1,"saturatedIds":model.saturated,"rawTargetDegrees":model.raw,"sentTargetDegrees":model.sent}).as_object().unwrap(){v[key]=value.clone();}
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
        v["walkSpeedMps"] = json!(model.command_twist[0]);
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
        model.observe(shared, &feedback, cal)?;
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
            let (compatible, reason) = match model.targets(smooth_action(&model.filtered_action, &action), &feedback, cal, &limits) {
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
                .map(|n| ((model.home[n] + smooth_action(&model.filtered_action, &action)[n]) as f64).to_degrees())
                .collect::<Vec<_>>());
            v["currentPoseTargetsCompatible"] = json!(compatible);
            v["reason"] = json!(reason);
            return Ok(v);
        }
        let home: BTreeMap<_, _> = ORDER
            .into_iter()
            .zip(model.home.map(|v| v as f64))
            .collect();
        stage = "home-transition";
        attempted = true;
        control::execute(
            bus,
            &json!({"action":"stand","calibration":cal,"standTargets":home}),
            cancel,
            shared,
        )?;
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
        goals=model.targets([0.;14],&feedback,cal,&limits)?;
        let worker=InferenceWorker::new(model.session.clone());
        let mut generation=0u64;
        stage = "policy-loop";
        let mut clock = control::FrameClock::with_period(Instant::now(), Duration::from_millis(10));
        let mut next_inference = Instant::now();
        let mut coast = control::FeedbackCoast::default();
        let mut paused: Option<Instant> = None;
        let mut last_status = Instant::now() - Duration::from_secs(1);
        // Bounded timestamp ring keeps long running policies at constant memory.
        while !cancel.load(Ordering::Acquire) && !halt.load(Ordering::Acquire) {
            let cycle_start=Instant::now();
            if kind=="xgoduck" {
                let twist=shared.read().unwrap().drive.as_ref().map(|d|d.current()).unwrap_or([0.;3]);
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
                        let filtered=smooth_action(&model.filtered_action,&action);
                        goals=model.targets(filtered,&feedback,cal,&limits)?;
                        accepted_action=Some((action,filtered));
                    }
                }
            }
            if worker.thread.as_ref().is_some_and(|t|t.is_finished()) { bail!("模型推理线程意外退出"); }
            if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) { break; }
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
            for (id, row) in &fresh { control::validate(*id, Some(row), ORDER.contains(id))?; }
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
                Some(rows) => observation_or_hold(model.observe(shared, rows, cal))?,
                None => None,
            };
            costs.lock().unwrap().cost(1,observe_start.elapsed().as_secs_f64()*1000.);
            bus.diagnostics = coast.diagnostics(diagnostics, obs.is_none());
            if sampled.is_some() && obs.is_none() {
                bus.diagnostics["feedbackHoldReason"] = json!("feedback-expired");
            }
            control::publish(shared, &fresh, &IDS, bus.diagnostics.clone());
            if obs.is_none() {
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
                v["policyPhase"] = json!("feedback-hold");
                v["feedbackHolding"] = json!(true);
                v["consecutiveFeedbackFailures"] = json!(coast.misses);
                v["skippedControlTicks"] = json!(clock.skipped);
                v["message"] = json!("反馈持续缺失，策略暂停，保持最后测得姿态；等待完整反馈恢复");
                control::status(shared, v);
                costs.lock().unwrap().cost(3,cycle_start.elapsed().as_secs_f64()*1000.);
                clock.wait();
                continue;
            }
            feedback = sampled.unwrap();
            paused.take();
            let obs = obs.unwrap();
            let now = Instant::now();
            if now >= next_inference {
                while next_inference <= now { next_inference += Duration::from_millis(20); }
                worker.submit(generation,obs);
            }
            let tick=Instant::now();
            if last_status.elapsed() >= Duration::from_millis(200) {
                let mut v = stats(&model, &timing, infer_ms);
                v["state"] = json!("policy");
                v["action"] = json!("policy");
                v["policyPhase"] = json!(if coast.misses > 0 { "coasting" } else { "running" });
                v["feedbackHolding"] = json!(false);
                v["consecutiveFeedbackFailures"] = json!(coast.misses);
                v["skippedControlTicks"] = json!(clock.skipped);
                v["message"] = json!(if kind=="xgoduck" { if model.command_twist==[0.;3] {"XgoDuck 原地平衡，按住方向键行走"} else {"XgoDuck 按住行走中"} } else if kind=="walk" { "v6 模型行走中" } else { "v5 模型站立维持中" });
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
                json!({"imu.orientation":s.latest.get("imu.orientation"),"imu.raw":s.latest.get("imu.raw")})
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
