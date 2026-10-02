use crate::telemetry::{epoch_ms, IDS};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
pub struct Calibration {
    pub path: PathBuf,
    pub state: Value,
}
fn keys(v: &Value, allowed: &[&str]) -> Result<()> {
    let o = v
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Invalid calibration section"))?;
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        bail!("Invalid calibration section")
    }
    Ok(())
}
fn vector(v: &Value, n: usize, low: f64, high: f64) -> Result<()> {
    let a = v
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid calibration vector"))?;
    if a.len() != n || a.iter().any(|v| v.as_f64().is_none()) {
        bail!("Invalid calibration vector")
    }
    let norm = a.iter().map(|v| v.as_f64().unwrap().powi(2)).sum::<f64>();
    if (low == 0.99 && (norm <= low || norm >= high))
        || (low != 0.99 && (norm < low || norm > high))
    {
        bail!("Invalid unit quaternion")
    }
    Ok(())
}
pub fn validate(p: &Value) -> Result<()> {
    keys(p, &["joints", "imu", "mounting"])?;
    if p.as_object().unwrap().is_empty() {
        bail!("Invalid calibration section")
    }
    if let Some(m) = p.get("mounting") {
        keys(m, &["yaw", "positionMm"])?;
        if !matches!(m["yaw"].as_f64(), Some(-90. | 0. | 90.)) {
            bail!("Invalid mounting yaw")
        }
        if let Some(pos) = m.get("positionMm") {
            let a = pos
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid mounting position"))?;
            if a.len() != 3
                || a.iter()
                    .any(|x| x.as_f64().map_or(true, |x| x.abs() > 300.))
            {
                bail!("Invalid mounting position")
            }
        }
    }
    if let Some(j) = p.get("joints") {
        keys(j, &["initialized", "references", "directions"])?;
        for key in ["references", "directions"] {
            let values = j[key]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Unknown servo ID"))?;
            for (id, v) in values {
                if !id
                    .parse::<u8>()
                    .ok()
                    .is_some_and(|i| IDS.contains(&i) && i.to_string() == *id)
                {
                    bail!("Unknown servo ID")
                }
                let x = v
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("Calibration must be finite"))?;
                if (key == "directions" && ![-1., 1.].contains(&x))
                    || (key == "references" && x.abs() > 2147483647.)
                {
                    bail!("Invalid joint calibration")
                }
            }
        }
    }
    if let Some(i) = p.get("imu") {
        keys(
            i,
            &[
                "initialized",
                "quaternion",
                "bootId",
                "time",
                "mountingQuaternion",
                "mountingSamples",
                "mountingSamplesMethod",
                "targetQuaternion",
            ],
        )?;
        if i["bootId"].as_str().map_or(true, |s| s.len() > 100)
            || i.get("time")
                .is_some_and(|v| v.as_str().map_or(true, |s| s.len() > 100))
        {
            bail!("Invalid IMU context")
        }
        for key in ["quaternion", "targetQuaternion", "mountingQuaternion"] {
            if let Some(q) = i.get(key).filter(|q| !q.is_null()) {
                let (lo, hi) = if key == "quaternion" {
                    (0.81, 1.21)
                } else {
                    (0.99, 1.01)
                };
                vector(q, 4, lo, hi)?;
            }
        }
        if let Some(method) = i.get("mountingSamplesMethod") {
            if method != "gravity-v1" {
                bail!("Invalid mounting sample method")
            }
        }
        if let Some(samples) = i.get("mountingSamples") {
            keys(samples, &["roll", "pitch"])?;
            for v in samples.as_object().unwrap().values() {
                vector(v, 3, 0.99, 1.01)?;
            }
        }
    }
    Ok(())
}
impl Calibration {
    pub fn open(path: PathBuf) -> Result<Self> {
        let state = if path.exists() {
            let v: Value = serde_json::from_slice(&fs::read(&path)?)?;
            let p = json!({"joints":v["joints"],"imu":v["imu"]});
            validate(&p)?;
            if let Some(m) = v.get("mounting") {
                validate(&json!({"mounting":m}))?;
            }
            v
        } else {
            json!({"schema":1,"deviceId":uuid::Uuid::new_v4().to_string(),"revision":0,"updatedAt":0,"joints":{"initialized":false,"references":{},"directions":{}},"imu":{"initialized":false,"quaternion":null,"bootId":"","time":""}})
        };
        let s = Self { path, state };
        if !s.path.exists() {
            s.persist(&s.state)?
        }
        Ok(s)
    }
    pub fn read(&self, boot: &str) -> Value {
        let mut v = self.state.clone();
        if v.get("mounting").is_none() {
            v["mounting"] = json!({"yaw":-90});
        }
        v["imu"]["validForBoot"] = json!(v["imu"]["bootId"] == boot);
        v["bootId"] = json!(boot);
        v
    }
    pub fn update(
        &mut self,
        revision: &Value,
        patch: &Value,
        boot: &str,
        migrate: bool,
    ) -> Result<Value> {
        validate(patch)?;
        if revision.as_u64() != self.state["revision"].as_u64() {
            bail!("标定已被其他页面更新，请重试")
        }
        for k in patch.as_object().unwrap().keys() {
            if migrate && self.state[k]["initialized"] == true {
                bail!("主板已有标定，未覆盖旧数据")
            }
        }
        if patch
            .get("imu")
            .is_some_and(|i| !i["quaternion"].is_null() && i["bootId"] != boot)
        {
            bail!("IMU 会话已改变，请重新归零")
        }
        let mut candidate = self.state.clone();
        for (k, v) in patch.as_object().unwrap() {
            candidate[k] = v.clone();
            if k != "mounting" {
                candidate[k]["initialized"] = json!(true);
            }
        }
        candidate["revision"] = json!(self.state["revision"].as_u64().unwrap() + 1);
        candidate["updatedAt"] = json!(epoch_ms());
        self.persist(&candidate)?;
        self.state = candidate;
        Ok(self.read(boot))
    }
    fn persist(&self, v: &Value) -> Result<()> {
        let parent = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let tmp = self.path.with_extension("tmp");
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(&serde_json::to_vec(v)?)?;
        f.sync_all()?;
        if self.path.exists() {
            fs::copy(&self.path, self.path.with_extension("previous.json"))?;
        }
        fs::rename(tmp, &self.path)?;
        #[cfg(unix)]
        {
            OpenOptions::new().read(true).open(parent)?.sync_all()?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_conflict() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("calibration.json");
        let mut c = Calibration::open(p.clone()).unwrap();
        c.update(
            &json!(0),
            &json!({"joints":{"references":{"34":2048},"directions":{"34":-1}}}),
            "boot",
            false,
        )
        .unwrap();
        assert!(c
            .update(&json!(0), &json!({"mounting":{"yaw":0}}), "boot", false)
            .is_err());
        assert_eq!(
            Calibration::open(p).unwrap().state["joints"]["references"]["34"],
            2048
        );
    }
    #[test]
    fn bad_vectors() {
        assert!(validate(&json!({"imu":{"bootId":"b","quaternion":[0,0,0,0]}})).is_err());
        assert!(validate(&json!({"mounting":{"yaw":45}})).is_err());
    }
}
