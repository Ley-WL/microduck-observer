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
pub fn metadata() -> Option<Value> {
    let p = path();
    if !p.is_file() {
        return None;
    }
    serde_json::from_slice(&std::fs::read(p.with_extension("metadata.json")).ok()?).ok()
}
pub fn verify_fixture(path: &std::path::Path) -> Result<Value> {
    let fixture: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut model = Model::open()?;
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
struct Model {
    session: Session,
    home: [f32; 14],
    sha: String,
    previous: Option<([f64; 14], [f64; 14])>,
    velocity: [f64; 14],
    last_action: [f32; 14],
    raw: Vec<f64>,
    sent: Vec<f64>,
    saturated: Vec<u8>,
}
impl Model {
    fn open() -> Result<Self> {
        let p = path();
        let meta = metadata().ok_or_else(|| anyhow::anyhow!("模型文件或元数据缺失"))?;
        if meta["jointOrder"] != json!(NAMES) || meta["controlHz"] != 50 {
            bail!("模型关节顺序或频率不匹配")
        }
        let sha = format!("{:x}", Sha256::digest(std::fs::read(&p)?));
        if meta["policySha256"] != sha {
            bail!("模型 SHA256 不匹配")
        }
        let home = orientation::vector::<14>(&meta["homeRadians"])?.map(|v| v as f32);
        // Explicit path: library absence is an API error, never a loader panic.
        let runtime = std::env::var("ORT_DYLIB_PATH")
            .map_err(|_| anyhow::anyhow!("未配置 ORT_DYLIB_PATH 原生推理库"))?;
        if !std::path::Path::new(&runtime).is_file() {
            bail!("ONNX Runtime 原生库不存在")
        }
        ort::init_from(&runtime)?.commit();
        let session = Session::builder()?
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
        Ok(Self {
            session,
            home,
            sha,
            previous: None,
            velocity: [0.; 14],
            last_action: [0.; 14],
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
        let mut velocity = [0.; 14];
        // A complete fresh sample after a long feedback gap starts a new
        // observation history, even if feedback-hold began less than 200ms ago.
        if self.previous.as_ref().map(|(_, times)| history_needs_reset(&stamps, times)).transpose()?.unwrap_or(false) {
            self.previous = None;
            self.velocity = [0.; 14];
            self.last_action = [0.; 14];
        }
        if let Some((old, times)) = self.previous {
            for n in 0..14 {
                let dt = stamps[n] - times[n];
                velocity[n] = if dt > 1e-6 {
                    (angles[n] - old[n]) / dt
                } else {
                    self.velocity[n]
                };
            }
        }
        let mut obs = Vec::with_capacity(61);
        obs.extend(gyro.iter().map(|v| *v as f32));
        obs.extend(gravity.iter().map(|v| *v as f32));
        obs.extend(
            angles
                .iter()
                .zip(self.home)
                .map(|(a, h)| (a - h as f64) as f32),
        );
        obs.extend(self.velocity.map(|v| v as f32));
        obs.extend(self.last_action);
        obs.extend([0f32; 13]);
        self.previous = Some((angles, stamps));
        self.velocity = velocity;
        if obs.len() != 61 || obs.iter().any(|v| !v.is_finite()) {
            bail!("模型观测无效")
        };
        Ok(obs)
    }
    fn infer(&mut self, obs: Vec<f32>) -> Result<([f32; 14], f64)> {
        let start = Instant::now();
        let tensor = Tensor::from_array(([1usize, 61], obs.into_boxed_slice()))?;
        let outputs = self.session.run(ort::inputs![tensor])?;
        let (_, data) = outputs[0].try_extract_tensor::<f32>()?;
        let action: [f32; 14] = data
            .try_into()
            .map_err(|_| anyhow::anyhow!("模型输出无效"))?;
        if action.iter().any(|v| !v.is_finite()) {
            bail!("模型输出无效")
        };
        Ok((action, start.elapsed().as_secs_f64() * 1000.))
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
    let mut model = Model::open()?;
    let cal = &command["calibration"];
    let mut attempted = false;
    let mut stage = "preflight";
    let mut timing = control::Timing::default();
    let mut infer_ms = 0.;
    let mut last_obs = None;
    let mut feedback = Feedback::new();
    let mut previous = Feedback::new();
    let mut goals = BTreeMap::new();
    let stats = |model: &Model, timing: &control::Timing, infer_ms: f64| -> Value {
        let mut v = timing.stats();
        for (key,value) in json!({"policy":"hd1910-head-v5","policySha256":model.sha,"inferenceMs":control::rounded(infer_ms,3),"saturatedIds":model.saturated,"rawTargetDegrees":model.raw,"sentTargetDegrees":model.sent}).as_object().unwrap(){v[key]=value.clone();}
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
        v
    };
    let result = (|| -> Result<Value> {
        if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) {
            return Ok(json!({"state":"holding","action":"policy","message":"模型启动已取消"}));
        }
        control::status(
            shared,
            json!({"state":"preflight","action":"policy","message":"v5 模型与实时传感器预检查"}),
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
            let (compatible, reason) = match model.targets(action, &feedback, cal, &limits) {
                Ok(g) => {
                    goals = g;
                    (true, String::new())
                }
                Err(e) => (false, e.to_string()),
            };
            let mut v = stats(&model, &timing, infer_ms);
            v["state"] = json!("shadow");
            v["action"] = json!("policy");
            v["message"] = json!("v5 只读推理完成，未发运动指令");
            v["targets"] = json!(goals);
            v["targetDegrees"] = json!((0..14)
                .map(|n| ((model.home[n] + action[n]) as f64).to_degrees())
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
        model.previous = None;
        model.velocity = [0.; 14];
        model.last_action = [0.; 14];
        stage = "policy-loop";
        let mut clock = control::FrameClock::new(Instant::now());
        let mut coast = control::FeedbackCoast::default();
        let mut paused: Option<Instant> = None;
        let mut last_status = Instant::now() - Duration::from_secs(1);
        // Bounded timestamp ring keeps long running policies at constant memory.
        while !cancel.load(Ordering::Acquire) && !halt.load(Ordering::Acquire) {
            let (fresh, diagnostics) = control::poll_tick(bus, &IDS);
            // Real faults and bad supply are never hidden by the dropped-read grace.
            for (id, row) in &fresh { control::validate(*id, Some(row), ORDER.contains(id))?; }
            let sampled = coast.sample(&fresh, &IDS, Instant::now());
            bus.diagnostics = coast.diagnostics(diagnostics.clone(), sampled.is_none());
            if sampled.is_some() && paused.is_some_and(|since| since.elapsed() >= Duration::from_millis(200)) {
                model.previous = None;
                model.velocity = [0.; 14];
                model.last_action = [0.; 14];
            }
            // Check actual per-servo timestamps at observation time, including the
            // few milliseconds between the coast decision and observation.
            let obs = match &sampled {
                Some(rows) => observation_or_hold(model.observe(shared, rows, cal))?,
                None => None,
            };
            bus.diagnostics = coast.diagnostics(diagnostics, obs.is_none());
            if sampled.is_some() && obs.is_none() {
                bus.diagnostics["feedbackHoldReason"] = json!("feedback-expired");
            }
            control::publish(shared, &fresh, &IDS, bus.diagnostics.clone());
            if obs.is_none() {
                if paused.is_none() {
                    if let Some(last) = &coast.last {
                        goals = ORDER.iter().map(|id| (*id,last[id]["position"].as_i64().unwrap() as i32)).collect();
                    }
                    if !cancel.load(Ordering::Acquire) && !halt.load(Ordering::Acquire) {
                        control::goals(bus, &goals)?;
                        timing.record(Instant::now());
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
                clock.wait();
                continue;
            }
            feedback = sampled.unwrap();
            paused.take();
            let obs = obs.unwrap();
            for id in ORDER {
                control::validate(id, feedback.get(&id), true)?;
            }
            last_obs = Some(obs.clone());
            let (action, ms) = model.infer(obs)?;
            infer_ms = ms;
            goals = model.targets(action, &feedback, cal, &limits)?;
            if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) {
                break;
            }
            control::goals(bus, &goals)?;
            model.last_action = action;
            previous = feedback.clone();
            let tick = Instant::now();
            timing.record(tick);
            if last_status.elapsed() >= Duration::from_millis(200) {
                let mut v = stats(&model, &timing, infer_ms);
                v["state"] = json!("policy");
                v["action"] = json!("policy");
                v["policyPhase"] = json!(if coast.misses > 0 { "coasting" } else { "running" });
                v["feedbackHolding"] = json!(false);
                v["consecutiveFeedbackFailures"] = json!(coast.misses);
                v["skippedControlTicks"] = json!(clock.skipped);
                v["message"] = json!("v5 模型站立维持中");
                control::status(shared, v);
                last_status = tick;
            }
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
