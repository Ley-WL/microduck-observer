use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, RwLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
pub type Shared = Arc<RwLock<Telemetry>>;
pub fn monotonic() -> f64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}
pub const IDS: [u8; 15] = [10, 11, 12, 13, 14, 20, 21, 22, 23, 24, 30, 31, 32, 33, 34];
pub fn epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
pub fn limit(id: u8) -> Option<(f64, f64)> {
    Some(match id {
        10 => ((-0.45f64).to_degrees(), (0.22f64).to_degrees()),
        20 => ((-0.22f64).to_degrees(), (0.45f64).to_degrees()),
        11 | 21 => (-22., 22.),
        12..=14 | 22..=24 => (-90., 90.),
        31 => (-105., 105.),
        30 => (-135., 60.),
        32 => (-170., 170.),
        33 => (-25., 25.),
        34 => (0., 30.),
        _ => return None,
    })
}
pub fn rates(hardware: bool, servos: bool, tof: bool) -> BTreeMap<String, Value> {
    let mut r = BTreeMap::from([
        ("imu.raw".into(), json!(50)),
        ("system".into(), json!(1)),
        ("logs".into(), Value::Null),
    ]);
    r.insert(
        if hardware { "imu.orientation" } else { "pose" }.into(),
        json!(50),
    );
    if servos {
        r.insert("joints".into(), json!(50));
    }
    if tof {
        r.insert("tof".into(), json!(20));
    }
    r
}
pub struct Telemetry {
    pub pending_skill: Option<String>,
    pub pending_mouth: Option<f64>,
    pub mouth_degrees: f64,
    pub drive: Option<DriveCommand>,
    pub boot: String,
    pub source: String,
    pub started: Instant,
    pub latest: BTreeMap<String, Value>,
    pub logs: VecDeque<Value>,
    seq: BTreeMap<String, u64>,
    pub control: Value,
    pub ble_health: Value,
    pub imu: Value,
    pub tof: Value,
    pub scenario: String,
    pub resources: Value,
}
pub struct DriveCommand {
    pub session: String,
    pub sequence: u64,
    pub twist: [f32; 3],
    pub updated: Instant,
}
impl Telemetry {
    pub fn queue_mouth(&mut self,session:&str,sequence:u64,degrees:f64)->bool {
        if !degrees.is_finite() || !(0. ..=30.).contains(&degrees) || self.control["state"]!="policy" || self.control["kind"]!="xgoduck" || self.control["activeSkill"]!="xgoduck" || self.control["feedbackHolding"]==true || self.pending_skill.is_some() {return false;}
        let Some(drive)=self.drive.as_mut() else {return false;};
        if !drive.update(session,sequence,[0.;3]) {return false;}
        self.pending_mouth=Some(degrees);true
    }
    pub fn queue_skill(&mut self, session:&str, sequence:u64, kind:&str) -> bool {
        if self.control["state"]!="policy" || self.control["kind"]!="xgoduck"
            || self.control["feedbackHolding"]==true || !(self.control["activeSkill"]=="xgoduck" || (kind=="sitstand_stand" && self.control["activeSkill"]=="sitstand_sit" && self.control["skillProgressSeconds"].as_f64().unwrap_or(0.)>=2.))
            || self.pending_skill.is_some() { return false; }
        let Some(drive)=self.drive.as_mut() else {return false;};
        if !drive.update(session,sequence,[0.;3]) {return false;}
        self.pending_mouth=None;self.pending_skill=Some(kind.to_owned());true
    }
}
impl DriveCommand {
    pub fn update(&mut self, session: &str, sequence: u64, twist: [f32;3]) -> bool {
        if session!=self.session || sequence<=self.sequence {return false;}
        self.sequence=sequence;self.twist=twist;self.updated=Instant::now();true
    }
    pub fn current(&self) -> [f32; 3] {
        if self.updated.elapsed() < std::time::Duration::from_millis(250) { self.twist } else { [0.; 3] }
    }
}
#[cfg(test)]
mod drive_tests {
    use super::*;
    #[test]
    fn seated_only_accepts_rise_after_settle_with_fresh_session() {
        let mut state=Telemetry::new(true);
        state.drive=Some(DriveCommand{session:"live".into(),sequence:0,twist:[0.;3],updated:Instant::now()});
        state.control=json!({"state":"policy","kind":"xgoduck","activeSkill":"sitstand_sit","skillProgressSeconds":1.9});
        assert!(!state.queue_skill("live",1,"sitstand_stand"));
        state.control["skillProgressSeconds"]=json!(2.);
        assert!(!state.queue_skill("foreign",1,"sitstand_stand"));
        assert!(!state.queue_skill("live",1,"xgoduck_pick"));
        assert!(!state.queue_mouth("live",1,30.));
        assert!(state.queue_skill("live",1,"sitstand_stand"));
        assert!(!state.queue_skill("live",2,"sitstand_stand"));
    }
    #[test]
    fn mouth_shares_sequence_and_rejects_skill_hold_and_bad_angles() {
        let mut state=Telemetry::new(true);
        state.control=json!({"state":"policy","kind":"xgoduck","activeSkill":"xgoduck"});
        state.drive=Some(DriveCommand{session:"live".into(),sequence:4,twist:[0.2,0.,0.],updated:Instant::now()});
        assert!(!state.queue_mouth("old",5,30.));
        assert!(!state.queue_mouth("live",5,31.));
        assert!(state.queue_mouth("live",5,30.));
        assert_eq!(state.drive.as_ref().unwrap().twist,[0.;3]);
        assert!(!state.queue_mouth("live",5,0.));
        assert!(state.queue_skill("live",6,"xgoduck_pick"));
        assert!(!state.queue_mouth("live",7,0.));
        state.pending_skill=None;state.control["feedbackHolding"]=json!(true);
        assert!(!state.queue_mouth("live",7,0.));
    }
    #[test]
    fn skill_queue_rejects_foreign_replayed_busy_and_held_requests() {
        let mut state=Telemetry::new(true);
        state.control=json!({"state":"policy","kind":"xgoduck","activeSkill":"xgoduck","feedbackHolding":false});
        state.drive=Some(DriveCommand {session:"live".into(),sequence:5,twist:[0.2,0.,0.],updated:Instant::now()});
        assert!(!state.queue_skill("old",6,"xgoduck_pick"));
        assert!(!state.queue_skill("live",5,"xgoduck_pick"));
        assert!(state.queue_skill("live",6,"xgoduck_pick"));
        assert_eq!(state.drive.as_ref().unwrap().twist,[0.;3]);
        assert!(!state.queue_skill("live",7,"xgoduck_roulade"));
        state.pending_skill=None;state.control["activeSkill"]=json!("transitioning");
        assert!(!state.queue_skill("live",7,"xgoduck_roulade"));
        state.control["activeSkill"]=json!("xgoduck");state.control["feedbackHolding"]=json!(true);
        assert!(!state.queue_skill("live",7,"xgoduck_getup"));
    }
    #[test]
    fn released_commands_cannot_be_overwritten_by_late_packets_or_old_sessions() {
        let mut d=DriveCommand{session:"new".into(),sequence:0,twist:[0.;3],updated:Instant::now()};
        assert!(d.update("new",1,[0.2,0.,0.]));
        assert!(d.update("new",3,[0.;3]));
        assert!(!d.update("new",2,[0.2,0.,0.]));
        assert!(!d.update("old",4,[0.2,0.,0.]));
        assert_eq!(d.current(),[0.;3]);
    }
    #[test]
    fn expired_held_command_returns_zero_without_a_browser_release() {
        let d=DriveCommand{session:"s".into(),sequence:1,twist:[0.2,0.,0.],updated:Instant::now()-std::time::Duration::from_millis(251)};
        assert_eq!(d.current(),[0.;3]);
    }
}
impl Telemetry {
    pub fn imu_health(&self) -> Value {
        if self.imu.is_null() {
            return Value::Null;
        }
        let mut h = self.imu.clone();
        let age = self
            .latest
            .get("imu.orientation")
            .map(|s| self.stamp(s)["ageMs"].clone())
            .unwrap_or(Value::Null);
        let valid = self
            .latest
            .get("imu.orientation")
            .is_some_and(|s| s["valid"] == true);
        h["sampleAgeMs"] = age.clone();
        h["state"] = json!(if valid && age.as_f64().is_some_and(|a| a < 1500.) {
            "streaming"
        } else {
            "unavailable"
        });
        h
    }
    pub fn tof_health(&self) -> Value {
        let mut h = self.tof.clone();
        let age = self
            .latest
            .get("tof")
            .map(|s| self.stamp(s)["ageMs"].clone())
            .unwrap_or(Value::Null);
        h["ageMs"] = age.clone();
        if h["state"] == "streaming" && age.as_f64().is_some_and(|v| v > 1500.) {
            h["state"] = json!("stale")
        };
        h
    }
    pub fn new(hardware: bool) -> Self {
        let mut s = Self {
            drive: None,
            pending_skill: None,
            pending_mouth: None, mouth_degrees: 0.,
            resources: Value::Null,
            boot: Uuid::new_v4().to_string(),
            source: if hardware { "hardware" } else { "simulation" }.into(),
            started: Instant::now(),
            latest: BTreeMap::new(),
            logs: VecDeque::new(),
            seq: BTreeMap::new(),
            control: json!({"state":"idle","message":"等待操作"}),
            ble_health: json!({"state":"disabled","backend":"rust","protocol":1}),
            imu: Value::Null,
            tof: json!({"state":"offline","error":"ToF stream timeout","ageMs":null,"hz":0}),
            scenario: "motion".into(),
        };
        s.log("INFO", "server", "Rust 后端启动");
        s
    }
    pub fn sample(&mut self, topic: &str, data: Value, valid: bool, received: Instant) {
        let n = self.seq.entry(topic.into()).or_default();
        *n += 1;
        let item = json!({"type":"sample","protocolVersion":1,"bootId":self.boot,"topic":topic,"seq":n,"sampleMonoMs":received.saturating_duration_since(self.started).as_secs_f64()*1000.,"source":self.source,"valid":valid,"data":data});
        self.latest.insert(topic.into(), item);
    }
    pub fn stamp(&self, item: &Value) -> Value {
        let mut v = item.clone();
        v["ageMs"] = json!((self.started.elapsed().as_secs_f64() * 1000.
            - item["sampleMonoMs"].as_f64().unwrap_or(0.))
        .max(0.));
        v
    }
    pub fn log(&mut self, level: &str, module: &str, message: &str) {
        self.sample(
            "logs",
            json!({"level":level,"module":module,"message":message,"eventTimeMs":epoch_ms()}),
            true,
            Instant::now(),
        );
        self.logs.push_back(self.latest["logs"].clone());
        if self.logs.len() > 2000 {
            self.logs.pop_front();
        }
    }
    pub fn simulate(&mut self) {
        if self.scenario == "imu_pause" {
            return;
        }
        let t = self.started.elapsed().as_secs_f64();
        let (r, p, y) = if self.scenario == "steady" {
            (0., 0., 0.)
        } else {
            (
                0.22 * (t * 0.8).sin(),
                0.15 * (t * 0.55).sin(),
                0.4 * (t * 0.3).sin(),
            )
        };
        let (sr, cr) = (r / 2.).sin_cos();
        let (sp, cp) = (p / 2.).sin_cos();
        let (sy, cy) = (y / 2.).sin_cos();
        let q = if self.scenario == "invalid" {
            vec![0.; 4]
        } else {
            vec![
                sr * cp * cy - cr * sp * sy,
                cr * sp * cy + sr * cp * sy,
                cr * cp * sy - sr * sp * cy,
                cr * cp * cy + sr * sp * sy,
            ]
        };
        self.sample(
            "pose",
            json!({"frame":"robot","quaternion":q,"calibrationId":"simulation-identity"}),
            self.scenario != "invalid",
            Instant::now(),
        );
        let (dr, dp, dy) = if self.scenario == "steady" {
            (0., 0., 0.)
        } else {
            (
                0.176 * (t * 0.8).cos(),
                0.0825 * (t * 0.55).cos(),
                0.12 * (t * 0.3).cos(),
            )
        };
        self.sample("imu.raw",json!({"frame":"simulated-sensor","gyro":[dr-dy*p.sin(),dp*r.cos()+dy*r.sin()*p.cos(),-dp*r.sin()+dy*r.cos()*p.cos()],"accel":[-9.81*p.sin(),9.81*r.sin()*p.cos(),9.81*r.cos()*p.cos()]}),true,Instant::now());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_contract() {
        let mut s = Telemetry::new(true);
        s.sample("joints", json!({"servos":[]}), true, Instant::now());
        let v = s.stamp(&s.latest["joints"]);
        assert_eq!(v["protocolVersion"], 1);
        assert_eq!(v["seq"], 1);
        assert!(v["ageMs"].as_f64().unwrap() >= 0.);
        assert_eq!(v["source"], "hardware");
    }
    #[test]
    fn limits_match() {
        assert_eq!(limit(34), Some((0., 30.)));
        assert_eq!(limit(20), Some(((-0.22f64).to_degrees(), (0.45f64).to_degrees())));
        assert_eq!(limit(30), Some((-135.,60.)));
        assert_eq!(limit(31), Some((-105.,105.)));
        assert_eq!(limit(1), None);
    }
}
