use crate::{
    calibration::Calibration,
    control::{Owner, Transport},
    orientation,
    telemetry::{epoch_ms, Shared},
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub struct Plan {
    pub value: Value,
    pub created: Instant,
}
pub type Plans = Arc<Mutex<Option<Plan>>>;
pub async fn capture(
    shared: &Shared,
    ids: &[u8],
    imu: bool,
) -> Result<(Vec<Value>, Option<Value>)> {
    let mut values: BTreeMap<u8, Vec<Value>> = ids.iter().map(|id| (*id, vec![])).collect();
    let mut quats: Vec<[f64; 4]> = vec![];
    let mut seen = BTreeMap::new();
    let boot = shared.read().unwrap().boot.clone();
    let until = Instant::now() + Duration::from_millis(1300);
    while Instant::now() < until {
        {
            let s = shared.read().unwrap();
            for topic in [
                if ids.is_empty() { None } else { Some("joints") },
                if imu { Some("imu.raw") } else { None },
                if imu { Some("imu.orientation") } else { None },
            ]
            .into_iter()
            .flatten()
            {
                let sample = s
                    .latest
                    .get(topic)
                    .ok_or_else(|| anyhow::anyhow!("{topic} 无新鲜实机数据"))?;
                if sample["source"] != "hardware"
                    || sample["valid"] != true
                    || s.stamp(sample)["ageMs"].as_f64().unwrap_or(f64::INFINITY) > 350.
                {
                    bail!("{topic} 无新鲜实机数据")
                }
                if seen.get(topic) == Some(&sample["seq"]) {
                    continue;
                }
                seen.insert(topic.to_owned(), sample["seq"].clone());
                if topic == "joints" {
                    let rows = sample["data"]["servos"]
                        .as_array()
                        .ok_or_else(|| anyhow::anyhow!("Invalid joints"))?;
                    for id in ids {
                        let r = rows
                            .iter()
                            .find(|r| r["id"] == *id)
                            .ok_or_else(|| anyhow::anyhow!("#{id} 离线"))?;
                        if r["online"] != true
                            || r["fault"].as_u64() != Some(0)
                            || r["ageMs"].as_f64().unwrap_or(999.) > 350.
                        {
                            bail!("#{id} 离线、过期或故障；可取消该关节后分批标定")
                        };
                        values.get_mut(id).unwrap().push(r.clone());
                    }
                } else if topic == "imu.raw" {
                    let a = orientation::vector::<3>(&sample["data"]["accel"])?;
                    let g = orientation::vector::<3>(&sample["data"]["gyro"])?;
                    let an = a.iter().map(|x| x * x).sum::<f64>().sqrt();
                    let gn = g.iter().map(|x| x * x).sum::<f64>().sqrt();
                    if !(8.3 < an && an < 11.3) || gn > 0.08 {
                        bail!("IMU 未静止或重力读数异常，不能用标定掩盖传感器异常")
                    }
                } else {
                    let mut q = orientation::vector::<4>(&sample["data"]["quaternion"])?;
                    let norm = q.iter().map(|x| x * x).sum::<f64>().sqrt();
                    if !(0.95 < norm && norm < 1.05) {
                        bail!("IMU 四元数无效")
                    };
                    for x in &mut q {
                        *x /= norm;
                    }
                    quats.push(q);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    if boot != shared.read().unwrap().boot {
        bail!("采样期间后端会话改变")
    }
    let mut result = vec![];
    for id in ids {
        let samples = &values[id];
        if samples.len() < 5 {
            bail!("#{id} 有效样本不足")
        };
        let positions = samples
            .iter()
            .map(|r| {
                r["position"]
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("Invalid position"))
            })
            .collect::<Result<Vec<_>>>()?;
        let low = positions.iter().copied().reduce(f64::min).unwrap();
        let high = positions.iter().copied().reduce(f64::max).unwrap();
        if high - low > 8. {
            bail!("#{id} 未保持静止，请支撑关节后重试")
        };
        result.push(json!({"id":id,"position":positions.iter().sum::<f64>()/positions.len() as f64,"torque":samples.last().unwrap()["torque"]}));
    }
    let q = if imu {
        if quats.len() < 15 {
            bail!("IMU 有效样本不足")
        }
        let first = quats[0];
        let mut mean = [0.; 4];
        for q in &quats {
            let dot = first.iter().zip(q).map(|(a, b)| a * b).sum::<f64>();
            if dot.abs() < 1f64.to_radians().cos() {
                bail!("IMU 朝向不稳定")
            };
            for n in 0..4 {
                mean[n] += q[n] * if dot >= 0. { 1. } else { -1. };
            }
        }
        let norm = mean.iter().map(|x| x * x).sum::<f64>().sqrt();
        Some(json!(mean.map(|x| x / norm)))
    } else {
        None
    };
    Ok((result, q))
}
pub async fn preview(
    body: &Value,
    shared: &Shared,
    cal: &Arc<Mutex<Calibration>>,
    plans: &Plans,
) -> Result<Value> {
    let poses: Value = serde_json::from_str(include_str!("poses.json"))?;
    let pose = body["pose"]
        .as_str()
        .filter(|p| poses.get(*p).is_some())
        .ok_or_else(|| anyhow::anyhow!("无效姿势或关节选择"))?;
    let ids = body["ids"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("无效关节选择"))?
        .iter()
        .map(|v| {
            v.as_u64()
                .filter(|i| *i < 256)
                .map(|v| v as u8)
                .ok_or_else(|| anyhow::anyhow!("无效关节选择"))
        })
        .collect::<Result<Vec<_>>>()?;
    let modes = body["modes"]
        .as_array()
        .filter(|m| {
            !m.is_empty()
                && m.iter()
                    .all(|v| matches!(v.as_str(), Some("position" | "hardware" | "imu")))
        })
        .ok_or_else(|| anyhow::anyhow!("请选择标定项目"))?;
    if ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
        || ids
            .iter()
            .any(|id| !poses[pose]["ids"].as_array().unwrap().contains(&json!(id)))
    {
        bail!("无效姿势或关节选择")
    }
    let joints = modes.contains(&json!("position")) || modes.contains(&json!("hardware"));
    let imu = modes.contains(&json!("imu"));
    if joints && ids.is_empty() {
        bail!("请选择至少一个关节")
    }
    let boot = shared.read().unwrap().boot.clone();
    let state = cal.lock().unwrap().read(&boot);
    let (mut rows, q) = capture(shared, if joints { &ids } else { &[] }, imu).await?;
    if let Some(q) = &q {
        orientation::imu_patch(&state, pose, q, &boot)?;
    }
    if modes.contains(&json!("hardware")) && rows.iter().any(|r| r["torque"] != 0) {
        bail!("硬件中位要求所有所选舵机扭矩关闭；本页面不会自动开关扭矩")
    }
    for r in &mut rows {
        let key = r["id"].as_u64().unwrap().to_string();
        let d = state["joints"]["directions"]
            .get(&key)
            .and_then(Value::as_f64)
            .unwrap_or(-1.);
        let angle = poses[pose]["angles"][&key].as_f64().unwrap();
        r["direction"] = json!(d);
        r["angle"] = json!(angle);
        r["reference"] =
            json!(r["position"].as_f64().unwrap() - angle * 4096. / std::f64::consts::TAU * d);
        r["target"] =
            json!((2048. + angle * 4096. / std::f64::consts::TAU * d).round_ties_even() as i32);
    }
    let v = json!({"token":uuid::Uuid::new_v4().simple().to_string(),"bootId":boot,"revision":state["revision"],"pose":pose,"modes":modes,"rows":rows,"quaternion":q,"created":shared.read().unwrap().started.elapsed().as_secs_f64(),"previousCalibration":state});
    *plans.lock().unwrap() = Some(Plan {
        value: v.clone(),
        created: Instant::now(),
    });
    Ok(v)
}
pub async fn execute(
    token: &Value,
    shared: &Shared,
    cal: &Arc<Mutex<Calibration>>,
    plans: &Plans,
    owner: Option<&Owner>,
) -> Result<Value> {
    let plan = plans
        .lock()
        .unwrap()
        .take()
        .filter(|p| p.value["token"] == *token && p.created.elapsed() < Duration::from_secs(60))
        .ok_or_else(|| anyhow::anyhow!("预览已过期，请重新采样"))?;
    let p = plan.value;
    let boot = shared.read().unwrap().boot.clone();
    if p["bootId"] != boot {
        bail!("预览已过期，请重新采样")
    }
    let (state, path) = {
        let c = cal.lock().unwrap();
        (c.read(&boot), c.path.clone())
    };
    if state["revision"] != p["revision"] {
        bail!("标定已被其他页面修改，请重新采样")
    }
    let ids = p["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    let modes = p["modes"].as_array().unwrap();
    let (rows, q) = capture(shared, &ids, modes.contains(&json!("imu"))).await?;
    if rows
        .iter()
        .zip(p["rows"].as_array().unwrap())
        .any(|(a, b)| {
            (a["position"].as_f64().unwrap() - b["position"].as_f64().unwrap()).abs() > 8.
        })
    {
        bail!("姿势已改变，请重新采样")
    }
    if let Some(q) = &q {
        let a = orientation::vector::<4>(q)?;
        let b = orientation::vector::<4>(&p["quaternion"])?;
        if a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>().abs() < 1f64.to_radians().cos() {
            bail!("IMU 姿势已改变")
        }
    }
    let mut joints = state["joints"].clone();
    let mut results = json!([]);
    if modes.contains(&json!("hardware")) {
        let owner = owner.ok_or_else(|| anyhow::anyhow!("舵机串口未启用"))?;
        let backup = path
            .parent()
            .unwrap_or(Path::new("."))
            .join("calibration-backups")
            .join(format!("{}.json", p["token"].as_str().unwrap()));
        let rx = owner.submit(
            json!({"action":"calibrate","plan":p,"backupPath":backup}),
            None,
        )?;
        results = rx.await??;
        for r in results.as_array().unwrap() {
            let key = r["id"].as_u64().unwrap().to_string();
            if r["ok"] == true {
                joints["references"][&key] = json!(2048)
            } else if r["attempted"] == true {
                joints["references"].as_object_mut().unwrap().remove(&key);
            }
        }
    } else if modes.contains(&json!("position")) {
        for r in p["rows"].as_array().unwrap() {
            joints["references"][r["id"].as_u64().unwrap().to_string()] = r["reference"].clone();
        }
    }
    let ok = results.as_array().unwrap().iter().all(|r| r["ok"] == true);
    let mut patch = json!({});
    if !ids.is_empty() {
        patch["joints"] = joints;
    }
    if modes.contains(&json!("imu")) && ok {
        patch["imu"] = orientation::imu_patch(
            &state,
            p["pose"].as_str().unwrap(),
            q.as_ref().unwrap(),
            &boot,
        )?;
    }
    let saved = if patch.as_object().unwrap().is_empty() {
        state.clone()
    } else {
        cal.lock()
            .unwrap()
            .update(&state["revision"], &patch, &boot, false)?
    };
    Ok(
        json!({"ok":ok,"calibration":saved,"results":results,"backup":if modes.contains(&json!("hardware")){p["token"].clone()}else{Value::Null}}),
    )
}
pub fn hardware<T: Transport>(
    bus: &mut T,
    p: &Value,
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Value> {
    let rows = p["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid calibration plan"))?;
    let mut before = json!({});
    let check = |id: u8, f: &Value, r: &Value| -> Result<()> {
        if f["torque"] != 0
            || f["fault"].as_u64() != Some(0)
            || (f["position"].as_f64().unwrap_or(f64::INFINITY) - r["position"].as_f64().unwrap())
                .abs()
                > 8.
        {
            bail!("#{id} 扭矩、位置或故障状态已改变，请重新采样")
        };
        Ok(())
    };
    for r in rows {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            bail!("硬件校准已取消，未解锁写入")
        };
        let id = r["id"].as_u64().unwrap() as u8;
        let f = bus.feedback(&[id])?;
        let f = f
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("#{id} 反馈丢失"))?;
        check(id, f, r)?;
        let version = bus.read(id, 0, 6)?;
        if version[..2] != [3, 46] {
            bail!("#{id} 固件不是已核对的3.46，停止硬件校准")
        };
        before[id.to_string()] = json!({"feedback":f,"offset":bus.read(id,31,2)?,"lock":bus.read(id,55,1)?,"version":version});
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec(
        &json!({"plan":p,"before":before,"eventTimeMs":epoch_ms()}),
    )?)?;
    file.sync_all()?;
    let mut results = vec![];
    for r in rows {
        let id = r["id"].as_u64().unwrap() as u8;
        let mut result = json!({"id":id,"ok":false,"attempted":false});
        let write = (|| -> Result<()> {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                bail!("硬件校准已取消，停止后续写入")
            };
            let f = bus.feedback(&[id])?;
            let f = f
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("#{id} 反馈丢失"))?;
            check(id, f, r)?;
            result["attempted"] = json!(true);
            bus.write(id, 55, &[0])?;
            let target = r["target"]
                .as_u64()
                .filter(|v| *v <= 65535)
                .ok_or_else(|| anyhow::anyhow!("Invalid midpoint"))?
                as u16;
            // An uncertain EEPROM instruction is NEVER retried automatically.
            // Transport must implement 0x0B directly rather than a register write.
            bus.calibrate(id, target)?;
            std::thread::sleep(Duration::from_millis(80));
            let after = bus.feedback(&[id])?;
            let a = after
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("#{id} 写后反馈丢失"))?;
            let offset = bus.read(id, 31, 2)?;
            if (a["position"].as_f64().unwrap_or(f64::INFINITY) - target as f64).abs() > 3.
                || a["torque"] != 0
                || a["fault"].as_u64() != Some(0)
            {
                bail!("写后位置或状态校验失败；保留备份，不自动重试")
            }
            if (f["position"].as_f64().unwrap() - target as f64).abs() > 3.
                && json!(offset) == before[id.to_string()]["offset"]
            {
                bail!("偏移未改变，固件可能不支持0x0B参数校准")
            };
            result["position"] = a["position"].clone();
            result["offset"] = json!(offset);
            Ok(())
        })();
        if let Err(e) = write {
            result["error"] = json!(e.to_string());
        } else {
            result["ok"] = json!(true);
        }
        if result["attempted"] == true {
            let finish = (|| -> Result<()> {
                let f = bus.feedback(&[id])?;
                let position =
                    f.get(&id)
                        .and_then(|r| r["position"].as_i64())
                        .ok_or_else(|| anyhow::anyhow!("收尾位置丢失"))? as i32;
                bus.write(id, 42, &crate::control::goal_payload(position)?)?;
                Ok(())
            })();
            // Even if alignment fails, independently attempt to lock EEPROM.
            let lock = bus.write(id, 55, &[1]).and_then(|_| {
                if bus.read(id, 55, 1)? != [1] {
                    bail!("EEPROM锁定未确认")
                };
                Ok(())
            });
            if let Err(e) = finish.and(lock) {
                result["ok"] = json!(false);
                result["error"] = json!(format!("收尾校验失败：{e}"));
            }
        }
        let success = result["ok"] == true;
        results.push(result);
        if !success {
            break;
        }
    }
    let result = json!(results);
    std::fs::write(
        path.with_extension("result.json"),
        serde_json::to_vec(&result)?,
    )?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    struct EepromMock {
        calls: Vec<(u8, Vec<u8>)>,
        fail_opcode: bool,
        position: i32,
        offset: u16,
    }
    impl Transport for EepromMock {
        fn calibrate(&mut self, _: u8, target: u16) -> Result<()> {
            self.calls.push((11, target.to_le_bytes().to_vec()));
            if self.fail_opcode {
                bail!("ACK lost")
            };
            self.position = target as i32;
            self.offset = 148;
            Ok(())
        }
        fn feedback(&mut self, ids: &[u8]) -> Result<crate::control::Feedback> {
            Ok(ids
                .iter()
                .map(|id| (*id, json!({"position":self.position,"torque":0,"fault":0})))
                .collect())
        }
        fn read(&mut self, _: u8, address: u8, size: u8) -> Result<Vec<u8>> {
            Ok(match (address, size) {
                (0, 6) => vec![3, 46, 0, 0, 0, 0],
                (31, 2) => self.offset.to_le_bytes().to_vec(),
                (55, 1) => vec![1],
                _ => vec![0; size as usize],
            })
        }
        fn write(&mut self, _: u8, address: u8, data: &[u8]) -> Result<()> {
            self.calls.push((address, data.to_vec()));
            Ok(())
        }
        fn sync(&mut self, _: u8, _: &BTreeMap<u8, Vec<u8>>) -> Result<()> {
            bail!("Unexpected sync write")
        }
    }
    #[test]
    fn uncertain_eeprom_write_not_retried_and_locked() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("backup.json");
        let mut bus = EepromMock {
            calls: vec![],
            fail_opcode: true,
            position: 1900,
            offset: 0,
        };
        let result = hardware(
            &mut bus,
            &json!({"rows":[{"id":34,"position":1900,"target":2048}]}),
            &p,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(p.exists());
        assert_eq!(result[0]["attempted"], true);
        assert_eq!(result[0]["ok"], false);
        assert_eq!(
            bus.calls.iter().map(|(a, _)| *a).collect::<Vec<_>>(),
            [55, 11, 42, 55]
        );
        assert_eq!(bus.calls.last().unwrap().1, [1]);
        assert!(p.with_extension("result.json").exists());
    }
    #[test]
    fn eeprom_success_verified_before_lock() {
        let d = tempfile::tempdir().unwrap();
        let mut bus = EepromMock {
            calls: vec![],
            fail_opcode: false,
            position: 1900,
            offset: 0,
        };
        let result = hardware(
            &mut bus,
            &json!({"rows":[{"id":34,"position":1900,"target":2048}]}),
            &d.path().join("backup.json"),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result[0]["ok"], true);
        assert_eq!(result[0]["position"], 2048);
        assert_eq!(bus.calls.iter().filter(|(a, _)| *a == 11).count(), 1);
        assert!(bus.calls.iter().all(|(a, _)| *a != 40 && *a != 44));
    }
    #[tokio::test]
    async fn supine_reference_preserves_mount_and_persists() {
        let shared = Arc::new(std::sync::RwLock::new(crate::telemetry::Telemetry::new(
            true,
        )));
        let data = shared.clone();
        let producer = tokio::spawn(async move {
            loop {
                {
                    let mut s = data.write().unwrap();
                    s.sample("joints",json!({"servos":[{"id":34,"position":1900,"torque":0,"online":true,"fault":0,"ageMs":0}]}),true,Instant::now());
                    s.sample(
                        "imu.raw",
                        json!({"accel":[0,0,9.80665],"gyro":[0,0,0]}),
                        true,
                        Instant::now(),
                    );
                    s.sample(
                        "imu.orientation",
                        json!({"quaternion":[0,0,0,1]}),
                        true,
                        Instant::now(),
                    );
                    drop(s);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        tokio::time::sleep(Duration::from_millis(25)).await;
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("calibration.json");
        let mut store = Calibration::open(path.clone()).unwrap();
        let boot = shared.read().unwrap().boot.clone();
        store.update(&json!(0),&json!({"imu":{"quaternion":[0,0,0,1],"bootId":boot,"mountingQuaternion":[0,0,0,1]}}),&boot,false).unwrap();
        let cal = Arc::new(Mutex::new(store));
        let plans = Arc::new(Mutex::new(None));
        let plan = preview(
            &json!({"pose":"supine","ids":[34],"modes":["position","imu"]}),
            &shared,
            &cal,
            &plans,
        )
        .await
        .unwrap();
        assert_eq!(plan["rows"][0]["reference"], 1900.);
        let result = execute(&plan["token"], &shared, &cal, &plans, None)
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(
            result["calibration"]["imu"]["mountingQuaternion"],
            json!([0, 0, 0, 1])
        );
        assert_eq!(result["calibration"]["revision"], 2);
        assert_eq!(
            Calibration::open(path).unwrap().state["joints"]["references"]["34"],
            1900.
        );
        producer.abort();
    }
}
