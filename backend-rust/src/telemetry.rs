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
        10 => (-30., 25.),
        20 => (-25., 30.),
        11 | 21 => (-22., 22.),
        12..=14 | 22..=24 | 31 => (-90., 90.),
        30 => (-90., 60.),
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
    pub drive: Option<DriveCommand>,
    pub boot: String,
    pub source: String,
    pub started: Instant,
    pub latest: BTreeMap<String, Value>,
    pub logs: VecDeque<Value>,
    seq: BTreeMap<String, u64>,
    pub control: Value,
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
            resources: Value::Null,
            boot: Uuid::new_v4().to_string(),
            source: if hardware { "hardware" } else { "simulation" }.into(),
            started: Instant::now(),
            latest: BTreeMap::new(),
            logs: VecDeque::new(),
            seq: BTreeMap::new(),
            control: json!({"state":"idle","message":"等待操作"}),
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
        assert_eq!(limit(20), Some((-25., 30.)));
        assert_eq!(limit(1), None);
    }
}
