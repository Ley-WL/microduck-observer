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
    state.pending_skill=None;state.pending_mouth=None;state.pending_quack=false;state.quack_running=false;
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
// kissqy robotd's LB edge: HOME-origin or confirmed-fallen recovery resets
// the controller; upright policy-origin recovery preserves its seated state.
fn native_recovery_transition(active: &str, return_home: bool, fallen: bool) -> Result<(bool, &'static str)> {
    if !return_home && !fallen && matches!(active,"xgoduck_pick"|"xgoduck_roulade") {
        bail!("当前动作执行中，直立状态不能切入倒地起身");
    }
    let reset=return_home || fallen;
    Ok((reset,if !reset && active=="sitstand_sit" {"sitstand_sit"} else {"xgoduck"}))
}
#[derive(Debug,PartialEq)]
enum NativeHandoff { Home, Skill(String), Continue }
fn native_handoff(active: &str, finished: bool, donor_getup: bool, return_home: bool,
    recovery_resume: &str, requested: Option<String>) -> NativeHandoff {
    // HOME-origin recovery expires before new one-shot requests in robotd.
    // Repeated LB presses acknowledge the active recovery without restarting it.
    if finished && donor_getup && return_home {return NativeHandoff::Home;}
    let next=requested.filter(|next|next!=active).or_else(||finished.then(||
        if donor_getup {recovery_resume.to_owned()} else {"xgoduck".to_owned()}));
    match next {Some(next) if next!=active=>NativeHandoff::Skill(next),_=>NativeHandoff::Continue}
}
#[derive(Default)]
struct NativeRecoveryPosture { since: Option<Instant>, fallen: bool }
impl NativeRecoveryPosture {
    fn observe(&mut self, angle: f64, now: Instant) -> bool {
        // Donor Safety defaults: gravity z > -0.5 for 200ms. This is
        // request arbitration only; it does not change the fall-stop guard.
        if angle>60. {
            let since=*self.since.get_or_insert(now);
            self.fallen=now.duration_since(since)>=Duration::from_millis(200);
        } else {self.since=None;self.fallen=false;}
        self.fallen
    }
}
#[derive(Clone, Copy, Default)]
struct SkillProfile { retrained: bool, bounded_targets: bool, kissqy: bool, donor_pick: bool, donor_getup: bool, donor_roll:bool }
impl SkillProfile {
    fn from_metadata(meta: &Value) -> Self {
        Self { donor_roll:meta["runtimeKind"]=="kissqy_roulade", donor_pick:meta["runtimeKind"]=="kissqy_ground_pick", donor_getup:meta["runtimeKind"]=="kissqy_stand", retrained: matches!(meta["runtimeKind"].as_str(), Some("hd1910_getup"|"hd1910_sitstand")),
            kissqy: meta["runtimeKind"]=="kissqy_sitstand", bounded_targets: meta["skillPhysicsRevision"]=="sit-self-contact-bounded-target-v3" }
    }
    fn donor(self)->bool {self.kissqy || self.donor_pick || self.donor_getup || self.donor_roll}
    fn sit_settle(self) -> f64 { if self.retrained || self.kissqy {4.} else {2.} }
    fn complete(self, kind: &str, progress: f64, upright: f64) -> bool {
        if self.donor_pick {return kind=="xgoduck_pick" && progress>=2.8;}
        if self.donor_roll {return kind=="xgoduck_roulade" && progress>=1.-1e-9;}
        if self.donor_getup {return kind=="xgoduck_getup" && progress>=6.-1e-9;}
        if !self.retrained { return skill_complete(kind,progress,upright); }
        match kind {
            "sitstand_stand" => progress>=4. && upright>=1.,
            "xgoduck_getup" => progress>=3. && upright>=1.,
            _ => skill_complete(kind,progress,upright),
        }
    }
    fn upright(self, obs: &[f32]) -> bool {
        if tilt(obs)>=15. { return false; }
        if !self.retrained { return true; }
        let gyro=obs[..3].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
        let legs=(0..5).chain(9..14).map(|n|obs[6+n].abs() as f64).sum::<f64>()/10.;
        gyro<2. && legs<0.25
    }
}
fn validate_skill_metadata(kind: &str, meta: &Value) -> Result<()> {
    let profile=SkillProfile::from_metadata(meta);
    if profile.donor_pick || profile.donor_getup || profile.donor_roll {
        let e=&meta["skillExecution"];
        let (expected,task,seconds)=if profile.donor_pick {("xgoduck_pick","Mjlab-GroundPick-Flat-MicroDuck",2.8)} else if profile.donor_roll {("xgoduck_roulade","Mjlab-Roulade-Flat-MicroDuck",1.)} else {("xgoduck_getup","Mjlab-StandUp-Rough-Backlash-MicroDuck",6.)};
        if kind!=expected || meta["task"]!=task || e["schema"]!="kissqy-skills-v1"
            || e["policySha256"]!=meta["policySha256"] || e["gains"]!=json!({"p":5,"d":20})
            || e["durationSeconds"].as_f64()!=Some(seconds) || e["actionScale"]!=1
            || meta["actionEmaOldWeight"]!=0 || meta["actionEmaNewWeight"]!=1 {bail!("kissqy技能执行合同不匹配");}
    } else if profile.kissqy {
        let e=&meta["sitExecution"];
        if !kind.starts_with("sitstand_") || meta["task"]!="Mjlab-SitStand-Flat-MicroDuck"
            || e["schema"]!="kissqy-sitstand-v1" || e["policySha256"]!=meta["policySha256"]
            || e["gains"]!=json!({"p":5,"d":20}) || meta["actionEmaOldWeight"]!=0 || meta["actionEmaNewWeight"]!=1
            || meta["standSeconds"]!=1 || meta["sitSettleSeconds"]!=4
            || meta["commandEncoding"]!=json!({"slot":"twist.vx","sit":1,"stand":0}) {bail!("kissqy坐站合同不匹配");}
    } else if profile.retrained {
        if profile.bounded_targets && !kind.starts_with("sitstand_") {bail!("坐站碰撞合同不能用于其他动作");}
        let (runtime,task)=if kind.starts_with("sitstand_") {
            ("hd1910_sitstand", if profile.bounded_targets {"Mjlab-SitStandContact-Flat-MicroDuck-HD1910"} else {"Mjlab-SitStand-Flat-MicroDuck-HD1910"})
        } else if kind=="xgoduck_getup" {
            ("hd1910_getup","Mjlab-Getup-Flat-MicroDuck-HD1910")
        } else { bail!("HD1910技能模型与动作不匹配") };
        if meta["runtimeKind"]!=runtime || meta["task"]!=task
            || meta["actionEmaOldWeight"]!=0.45 || meta["actionEmaNewWeight"]!=0.55
            || if profile.bounded_targets { !valid_sit_target_contract(meta) } else { !meta["trainingTargetClipRadians"].is_null() }
            || meta["assembledBodyGroundContactMeshes"]!=68
            || meta["uprightHoldSeconds"]!=1
            || (kind.starts_with("sitstand_") && (meta["sitSettleSeconds"]!=4 || meta["standSeconds"]!=4
                || meta["commandEncoding"]!=json!({"slot":"twist.vx","sit":1,"stand":0})))
            || (kind=="xgoduck_getup" && meta["getupMinSeconds"]!=3) {
            bail!("HD1910技能模型运行合同不匹配")
        }
    } else {
        if kind.starts_with("sitstand_") && (meta["runtimeKind"]!="official_sitstand" || meta["task"]!="Mjlab-SitStand-Flat-MicroDuck") { bail!("坐站模型任务不匹配") }
        if is_xgoduck(kind) && !kind.starts_with("sitstand_") && meta["runtimeKind"]!=kind && !(kind=="xgoduck" && meta["task"]=="Mjlab-Velocity-Flat-XgoDuck") { bail!("XgoDuck模型任务不匹配") }
    }
    Ok(())
}
fn valid_sit_target_contract(meta: &Value) -> bool {
    // Historical v3 training bounds describe its training physics, not today's
    // mechanical output guard. Never rewrite a model's training contract.
    let archived:Value=serde_json::from_str(include_str!("../tests/fixtures/xgoduck-joint-limits.json")).unwrap();
    let Some(bounds)=meta["trainingTargetClipRadians"].as_object() else {return false};
    let Some(names)=meta["jointOrder"].as_array() else {return false};
    meta["jointOrder"]==json!(NAMES) && bounds.len()==14 && ORDER.iter().enumerate().all(|(n,id)| {
        let Some(name)=names[n].as_str() else {return false};
        let Some(range)=bounds.get(name).and_then(Value::as_array) else {return false};
        let _=id;
        let expected=&archived["joints"][name]["range"];
        range.len()==2 && range[0].as_f64().is_some_and(|v|(v-expected[0].as_f64().unwrap()).abs()<1e-8)
            && range[1].as_f64().is_some_and(|v|(v-expected[1].as_f64().unwrap()).abs()<1e-8)
    })
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
fn home_posture_allowed(angle:f64) -> Result<()> {
    if !angle.is_finite() || angle>=FALL_TILT_DEG {
        bail!("机身倾角{angle:.1}°，仍处于倒地姿态，拒绝预备/HOME目标；请使用倒地起身");
    }
    Ok(())
}
pub fn check_home_posture(shared:&Shared,cal:&Value) -> Result<f64> {
    let sensors=imu_samples(shared)?;
    let gravity=body_gravity(&sensors,cal)?;
    let angle=(-gravity.z/gravity.norm()).clamp(-1.,1.).acos().to_degrees();
    home_posture_allowed(angle)?;Ok(angle)
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
    if !speed.is_finite() || !(0.0..=0.4).contains(&speed) || (kind != "walk" && speed != 0.0) {
        bail!("前进速度应在0–0.4m/s内，站立模型速度必须为0")
    }
    Ok((kind, speed as f32))
}
pub fn drive_twist(body: &Value) -> Result<[f32; 3]> {
    let twist=orientation::vector::<3>(&body["twist"])?;
    for (v,max) in twist.iter().zip([0.4,0.1,0.5]) {
        if !v.is_finite() || v.abs()>max { bail!("行走指令超出范围"); }
    }
    // Physical commands go to the donor observation unchanged. No local
    // reverse/yaw compensation hidden between the UI and the policy.
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
    if model.skill_profile.donor() {
        let filtered=model.filter_action(&action);
        if filtered!=action {bail!("kissqy坐站目标不应平滑");}
    }
    if let Some(profile)=model.walk_profile {
        let first=profile.filter(&[0.;14],&action,false);
        let second=profile.filter(&first,&[0.;14],true);
        for (name,actual) in [("filteredActionFirst",first),("filteredActionSecond",second)] {
            let expected=orientation::vector::<14>(&fixture[name])?;
            if actual.iter().zip(expected).any(|(a,b)|(*a as f64-b).abs()>1e-5) {bail!("行走执行目标数值不一致：{name}");}
        }
    }
    let filtered=model.filter_action(&action);
    let encoder_bounds=ORDER.into_iter().map(|id|(id,(0,4095))).collect();
    let goals=model.targets(filtered,&feedback,&fixture["calibration"],&encoder_bounds)?;
    for (n,id) in ORDER.iter().enumerate() {
        let (lo,hi)=limit(*id).unwrap();
        let sent=model.sent[n];
        if sent<lo-0.05 || sent>hi+0.05 {bail!("#{id} 纯推理fixture目标超机械范围");}
    }
    let mut target_cases=0;
    if let Some(cases)=fixture["targetCases"].as_array() {
        for case in cases {
            let angles=orientation::vector::<14>(&case["targetsRadians"])?;
            for (n,id) in ORDER.iter().enumerate() {
                let (goal,_,_)=donor_target(*id,angles[n],&fixture["targetCaseCalibration"],(0,4095))?;
                if case["expectedGoals"][id.to_string()].as_i64()!=Some(goal as i64) {bail!("#{id} 保存实机目标裁剪对照失败");}
            }
            target_cases+=1;
        }
    }
    Ok(
        json!({"ok":true,"observationMaxAbsError":observation_error,"actionMaxAbsError":action_error,"inferenceMs":inference_ms,"policySha256":model.sha,"kpRun":model.gains.p,"kdRun":model.gains.d,"walkExecutionVerified":model.walk_profile.is_some(),"mechanicalTargetsVerified":true,"savedTargetCasesVerified":target_cases,"goals":goals,"sentTargetDegrees":model.sent,"saturatedIds":model.saturated,"hardwareAccess":false}),
    )
}
fn verified_metadata_for(kind: &str) -> Result<Value> {
    let p = path_for(kind)?;
    let meta: Value = serde_json::from_slice(&std::fs::read(p.with_extension("metadata.json"))?)?;
    if meta["jointOrder"] != json!(NAMES) || meta["controlHz"] != 50 || meta["actorObservationDim"] != 61 || meta["actionDim"] != 14 {
        bail!("模型关节顺序或频率不匹配")
    }
    if kind == "walk" && meta["task"] != "Mjlab-Walk-Flat-MicroDuck-HD1910" { bail!("行走模型任务不匹配") }
    validate_skill_metadata(kind,&meta)?;
    crate::walk_profile::WalkProfile::parse(kind,&meta)?;
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
    arm_current_profile(bus,ids,feedback,gains,cancel,halt,false)
}
fn arm_current_profile<T: control::Transport>(bus: &mut T, ids: &[u8], feedback: &Feedback, gains:(u8,u8), cancel:&AtomicBool, halt:&AtomicBool, native:bool) -> Result<()> {
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
    if native {control::native_profiles(bus,ids,feedback,gains)?;} else {control::profiles_with_gains(bus,ids,feedback,gains)?;}
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
/// Same battery law as robotd: mean servo supply, sampled once per second,
/// EMA new weight .1, then nominal 7.4 / clamp(V,6,9.5). It scales actions,
/// never raw previous-action observations or the calibrated HOME.
#[derive(Default)]
pub(crate) struct NativeVoltage { pub(crate) volts:Option<f64>, sampled:Option<Instant> }
impl NativeVoltage {
    pub(crate) fn scale(&mut self, feedback:&Feedback, now:Instant)->f32 {
        if self.sampled.is_none_or(|at| now.saturating_duration_since(at)>=Duration::from_secs(1)) {
            let values:Option<Vec<f64>>=IDS.iter().map(|id|feedback.get(id).and_then(|v|v["voltage"].as_f64()).filter(|v|v.is_finite() && *v>0.)).collect();
            if let Some(values)=values {
                let v=values.iter().sum::<f64>()/values.len() as f64;
                self.volts=Some(self.volts.map_or(v,|old|0.1*v+0.9*old));self.sampled=Some(now);
            }
        }
        self.volts.map_or(1.,|v|(7.4/v.clamp(6.,9.5)) as f32)
    }
}
struct Model {
    kind: String,
    progress: f64,
    upright: f64,
    session: std::sync::Arc<std::sync::Mutex<Session>>,
    command_twist: [f32;3],
    skill_profile: SkillProfile,
    walk_profile: Option<crate::walk_profile::WalkProfile>,
    target_initialized: bool,
    home: [f32; 14],
    sha: String,
    previous: Option<([f64; 14], [f64; 14])>,
    velocity: [f64; 14],
    last_action: [f32; 14],
    filtered_action: [f32; 14],
    filtered_gyro: Option<(Vector3<f64>, f64)>,
    gains: crate::gains::Gains,
    raw: Vec<f64>,
    sent: Vec<f64>,
    saturated: Vec<u8>,
}
// Bound both memory and capture cost. Preserve the first 12 policy seconds of
// each skill so long stationary holds cannot erase the transition diagnostic.
#[derive(Default)]
struct PolicyTrace { frames: std::collections::VecDeque<Value>, skill_frames: usize }
impl PolicyTrace {
    fn push(&mut self, frame: Value) {
        if self.frames.len()==2000 {self.frames.pop_front();}
        self.frames.push_back(frame);
    }
    fn transition(&mut self, frame: Value) {self.skill_frames=0;self.push(frame);}
    fn frame(&mut self, frame: Value) {
        if self.skill_frames<600 {self.push(frame);}
        self.skill_frames+=1;
    }
    fn save(&self, cal:&Value, result:&Result<Value>) -> Result<()> {
        if self.frames.is_empty() {return Ok(());}
        let calibration=PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE")
            .unwrap_or("./state/calibration.json".into()));
        let destination=calibration.parent().unwrap().join("policy-session-latest.json");
        let tmp=destination.with_extension("tmp");
        let value=json!({"schemaVersion":1,"eventTimeMs":epoch_ms(),"hardwareMotionAdded":false,
            "captureLimitPerSkill":600,"jointOrder":NAMES,"jointIds":ORDER,"calibration":cal,
            "result":match result {Ok(v)=>v.clone(),Err(e)=>json!({"error":e.to_string()})},"frames":self.frames});
        std::fs::write(&tmp,serde_json::to_vec(&value)?)?;
        std::fs::rename(tmp,destination)?;
        Ok(())
    }
}
fn same_sit_actor(from:&str,to:&str,sha_from:&str,sha_to:&str,home_from:&[f32;14],home_to:&[f32;14])->bool {
    from.starts_with("sitstand_") && to.starts_with("sitstand_") && sha_from==sha_to && home_from==home_to
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
    fn donor_sensors(&self)->bool { self.walk_profile.is_some() || self.skill_profile.donor() }
    fn filter_action(&self, raw:&[f32;14])->[f32;14] {
        if self.skill_profile.donor() { return *raw; }
        self.walk_profile.map_or_else(||smooth_skill(&self.kind,&self.filtered_action,raw),|p|p.filter(&self.filtered_action,raw,self.target_initialized))
    }
    fn open() -> Result<Self> { Self::open_for("stand", 0.0) }
    fn open_for(kind: &str, speed: f32) -> Result<Self> {
        let p = path_for(kind)?;
        let meta = verified_metadata_for(kind)?;
        let sha = meta["policySha256"].as_str().unwrap().to_owned();
        let walk_profile=crate::walk_profile::WalkProfile::parse(kind,&meta)?;
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
            command_twist: if SkillProfile::from_metadata(&meta).donor_pick {skill_twist(kind,0.)} else if kind=="sitstand_sit" {[1.,0.,0.]} else {[speed,0.,0.]},
            skill_profile: SkillProfile::from_metadata(&meta),
            walk_profile, target_initialized:false,
            home,
            sha,
            previous: None,
            velocity: [0.; 14],
            last_action: [0.; 14],
            filtered_action: [0.; 14],
            filtered_gyro: None,
            gains: if SkillProfile::from_metadata(&meta).donor() {crate::gains::Gains{p:5,d:20}} else {walk_profile.map_or_else(crate::gains::current,|p|crate::gains::walking(p.gains))},
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
            self.target_initialized=false;
            self.filtered_gyro = None;
        }
        let mut velocity = [0.; 14];
        for (n, id) in ORDER.iter().enumerate() {
            let direction = cal["joints"]["directions"].get(id.to_string())
                .and_then(Value::as_f64).unwrap_or(-1.);
            let raw = feedback[id]["velocityRaw"].as_f64()
                .ok_or_else(|| anyhow::anyhow!("#{id} 速度反馈缺失"))?;
            let measured = raw * 50. * std::f64::consts::TAU / 4096. * direction;
            velocity[n] = if self.donor_sensors() { measured } else if let Some((_, times)) = self.previous {
                let dt = stamps[n] - times[n];
                if dt > 1e-6 {
                    let alpha = 0.4f64.powf(dt / 0.01);
                    alpha * self.velocity[n] + (1. - alpha) * measured
                } else { self.velocity[n] }
            } else { measured };
        }
        let gyro = if self.donor_sensors() { gyro } else if let Some((old, at)) = self.filtered_gyro {
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
        let observed_velocity=if self.donor_sensors() {
            if let Some((_,times))=self.previous {
                std::array::from_fn(|n|if stamps[n]-times[n]<=0.1 {self.velocity[n]} else {velocity[n]})
            } else {velocity}
        } else {velocity};
        obs.extend(observed_velocity.map(|v|v as f32));
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
                let (goal, sent, saturated) = skill_target(*id, raw, cal, limits[id], self.skill_profile.bounded_targets)?;
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
fn skill_target(id:u8, raw:f64, cal:&Value, hardware:(i32,i32), bounded:bool)->Result<(i32,f64,bool)> {
    if !raw.is_finite() {bail!("#{id} 模型目标无效");}
    let angle=if bounded {
        let (low,high)=limit(id).ok_or_else(||anyhow::anyhow!("#{id} 无关节限位"))?;
        raw.clamp(low.to_radians(),high.to_radians())
    } else {raw};
    let (goal,sent,saturated)=donor_target(id,angle,cal,hardware)?;
    Ok((goal,sent,saturated || (angle-raw).abs()>1e-6))
}
// kissqy: clamp measured mechanical angle first, convert without wrapping,
// then reject unencodable goals. Keep this board's zero/sign and EEPROM bounds.
fn donor_target(id: u8, angle: f64, cal: &Value, hardware: (i32, i32)) -> Result<(i32, f64, bool)> {
    let key = id.to_string();
    let reference = cal["joints"]["references"][&key].as_f64()
        .filter(|v| v.is_finite()).ok_or_else(|| anyhow::anyhow!("#{id} 缺少位置标定"))?;
    let direction = cal["joints"]["directions"].get(&key).and_then(Value::as_f64).unwrap_or(-1.);
    if !angle.is_finite() || ![-1., 1.].contains(&direction) { bail!("#{id} 模型目标或标定无效"); }
    let (mut low, mut high) = (0, 4095);
    if hardware.1 > hardware.0 { low = low.max(hardware.0); high = high.min(hardware.1); }
    if low > high { bail!("#{id} 舵机硬件限位与编码范围不相交"); }
    let (min,max)=limit(id).ok_or_else(|| anyhow::anyhow!("#{id} 缺少机械角度限位"))?;
    let bounded=angle.clamp(min.to_radians(),max.to_radians());
    let raw = (reference + bounded * 4096. / std::f64::consts::TAU * direction).round();
    if !raw.is_finite() || raw < low as f64 || raw > high as f64 {
        bail!("#{id} 目标超出单圈编码范围或舵机硬件限位");
    }
    let sent = (raw - reference) * std::f64::consts::TAU / 4096. * direction;
    Ok((raw as i32, sent, bounded != angle))
}
/// Native donor policies own a synchronous tick: read -> observe -> infer ->
/// target write. There is no worker mailbox and no repeat write between ticks.
fn native_policy_loop(
    bus:&mut Bus, model:&mut Model, bank:&mut BTreeMap<String,Model>, cal:&Value,
    limits:&BTreeMap<u8,(i32,i32)>, cancel:&AtomicBool, halt:&AtomicBool, shared:&Shared,
    feedback:&mut Feedback, previous:&mut Feedback, goals:&mut BTreeMap<u8,i32>,
    last_obs:&mut Option<Vec<f32>>, infer_ms:&mut f64, timing:&mut control::Timing,
    trace:&mut PolicyTrace, costs:&std::sync::Mutex<CycleCosts>,
    stats:&impl Fn(&Model,&control::Timing,f64)->Value,
) -> Result<Value> {
    let return_home=model.skill_profile.donor_getup;
    let mut recovery_resume="xgoduck";
    let mut recovery_resume_progress=0.;
    let mut recovery_posture=NativeRecoveryPosture::default();
    let mut clock=control::FrameClock::new(Instant::now());
    let mut coast=control::FeedbackCoast::default();
    let mut imu_pause=ImuPause::default();
    let mut fall_guard=FallGuard::default();
    let mut paused:Option<Instant>=None;
    // Native kissqy owns all fifteen servos; the mouth is a separate intent,
    // never a scripted side effect of the ground-pick model.
    arm_current_profile(bus,&[34],feedback,(10,20),cancel,halt,true)?;
    let mouth_owned=true;
    let mut quack_cycle:Option<(Instant,crate::quack::Playback)>=None;
    shared.write().unwrap().mouth_degrees=-5.;
    let mouth_limits=control::configuration(bus,34)?;
    trace.transition(json!({"type":"start","mono":crate::telemetry::monotonic(),"kind":model.kind,"policySha256":model.sha,"execution":"kissqy-sync50-v1","returnHome":return_home}));
    while !cancel.load(Ordering::Acquire) && !halt.load(Ordering::Acquire) {
        let started=Instant::now();
        let (mut fresh,mut diagnostics)=control::poll_policy_tick(bus,&IDS);
        let unconfirmed=control::confirm_policy_faults(&mut fresh,&mut diagnostics,|ids| {
            let rows=bus.read_fast(ids)?;Ok((rows,bus.diagnostics.clone()))
        })?;
        bus.diagnostics=diagnostics.clone();
        for (id,row) in &fresh {control::validate(*id,Some(row),ORDER.contains(id) || (*id==34 && mouth_owned))?;}
        let sample=coast.sample(&fresh,&IDS,Instant::now()).filter(|_|!unconfirmed);
        let sensors=imu_samples(shared);
        let ready=imu_pause.assess(sensors,Instant::now())?;
        let observe_at=Instant::now();
        let observed=if ready {match &sample {
            Some(rows)=>match model.observe(shared,rows,cal) {
                Err(e) if e.is::<StaleImu>()=>{imu_pause.assess(Err(e),Instant::now())?;None},
                result=>observation_or_hold(result)?,
            },None=>None,
        }} else {None};
        {
            let mut c=costs.lock().unwrap();c.read(observe_at.duration_since(started).as_secs_f64()*1000.);
            c.cost(1,observe_at.elapsed().as_secs_f64()*1000.);
            let missing=IDS.iter().filter(|id|!fresh.contains_key(id)).count() as u64;
            c.missing_frames+=u64::from(missing>0);c.missing_responses+=missing;
            c.checksum_errors+=diagnostics["checksumErrors"].as_u64().unwrap_or(0);
        }
        bus.diagnostics=coast.diagnostics(diagnostics,observed.is_none());
        bus.diagnostics["feedbackTargetHz"]=json!(50);
        control::publish(shared,&fresh,&IDS,bus.diagnostics.clone());
        let Some(mut obs)=observed else {
            quack_cycle=None;shared.write().unwrap().mouth_degrees=-5.;
            pause_drive(shared);model.command_twist=[0.;3];model.upright=0.;
            if paused.is_none() {
                paused=Some(Instant::now());
                if let Some(rows)=&coast.last {
                    *goals=ORDER.iter().map(|id|(*id,rows[id]["position"].as_i64().unwrap() as i32)).collect();
                    if mouth_owned {goals.insert(34,rows[&34]["position"].as_i64().unwrap() as i32);}
                }
            }
            if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) {break;}
            control::goals(bus,goals)?;timing.record(Instant::now());
            let mut v=stats(model,timing,*infer_ms);v["state"]=json!("policy");v["action"]=json!("policy");
            v["policyPhase"]=json!("feedback-hold");v["feedbackHolding"]=json!(true);
            v["message"]=json!("传感器暂未就绪，暂停推理并保持测得姿态");control::status(shared,v);
            clock.wait();continue;
        };
        *previous=feedback.clone();*feedback=sample.unwrap();
        if paused.take().is_some_and(|at|at.elapsed()>=Duration::from_millis(200)) {
            model.previous=None;model.velocity=[0.;14];model.last_action=[0.;14];
            model.filtered_action=[0.;14];model.target_initialized=false;model.filtered_gyro=None;
            obs=model.observe(shared,feedback,cal)?;
        }
        if fall_guard.check(&model.kind,Some(tilt(&obs)),Instant::now()) {
            pause_drive(shared);bail!("检测到行走跌倒，停止推理并卸力");
        }
        let recovery_fallen=recovery_posture.observe(tilt(&obs),Instant::now());
        let requested=shared.write().unwrap().pending_skill.take();
        let finished=model.skill_profile.complete(&model.kind,model.progress,model.upright);
        if finished && model.skill_profile.donor_getup && home_posture_allowed(tilt(&obs)).is_err() {
            trace.transition(json!({"type":"recovery-failed","mono":crate::telemetry::monotonic(),
                "tiltDegrees":tilt(&obs),"progressSeconds":model.progress,"destination":"disabled"}));
            pause_drive(shared);
            bail!("倒地起身6秒窗口结束但机身仍倒地（{:.1}°），停止推理并卸力；未发HOME目标",tilt(&obs));
        }
        let handoff=native_handoff(&model.kind,finished,model.skill_profile.donor_getup,return_home,recovery_resume,requested);
        if handoff==NativeHandoff::Home {
            trace.transition(json!({"type":"recovery-complete","mono":crate::telemetry::monotonic(),"destination":"HOME","tiltDegrees":tilt(&obs),"progressSeconds":model.progress}));
            pause_drive(shared);
            let home:BTreeMap<_,_>=ORDER.into_iter().zip(model.home.map(|v|v as f64)).collect();
            let mut v=control::execute(bus,&json!({"action":"stand","calibration":cal,"standTargets":home,"nativePolicyGains":[model.gains.p,model.gains.d],"requireUpright":true}),cancel,shared)?;
            v["state"]=json!("holding");v["action"]=json!("policy");v["policyPhase"]=json!("home");
            v["message"]=json!("起身窗口结束，已返回HOME保持；行走需主动启动");
            v["recoveryWindowComplete"]=json!(true);v["recoveryUprightAtEnd"]=json!(tilt(&obs)<15.);
            return Ok(v);
        }
        if let NativeHandoff::Skill(next)=handoff {
            let reset=if next=="xgoduck_getup" {
                match native_recovery_transition(&model.kind,return_home,recovery_fallen) {
                    Ok((reset,resume))=>{recovery_resume=resume;recovery_resume_progress=model.progress;reset},
                    Err(e)=>{shared.write().unwrap().log("WARN","policy-switch",&e.to_string());clock.wait();continue;},
                }
            } else {false};
            let mut selected=bank.remove(&next).ok_or_else(||anyhow::anyhow!("动作模型未预加载"))?;
            let retain=!reset && ((model.donor_sensors() && selected.donor_sensors() && model.home==selected.home)
                || same_sit_actor(&model.kind,&selected.kind,&model.sha,&selected.sha,&model.home,&selected.home));
            // Sensor delay belongs to the hardware sample, not the actor.
            // reset_for_home clears actor history without sampling velocity twice.
            selected.previous=model.previous.take();selected.velocity=model.velocity;
            selected.filtered_gyro=model.filtered_gyro;
            selected.last_action=if retain {model.last_action}else{[0.;14]};
            selected.filtered_action=if retain {model.filtered_action}else{[0.;14]};selected.target_initialized=retain && model.target_initialized;
            selected.progress=if model.skill_profile.donor_getup && next=="sitstand_sit" {recovery_resume_progress} else {0.};
            selected.upright=0.;selected.command_twist=skill_twist(&next,selected.progress);
            control::native_profiles(bus,&ORDER,feedback,(selected.gains.p,selected.gains.d))?;
            // Gain changes only: no torque toggle and no stale pre-enable observation.
            if selected.gains!=model.gains {
                *feedback=bus.read_feedback(&IDS)?;
                for id in ORDER {control::validate(id,feedback.get(&id),true)?;
                    if feedback[&id]["kpRaw"]!=selected.gains.p || feedback[&id]["kdRaw"]!=selected.gains.d {bail!("技能切换增益回读不一致");}}
            }
            trace.transition(json!({"type":"transition","mono":crate::telemetry::monotonic(),"from":model.kind,"to":selected.kind,"policySha256":selected.sha,"retainedActions":retain,"recoveryFallen":recovery_fallen,"recoveryReturnHome":return_home,"recoveryResumeSkill":recovery_resume}));
            let old_home=model.home;
            let old=std::mem::replace(model,selected);bank.insert(old.kind.clone(),old);
            pause_drive(shared);
            // Rebase the one current sensor sample; only q-HOME and raw history
            // change. Never advance the native one-sample delay a second time.
            for n in 0..14 {obs[6+n]+=old_home[n]-model.home[n];obs[34+n]=model.last_action[n];}
        }
        model.command_twist=if model.kind=="xgoduck" {shared.read().unwrap().drive.as_ref().map(|d|d.current()).unwrap_or([0.;3])} else {skill_twist(&model.kind,model.progress)};
        obs[48..51].copy_from_slice(&model.command_twist);
        if walking_policy(&model.kind) && tilt(&obs)>=FALL_TILT_DEG {bail!("起身结束时仍倒地，拒绝进入行走，停止并卸力");}
        let (scale,filtered_voltage)={let mut state=shared.write().unwrap();
            let scale=state.native_voltage.scale(feedback,Instant::now());(scale,state.native_voltage.volts)};
        let (action,ms)=model.infer(obs.clone())?;*infer_ms=ms;*last_obs=Some(obs.clone());
        let adjusted=action.map(|v|v*scale);
        let filtered=model.filter_action(&adjusted);
        *goals=model.targets(filtered,feedback,cal,limits)?;
        let requested={let mut state=shared.write().unwrap();let v=state.pending_quack;state.pending_quack=false;if v {state.quack_running=true;}v};
        if requested {
            match crate::quack::Playback::start() {
                Ok(audio)=>{quack_cycle=Some((Instant::now(),audio));shared.write().unwrap().quack_running=true;}
                Err(e)=>{let mut state=shared.write().unwrap();state.quack_running=false;state.log("ERROR","quack",&e.to_string());},
            }
        }
        if let Some((started,audio))=quack_cycle.as_mut() {
            if let Err(e)=audio.check(){shared.write().unwrap().log("ERROR","quack",&e.to_string());}
            shared.write().unwrap().mouth_degrees=crate::quack::degrees(started.elapsed());
            if started.elapsed()>=crate::quack::DURATION {quack_cycle=None;shared.write().unwrap().quack_running=false;}
        }
        let mouth_request=shared.write().unwrap().pending_mouth.take();
        if let Some(degrees)=mouth_request {shared.write().unwrap().mouth_degrees=degrees;}
        if mouth_owned {
            let degrees=shared.read().unwrap().mouth_degrees;
            goals.insert(34,control::target(34,feedback[&34]["position"].as_i64().unwrap() as i32,cal,mouth_limits.0,mouth_limits.1,degrees.to_radians(),true)?);
        }
        if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) {break;}
        let write_at=Instant::now();control::goals(bus,goals)?;timing.record(write_at);
        trace.frame(json!({"type":"inference","mono":crate::telemetry::monotonic(),"kind":model.kind,"policySha256":model.sha,"commandTwist":model.command_twist,
            "observation":obs,"rawActionRadians":action,"emaPreviousRadians":model.filtered_action,"filteredActionRadians":filtered,"homeRadians":model.home,
            "preClipTargetDegrees":model.raw,"sentTargetDegrees":model.sent,"goals":goals,"feedbackBeforeWrite":feedback,"readDiagnostics":bus.diagnostics,
            "inferenceMs":ms,"resultAgeMs":0.,"voltageScale":scale,"filteredVoltage":filtered_voltage,"execution":"kissqy-sync50-v1"}));
        model.last_action=action;model.filtered_action=filtered;model.target_initialized=true;
        if model.kind!="xgoduck" {model.progress+=0.02;}
        {let mut c=costs.lock().unwrap();c.writes+=1;c.inference_results+=1;c.cost(2,write_at.elapsed().as_secs_f64()*1000.);c.cost(3,started.elapsed().as_secs_f64()*1000.);c.result_age_ms=0.;}
        let mut v=stats(model,timing,ms);v["state"]=json!("policy");v["action"]=json!("policy");v["policyPhase"]=json!("running");v["feedbackHolding"]=json!(false);
        v["voltageAdapt"]=json!(true);v["filteredVoltage"]=json!(filtered_voltage);v["voltageScale"]=json!(scale);v["skippedControlTicks"]=json!(clock.skipped);
        v["recoveryFallen"]=json!(recovery_fallen);v["recoveryReturnHome"]=json!(return_home);v["recoveryResumeSkill"]=json!(recovery_resume);
        v["message"]=json!(if model.kind=="xgoduck_getup" {"kissqy倒地起身，同步50Hz，6秒窗口"} else {"kissqy同步50Hz控制"});control::status(shared,v);
        clock.wait();
    }
    pause_drive(shared);
    let mut v=if cancel.load(Ordering::Acquire) {control::unload(bus)?} else {json!({"state":"holding","message":"模型已停止，保持最后目标"})};
    for (key,value) in stats(model,timing,*infer_ms).as_object().unwrap() {v[key]=value.clone();}
    v["action"]=json!("policy");Ok(v)
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
    let mut policy_trace=PolicyTrace::default();
    let mut feedback = Feedback::new();
    let mut previous = Feedback::new();
    let mut goals = BTreeMap::new();
    let costs=std::sync::Arc::new(std::sync::Mutex::new(CycleCosts::default()));
    let stats = |model: &Model, timing: &control::Timing, infer_ms: f64| -> Value {
        let mut v = timing.stats();
        v["fallDetection"]=json!({"enabled":walking_policy(&model.kind),"tiltDegrees":FALL_TILT_DEG,"confirmMs":FALL_CONFIRM.as_millis(),"immediateTiltDegrees":FALL_IMMEDIATE_DEG});
        v["commandTargetHz"]=json!(100);
        v["angleLimitsRevision"]=json!("kissqy-hd1910-saved-mechanical-v1");
        v["targetEncoding"]=json!("mechanical-clamp-single-turn-reject-v1");
        for (key,value) in json!({"policy":model_id(&model.kind),"kind":if is_xgoduck(kind) {"xgoduck"} else {kind},"requestedKind":kind,"activeSkill":model.kind,"skillProgressSeconds":model.progress,"sitSettleSeconds":model.skill_profile.sit_settle(),"modelSource":if model.skill_profile.retrained {"replica-hd1910"} else {"donor"},"posture":if model.kind=="sitstand_sit" {if model.progress>=model.skill_profile.sit_settle() {"seated"} else {"sitting"}} else if model.kind=="sitstand_stand" {"rising"} else {"standing"},"walkSpeedMps":speed,"policySha256":model.sha,"inferenceMs":control::rounded(infer_ms,3),"runtimeProfile":"xgoduck-schedule100-v1","homeCommandTargetHz":50,"feedbackReplyBudgetMs":6,"feedbackTargetHz":100,"inferenceTargetHz":50,"actionAlpha":if model.kind=="xgoduck_roulade" {0.15} else {0.45},"velocitySource":"servo-register","velocityAlphaAt100Hz":0.4,"kpRun":model.gains.p,"kdRun":model.gains.d,"kpHome":control::MANUAL_GAINS.0,"kdHome":control::MANUAL_GAINS.1,"saturatedIds":model.saturated,"rawTargetDegrees":model.raw,"sentTargetDegrees":model.sent}).as_object().unwrap(){v[key]=value.clone();}
        if model.skill_profile.donor() {
            v["runtimeProfile"]=json!(if model.skill_profile.kissqy {"kissqy-sitstand-v1"} else {"kissqy-skills-v1"});v["modelSource"]=json!("kissqy/Microduck-kissqy");
            v["actionAlpha"]=json!(0);v["actionScale"]=json!(1);v["velocitySource"]=json!("servo-register-one-sample-delay");v["velocityAlphaAt100Hz"]=Value::Null;
        }
        if let Some(profile)=model.walk_profile {
            v["runtimeProfile"]=json!("kissqy-r17-v1");v["modelSource"]=json!("kissqy/Microduck-kissqy");
            v["actionAlpha"]=Value::Null;v["actionScale"]=json!(profile.scale);
            v["headNewWeight"]=json!(profile.head);v["legsNewWeight"]=json!(profile.legs);
            v["velocityAlphaAt100Hz"]=Value::Null;v["velocitySource"]=json!("servo-register-one-sample-delay");
        }
        if model.donor_sensors() {
            v["runtimeProfile"]=json!("kissqy-sync50-v1");
            v["commandTargetHz"]=json!(50);v["feedbackTargetHz"]=json!(50);
            v["kpHome"]=json!(model.gains.p);v["kdHome"]=json!(model.gains.d);
            v["voltageAdapt"]=json!(true);v["nominalVoltage"]=json!(7.4);
            v["executionOrder"]=json!("read-observe-infer-write");
        }
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
        let native_ready=shared.read().unwrap().native_homed && ORDER.iter().all(|id|feedback[id]["torque"]==1);
        if command["shadow"]!=true && model.skill_profile.donor_getup && !native_ready {
            // kissqy only permits a limp -> LB edge after a confirmed fall.
            // Confirm with reads before arm_current_profile can write torque.
            stage="recovery-entry-posture";
            let mut posture=NativeRecoveryPosture::default();
            let mut entry_obs=initial_obs.clone();
            let mut entry_clock=control::FrameClock::new(Instant::now());
            loop {
                if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) {
                    return Ok(json!({"state":"holding","action":"policy","message":"起身预检查已取消，未发运动指令"}));
                }
                let angle=tilt(&entry_obs);
                if angle<=60. {bail!("未完成HOME或卸力状态仅确认倒地后可启动起身；当前倾角{angle:.1}°，未发运动指令");}
                if posture.observe(angle,Instant::now()) {break;}
                entry_clock.wait();feedback=bus.read_feedback(&IDS)?;
                entry_obs=model.observe(shared,&feedback,cal)?;
            }
        }
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
            let (compatible, reason) = match model.targets(model.filter_action(&action), &feedback, cal, &limits) {
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
                .map(|n| ((model.home[n] + model.filter_action(&action)[n]) as f64).to_degrees())
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
            arm_current_profile(bus,&ORDER,&feedback,if model.donor_sensors() {(model.gains.p,model.gains.d)} else {control::MANUAL_GAINS},cancel,halt,model.donor_sensors())?;
        } else {
            let mut home_command=json!({"action":"stand","calibration":cal,"standTargets":home,"requireUpright":true});
            if model.donor_sensors() {home_command["nativePolicyGains"]=json!([model.gains.p,model.gains.d]);}
            control::execute(bus,&home_command,cancel,shared)?;
        }
        if cancel.load(Ordering::Acquire) || halt.load(Ordering::Acquire) { return control::unload(bus); }
        // HOME completes with the manual gains. Switch only the model joints
        // after arrival; retain enabled state and HOME goals throughout.
        stage = "policy-profile";
        feedback = bus.read_feedback(&IDS)?;
        for id in ORDER { control::validate(id,feedback.get(&id),true)?; }
        if model.donor_sensors() {control::native_profiles(bus,&ORDER,&feedback,(model.gains.p,model.gains.d))?;}
        else {control::profiles_with_gains(bus,&ORDER,&feedback,(model.gains.p,model.gains.d))?;}
        feedback = bus.read_feedback(&IDS)?;
        for id in ORDER {
            control::validate(id,feedback.get(&id),true)?;
            let (kp,kd)=(model.gains.p,model.gains.d);
            if feedback[&id]["kpRaw"]!=kp || feedback[&id]["kdRaw"]!=kd {
                bail!("#{id} 模型增益设置未确认");
            }
        }
        model.previous = None;
        model.velocity = [0.; 14];
        model.last_action = [0.; 14];
        model.filtered_action = [0.; 14];
        model.target_initialized=false;
        model.filtered_gyro = None;
        goals=if matches!(kind,"xgoduck_getup"|"sitstand_stand") { ORDER.iter().map(|id|(*id,feedback[id]["position"].as_i64().unwrap() as i32)).collect() }
            else {model.targets([0.;14],&feedback,cal,&limits)?};
        if model.donor_sensors() {
            stage="policy-loop";
            return native_policy_loop(bus,&mut model,&mut bank,cal,&limits,cancel,halt,shared,&mut feedback,&mut previous,&mut goals,
                &mut last_obs,&mut infer_ms,&mut timing,&mut policy_trace,&costs,&stats);
        }
        let mouth_limits = if is_xgoduck(kind) {Some(control::configuration(bus,34)?)} else {None};
        let mut mouth_owned = false;
        if kind=="xgoduck_pick" { arm_current(bus,&[34],&feedback,control::MANUAL_GAINS,cancel,halt)?; mouth_owned=true; }
        let mut worker=InferenceWorker::new(model.session.clone());
        let mut generation=0u64;
        stage = "policy-loop";
        policy_trace.transition(json!({"type":"start","mono":crate::telemetry::monotonic(),"kind":model.kind,"policySha256":model.sha}));
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
            let mut accepted_trace=None;
            let now=crate::telemetry::monotonic();
            let feedback_recent=ORDER.iter().all(|id|feedback.get(id).and_then(|r|r["_receivedMono"].as_f64()).is_some_and(|at|(0.0..0.150).contains(&(now-at))));
            if paused.is_none() && feedback_recent {
                if let Some((result_generation,submitted,result_obs,result))=worker.take() {
                    if result_generation==generation && submitted.elapsed()<Duration::from_millis(150) {
                        costs.lock().unwrap().result_age_ms=submitted.elapsed().as_secs_f64()*1000.;
                        let (action,ms)=result?;infer_ms=ms;last_obs=Some(result_obs);
                        let filtered=model.filter_action(&action);
                        if model.kind=="xgoduck_getup" || (model.skill_profile.retrained && model.kind=="sitstand_stand") {
                            model.upright=if model.skill_profile.upright(last_obs.as_ref().unwrap()) {model.upright+0.02} else {0.};
                        }
                        goals=model.targets(filtered,&feedback,cal,&limits)?;
                        if policy_trace.skill_frames<600 {
                            accepted_trace=Some(json!({"type":"inference","mono":crate::telemetry::monotonic(),
                                "kind":model.kind,"policySha256":model.sha,"commandTwist":model.command_twist,
                                "observation":last_obs,"rawActionRadians":action,"emaPreviousRadians":model.filtered_action,
                                "filteredActionRadians":filtered,"homeRadians":model.home,
                                "preClipTargetDegrees":model.raw,"sentTargetDegrees":model.sent,"goals":goals,
                                "saturatedIds":model.saturated,"inferenceMs":ms,"resultAgeMs":submitted.elapsed().as_secs_f64()*1000.}));
                        }
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
            if let Some(mut frame)=accepted_trace {
                frame["feedbackAfterWrite"]=json!(fresh);
                frame["readDiagnostics"]=diagnostics.clone();
                policy_trace.frame(frame);
            } else if accepted_action.is_some() {policy_trace.skill_frames+=1;}
            if let Some((action,filtered))=accepted_action {
                model.last_action=action;model.filtered_action=filtered;model.target_initialized=true;
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
        model.target_initialized=false;
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
                let next = requested.or_else(||model.skill_profile.complete(&model.kind,model.progress,model.upright)
                    .then(||if model.kind!="sitstand_stand" && tilt(&obs)>55. {"xgoduck_getup".to_owned()} else {"xgoduck".to_owned()}));
                if let Some(next) = next.filter(|next|next!=&model.kind) {
                    if next!="xgoduck_getup" && tilt(&obs)>55. { bail!("当前姿态不支持拾取/翻滚或返回平衡"); }
                    shared.write().unwrap().mouth_degrees=0.;
                    if next=="xgoduck_pick" && !mouth_owned {
                        arm_current(bus,&[34],&feedback,control::MANUAL_GAINS,cancel,halt)?; mouth_owned=true;
                    }
                    let mut selected=bank.remove(&next).ok_or_else(||anyhow::anyhow!("动作模型未预加载"))?;
                    let retain_actions=(model.donor_sensors() && selected.donor_sensors() && model.home==selected.home) || same_sit_actor(&model.kind,&selected.kind,&model.sha,&selected.sha,&model.home,&selected.home);
                    selected.previous=model.previous.take();selected.velocity=model.velocity;
                    // Each banked model keeps its own runtime P/D. Confirm changes before inference.
                    if selected.gains!=model.gains {
                        control::profiles_with_gains(bus,&ORDER,&feedback,(selected.gains.p,selected.gains.d))?;
                        feedback=bus.read_feedback(&IDS)?;
                        for id in ORDER {
                            control::validate(id,feedback.get(&id),true)?;
                            if feedback[&id]["kpRaw"]!=selected.gains.p || feedback[&id]["kdRaw"]!=selected.gains.d {bail!("技能切换增益回读不一致");}
                        }
                    }
                    selected.filtered_gyro=model.filtered_gyro.take();
                    if selected.donor_sensors()!=model.donor_sensors() {selected.previous=None;selected.velocity=[0.;14];selected.filtered_gyro=None;}
                    selected.target_initialized=retain_actions && model.target_initialized;
                    selected.last_action=if retain_actions {model.last_action} else {[0.;14]};
                    selected.filtered_action=if retain_actions {model.filtered_action} else {[0.;14]};
                    selected.progress=0.;selected.upright=0.;selected.command_twist=skill_twist(&next,0.);
                    policy_trace.transition(json!({"type":"transition","mono":crate::telemetry::monotonic(),
                        "from":model.kind,"to":selected.kind,"policySha256":selected.sha,"retainedActions":retain_actions,
                        "initialRawActionRadians":selected.last_action,"initialFilteredActionRadians":selected.filtered_action}));
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
                v["message"] = json!(if model.kind=="sitstand_sit" {"坐下模型维持坐姿，点站起恢复行走"} else if model.kind=="sitstand_stand" {"站起模型过渡中"} else if model.kind=="xgoduck_getup" {if model.skill_profile.donor_getup {"kissqy倒地起身，6有效秒后尝试返回平衡"}else{"倒地起身中，直立稳定后返回零速度平衡"}} else if model.kind=="xgoduck_pick" {"XgoDuck 拾取中"} else if model.kind=="xgoduck_roulade" {"XgoDuck 翻滚中"} else if is_xgoduck(kind) { if model.command_twist==[0.;3] {"XgoDuck 原地平衡，按住方向键行走"} else {"XgoDuck 按住行走中"} } else if kind=="walk" { "v6 模型行走中" } else { "v5 模型站立维持中" });
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
        Ok(v) => {
            if let Err(error)=policy_trace.save(cal,&Ok(v.clone())) {
                shared.write().unwrap().log("WARN","policy-trace",&format!("模型逐帧诊断保存失败：{error}"));
            }
            Ok(v)
        },
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
            // Diagnostics must never delay fault-triggered torque disable.
            if let Err(error)=policy_trace.save(cal,&Err(anyhow::anyhow!(e.to_string()))) {
                shared.write().unwrap().log("WARN","policy-trace",&format!("模型逐帧诊断保存失败：{error}"));
            }
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
    fn same_actor_command_change_preserves_history_only_for_identical_sit_models() {
        let home=[0.;14];
        assert!(same_sit_actor("sitstand_sit","sitstand_stand","a","a",&home,&home));
        assert!(!same_sit_actor("xgoduck","sitstand_sit","a","a",&home,&home));
        assert!(!same_sit_actor("sitstand_sit","sitstand_stand","a","b",&home,&home));
        assert!(!same_sit_actor("sitstand_sit","sitstand_stand","a","a",&home,&[1.;14]));
    }
    #[test]
    fn trace_is_bounded_and_preserves_transition_after_long_hold() {
        let mut trace=PolicyTrace::default();
        trace.transition(json!({"type":"transition"}));
        for n in 0..10000 {trace.frame(json!({"n":n}));}
        assert_eq!(trace.frames.len(),601);
        assert_eq!(trace.frames[0]["type"],"transition");
        assert_eq!(trace.frames.back().unwrap()["n"],599);
        for _ in 0..2400 {trace.transition(json!({"type":"transition"}));}
        assert_eq!(trace.frames.len(),2000);
    }
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
    fn physical_drive_commands_reach_donor_without_reverse_or_yaw_multiplier() {
        for (input,expected) in [([0.,0.,0.],[0.,0.,0.]),([0.2,0.,0.],[0.2,0.,0.]),
            ([0.4,0.,0.],[0.4,0.,0.]),([-0.4,0.,0.],[-0.4,0.,0.]),([-0.2,0.,0.],[-0.2,0.,0.]),([-0.1,0.1,0.25],[-0.1,0.1,0.25]),
            ([0.,0.,0.5],[0.,0.,0.5]),([0.,0.,-0.5],[0.,0.,-0.5])] {
            assert_eq!(drive_twist(&json!({"twist":input})).unwrap(),expected);
        }
        for input in [[-0.401,0.,0.],[0.401,0.,0.],[0.,0.101,0.],[0.,0.,0.501],[0.,0.,-0.501]] {
            assert!(drive_twist(&json!({"twist":input})).is_err());
        }
    }
    #[test]
    fn native_voltage_matches_donor_mean_cadence_ema_and_clamp() {
        let now=Instant::now();let rows=|v:f64| IDS.into_iter().map(|id|(id,json!({"voltage":v}))).collect();
        let mut voltage=NativeVoltage::default();
        assert!((voltage.scale(&rows(8.),now)-0.925).abs()<1e-6);
        assert!((voltage.scale(&rows(6.),now+Duration::from_millis(999))-0.925).abs()<1e-6);
        assert!((voltage.scale(&rows(6.),now+Duration::from_secs(1))-7.4/7.8).abs()<1e-6);
        let mut low=NativeVoltage::default();assert!((low.scale(&rows(5.),now)-7.4/6.).abs()<1e-6);
        let mut high=NativeVoltage::default();assert!((high.scale(&rows(10.),now)-7.4/9.5).abs()<1e-6);
        let before=voltage.volts;voltage.scale(&Feedback::new(),now+Duration::from_secs(2));assert_eq!(voltage.volts,before);
    }
    #[test]
    fn donor_getup_finishes_at_300_nominal_ticks_even_when_not_upright() {
        let profile=SkillProfile{donor_getup:true,..Default::default()};
        let mut time=0.;for _ in 0..299 {time+=0.02;}
        assert!(!profile.complete("xgoduck_getup",time,0.));time+=0.02;
        assert!(profile.complete("xgoduck_getup",time,0.));
    }
    #[test]
    fn expired_recovery_does_not_authorize_home_while_still_fallen() {
        let profile=SkillProfile{donor_getup:true,..Default::default()};
        assert!(profile.complete("xgoduck_getup",6.,0.));
        for angle in [60.,94.70054285503988,180.,f64::NAN] {
            assert!(home_posture_allowed(angle).is_err());
        }
        assert!(home_posture_allowed(11.246106389039467).is_ok());
        assert!(home_posture_allowed(59.99).is_ok());
    }
    #[test]
    fn donor_recovery_arbitrates_origin_posture_and_underlying_seat() {
        assert_eq!(native_recovery_transition("xgoduck_getup",true,false).unwrap(),(true,"xgoduck"));
        assert_eq!(native_recovery_transition("xgoduck",false,false).unwrap(),(false,"xgoduck"));
        assert_eq!(native_recovery_transition("sitstand_sit",false,false).unwrap(),(false,"sitstand_sit"));
        assert_eq!(native_recovery_transition("sitstand_stand",false,false).unwrap(),(false,"xgoduck"));
        for active in ["xgoduck_pick","xgoduck_roulade"] {
            assert!(native_recovery_transition(active,false,false).is_err());
            assert_eq!(native_recovery_transition(active,false,true).unwrap(),(true,"xgoduck"));
        }
        assert_eq!(native_recovery_transition("sitstand_sit",false,true).unwrap(),(true,"xgoduck"));
    }
    #[test]
    fn donor_fallen_arbitration_requires_200ms_and_clears_on_upright_sample() {
        let now=Instant::now();let mut posture=NativeRecoveryPosture::default();
        assert!(!posture.observe(61.,now));
        assert!(!posture.observe(61.,now+Duration::from_millis(199)));
        assert!(posture.observe(61.,now+Duration::from_millis(200)));
        assert!(!posture.observe(60.,now+Duration::from_millis(201)));
        assert!(!posture.observe(61.,now+Duration::from_millis(202)));
    }
    #[test]
    fn donor_recovery_expiry_preserves_origin_and_duplicate_press_cannot_extend_window() {
        let profile=SkillProfile{donor_getup:true,..Default::default()};
        for origin_home in [true,false] {
            assert_eq!(native_handoff("xgoduck_getup",profile.complete("xgoduck_getup",5.98,0.),true,
                origin_home,"xgoduck",Some("xgoduck_getup".into())),NativeHandoff::Continue);
            assert_eq!(native_handoff("xgoduck_getup",profile.complete("xgoduck_getup",6.,0.),true,
                origin_home,"xgoduck",Some("xgoduck_getup".into())),
                if origin_home {NativeHandoff::Home} else {NativeHandoff::Skill("xgoduck".into())});
        }
        assert_eq!(native_handoff("xgoduck_getup",true,true,true,"xgoduck",Some("sitstand_sit".into())),NativeHandoff::Home);
        assert_eq!(native_handoff("xgoduck_getup",true,true,false,"sitstand_sit",None),NativeHandoff::Skill("sitstand_sit".into()));
    }
    #[test]
    fn kissqy_clamps_mechanical_angle_then_rejects_invalid_encoding() {
        let cal=json!({"joints":{"references":{"10":2079},"directions":{}}});
        let (goal,sent,saturated)=donor_target(10,15.297f64.to_radians(),&cal,(0,4095)).unwrap();
        assert_eq!(goal,1905);assert!(sent.to_degrees()>12.6051);assert!(!saturated);
        let (goal,_,saturated)=donor_target(10,400f64.to_radians(),&cal,(0,4095)).unwrap();
        assert_eq!(goal,1795);assert!(saturated);
        assert!(donor_target(10,400f64.to_radians(),&cal,(2000,3000)).is_err());
        let edge=json!({"joints":{"references":{"10":50},"directions":{}}});
        assert!(donor_target(10,25f64.to_radians(),&edge,(0,4095)).is_err());
        assert!(donor_target(10,f64::NAN,&cal,(0,4095)).is_err());
        assert!(donor_target(10,0.,&cal,(5000,6000)).is_err());
        assert!(control::angle_request(&json!(10),&json!(15.297)).is_ok());
    }
    #[test]
    fn contact_candidate_bounds_all_targets_but_keeps_calibration_and_encoder_bounds() {
        for id in ORDER {
            let key=id.to_string();
            for direction in [-1.,1.] {
                let cal=json!({"joints":{"references":{key.clone():2048},"directions":{key.clone():direction}}});
                let (lo,hi)=limit(id).unwrap();
                for (raw,expected) in [(-9.188,lo.to_radians()),(9.188,hi.to_radians())] {
                    let (_,sent,saturated)=skill_target(id,raw,&cal,(0,4095),true).unwrap();
                    assert!((sent-expected).abs()<=std::f64::consts::TAU/8192.);
                    assert!(saturated);
                }
                assert!(skill_target(id,9.188,&cal,(2048,2049),true).is_err());
                assert!(skill_target(id,f64::NAN,&cal,(0,4095),true).is_err());
            }
        }
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
    fn kissqy_pick_and_getup_contracts_and_handoffs() {
        for (kind,text,duration) in [
            ("xgoduck_pick",include_str!("../../models/xgoduck_pick.metadata.json"),2.8),
            ("xgoduck_getup",include_str!("../../models/xgoduck_getup.metadata.json"),6.)] {
            let meta:Value=serde_json::from_str(text).unwrap();
            validate_skill_metadata(kind,&meta).unwrap();let p=SkillProfile::from_metadata(&meta);
            assert!(p.donor());assert!(!p.complete(kind,duration-0.02,100.));
            assert!(p.complete(kind,duration,0.));
            assert!(validate_skill_metadata("sitstand_sit",&meta).is_err());
            let mut bad=meta.clone();bad["skillExecution"]["durationSeconds"]=json!(100);
            assert!(validate_skill_metadata(kind,&bad).is_err());
            bad=meta.clone();bad["skillExecution"]["policySha256"]=json!("wrong");
            assert!(validate_skill_metadata(kind,&bad).is_err());
        }
        assert_eq!(skill_twist("xgoduck_pick",0.),[1.,0.,0.]);
        let quarter=skill_twist("xgoduck_pick",1.);assert!(quarter[0].abs()<1e-6 && (quarter[1]-1.).abs()<1e-6);
        assert_eq!(skill_twist("xgoduck_getup",3.),[0.;3]);
    }

    #[test]
    fn kissqy_sitstand_uses_donor_timing_and_rejects_mismatched_contract() {
        let meta:Value=serde_json::from_str(include_str!("../../models/alpha_sitstand.metadata.json")).unwrap();
        for kind in ["sitstand_sit","sitstand_stand"] {validate_skill_metadata(kind,&meta).unwrap();}
        let p=SkillProfile::from_metadata(&meta);
        assert!(p.kissqy && !p.retrained && !p.bounded_targets);
        assert_eq!(p.sit_settle(),4.);
        assert!(!p.complete("sitstand_stand",0.98,10.));
        assert!(p.complete("sitstand_stand",1.,0.));
        assert!(!p.complete("sitstand_sit",100.,10.));
        let mut bad=meta.clone();bad["sitExecution"]["policySha256"]=json!("wrong");
        assert!(validate_skill_metadata("sitstand_sit",&bad).is_err());
        bad=meta.clone();bad["sitExecution"]["gains"]["p"]=json!(3);
        assert!(validate_skill_metadata("sitstand_stand",&bad).is_err());
        bad=meta.clone();bad["actionEmaOldWeight"]=json!(0.45);
        assert!(validate_skill_metadata("sitstand_sit",&bad).is_err());
        assert!(validate_skill_metadata("xgoduck",&meta).is_err());
    }

    #[test]
    fn replica_skill_metadata_and_transition_contract() {
        let getup: Value=serde_json::from_str(include_str!("../tests/fixtures/replica-getup.metadata.json")).unwrap();
        let seated: Value=serde_json::from_str(include_str!("../tests/fixtures/replica-sitstand.metadata.json")).unwrap();
        validate_skill_metadata("xgoduck_getup",&getup).unwrap();
        validate_skill_metadata("sitstand_sit",&seated).unwrap();
        validate_skill_metadata("sitstand_stand",&seated).unwrap();
        assert!(validate_skill_metadata("xgoduck_pick",&getup).is_err());
        let mut wrong=seated.clone();wrong["standSeconds"]=json!(1);
        assert!(validate_skill_metadata("sitstand_stand",&wrong).is_err());
        let mut candidate=seated.clone();
        candidate["task"]=json!("Mjlab-SitStandContact-Flat-MicroDuck-HD1910");
        candidate["skillPhysicsRevision"]=json!("sit-self-contact-bounded-target-v3");
        candidate["trainingTargetClipRadians"]=json!({});
        let archived:Value=serde_json::from_str(include_str!("../tests/fixtures/xgoduck-joint-limits.json")).unwrap();
        for name in NAMES {
            candidate["trainingTargetClipRadians"][name]=archived["joints"][name]["range"].clone();
        }
        validate_skill_metadata("sitstand_sit",&candidate).unwrap();
        assert!(SkillProfile::from_metadata(&candidate).bounded_targets);
        candidate["trainingTargetClipRadians"]["right_knee"]=json!([-10.,10.]);
        assert!(validate_skill_metadata("sitstand_sit",&candidate).is_err());
        let profile=SkillProfile::from_metadata(&seated);
        assert_eq!(profile.sit_settle(),4.);
        assert!(!profile.complete("sitstand_stand",1.,2.));
        assert!(!profile.complete("sitstand_stand",4.,0.5));
        assert!(profile.complete("sitstand_stand",4.,1.));
        assert!(!profile.complete("xgoduck_getup",2.,2.));
        assert!(profile.complete("xgoduck_getup",3.,1.));
        let mut obs=vec![0f32;61];obs[5]=-1.;
        assert!(profile.upright(&obs));
        for n in (0..5).chain(9..14) {obs[6+n]=0.6;}
        assert!(!profile.upright(&obs),"upright torso with folded legs is not standing");
        for n in 6..20 {obs[n]=0.;}obs[0]=3.;
        assert!(!profile.upright(&obs),"moving torso must not complete recovery");
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
        wire.writes.clear();wire.row["torque"]=json!(0);wire.row["accelerationRaw"]=json!(7);
        let feedback=Feedback::from([(34,wire.row.clone())]);
        arm_current_profile(&mut wire,&[34],&feedback,(5,20),&stop,&halt,true).unwrap();
        assert_eq!(wire.writes,vec![42,50,40,42]);assert_eq!(wire.row["accelerationRaw"],7);
        assert_eq!(wire.row["kpRaw"],5);assert_eq!(wire.row["kdRaw"],20);
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
        assert_eq!(request_profile(&json!({"kind":"walk","speed":0.4})).unwrap(), ("walk", 0.4));
        assert_eq!(request_profile(&json!({"kind":"walk","speed":0.1})).unwrap(), ("walk", 0.1));
        for body in [json!({"kind":"../walk"}),json!({"kind":null}),json!({"kind":"walk","speed":-0.1}),json!({"kind":"walk","speed":0.401}),json!({"kind":"walk","speed":"0.1"}),json!({"speed":0.1})] {
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
        let mut model = Model::open_for(fixture["kind"].as_str().unwrap_or("stand"),fixture["speed"].as_f64().unwrap_or(0.) as f32).unwrap();
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
