//! Native serial-owner control. No Python process or HTTP forwarding.
use crate::{
    servos::Bus,
    telemetry::{limit, Shared, IDS},
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, OwnedSemaphorePermit};

pub type Feedback = BTreeMap<u8, Value>;
pub fn rounded(v: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (v * scale).round_ties_even() / scale
}
#[derive(Default)]
pub struct Timing {
    count: u64,
    first: Option<Instant>,
    last: Option<Instant>,
    max_gap: f64,
}
impl Timing {
    pub fn record(&mut self, tick: Instant) {
        self.count += 1;
        if self.first.is_none() {
            self.first = Some(tick)
        };
        if let Some(last) = self.last {
            self.max_gap = self.max_gap.max((tick - last).as_secs_f64())
        };
        self.last = Some(tick);
    }
    pub fn stats(&self) -> Value {
        json!({"commandTargetHz":50,"commandCount":self.count,"commandHz":if self.count>1{json!(rounded((self.count-1) as f64/(self.last.unwrap()-self.first.unwrap()).as_secs_f64(),2))}else{Value::Null},"commandMaxGapMs":rounded(self.max_gap*1000.,2)})
    }
}
pub trait Transport {
    fn calibrate(&mut self, id: u8, target: u16) -> Result<()>;
    fn feedback(&mut self, ids: &[u8]) -> Result<Feedback>;
    fn read(&mut self, id: u8, address: u8, size: u8) -> Result<Vec<u8>>;
    fn write(&mut self, id: u8, address: u8, data: &[u8]) -> Result<()>;
    fn sync(&mut self, address: u8, values: &BTreeMap<u8, Vec<u8>>) -> Result<()>;
}
impl Transport for Bus {
    fn calibrate(&mut self, id: u8, target: u16) -> Result<()> {
        self.exchange(id, 11, &target.to_le_bytes(), 0)?;
        Ok(())
    }
    fn feedback(&mut self, ids: &[u8]) -> Result<Feedback> {
        self.read_feedback(ids)
    }
    fn read(&mut self, id: u8, address: u8, size: u8) -> Result<Vec<u8>> {
        self.read_register(id, address, size)
    }
    fn write(&mut self, id: u8, address: u8, data: &[u8]) -> Result<()> {
        self.write_register(id, address, data)
    }
    fn sync(&mut self, address: u8, values: &BTreeMap<u8, Vec<u8>>) -> Result<()> {
        self.sync_write(address, values)
    }
}
pub fn encode(value: i32) -> Result<Vec<u8>> {
    if value.unsigned_abs() > 32767 {
        bail!("目标超出编码范围")
    }
    Ok(
        ((value.unsigned_abs() as u16) | if value < 0 { 0x8000 } else { 0 })
            .to_le_bytes()
            .to_vec(),
    )
}
pub fn validate(id: u8, f: Option<&Value>, enabled: bool) -> Result<()> {
    let f = f.ok_or_else(|| anyhow::anyhow!("#{id} 反馈丢失"))?;
    if f["fault"].as_u64() != Some(0) {
        bail!("#{id} 舵机故障码 {}", f["fault"])
    }
    if !f["voltage"]
        .as_f64()
        .is_some_and(|v| (4.0..=8.4).contains(&v))
    {
        bail!("#{id} 电压 {}V 超出舵机工作范围 4.0–8.4V", f["voltage"])
    }
    if enabled && f["torque"] != 1 {
        bail!("#{id} 未使能")
    }
    Ok(())
}
pub fn angle_request(id: &Value, degree: &Value) -> Result<(u8, f64)> {
    let id = id
        .as_u64()
        .filter(|i| *i < 256)
        .ok_or_else(|| anyhow::anyhow!("该舵机没有已核实的官方角度范围"))? as u8;
    let (low, high) = limit(id).ok_or_else(|| anyhow::anyhow!("该舵机没有已核实的官方角度范围"))?;
    let degree = degree
        .as_f64()
        .filter(|v| v.is_finite() && *v >= low && *v <= high)
        .ok_or_else(|| anyhow::anyhow!("#{id} 角度须在配置范围 {low}°–{high}°"))?;
    Ok((id, degree))
}
pub fn target(
    id: u8,
    start: i32,
    cal: &Value,
    low: i32,
    high: i32,
    angle: f64,
    nearest: bool,
) -> Result<i32> {
    let key = id.to_string();
    let reference = cal["joints"]["references"][&key]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("#{id} 请先完成位置标定"))?;
    let direction = cal["joints"]["directions"]
        .get(&key)
        .and_then(Value::as_f64)
        .unwrap_or(-1.);
    let (a, b) = limit(id).ok_or_else(|| anyhow::anyhow!("Unknown ID"))?;
    if !angle.is_finite()
        || angle < a.to_radians()
        || angle > b.to_radians()
        || ![-1., 1.].contains(&direction)
    {
        bail!("#{id} 目标超出关节角度范围 {a}°–{b}°")
    }
    let mut goal =
        (reference + angle * 4096. / std::f64::consts::TAU * direction).round_ties_even();
    if nearest {
        goal += ((start as f64 - goal) / 4096.).round_ties_even() * 4096.;
    }
    if goal.abs() > 32767. || (high > low && (goal < low as f64 || goal > high as f64)) {
        bail!("#{id} 目标超出编码范围或舵机硬件限位")
    }
    Ok(goal as i32)
}
pub fn configuration<T: Transport>(bus: &mut T, id: u8) -> Result<(i32, i32, u16)> {
    let c = bus.read(id, 0, 40)?;
    if c.len() != 40 || c[..2] != [3, 46] || c[33] != 4 {
        bail!("#{id} 固件或运行模式未经验证")
    }
    let word = |n| u16::from_le_bytes([c[n], c[n + 1]]);
    Ok((word(9) as i32, word(11) as i32, word(16)))
}
fn position(f: &Value) -> Result<i32> {
    Ok(f["position"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("Invalid encoder"))? as i32)
}
pub fn goals<T: Transport>(bus: &mut T, values: &BTreeMap<u8, i32>) -> Result<()> {
    let bytes = values
        .iter()
        .map(|(id, v)| Ok((*id, encode(*v)?)))
        .collect::<Result<_>>()?;
    bus.sync(42, &bytes)
}
pub fn unload<T: Transport>(bus: &mut T) -> Result<Value> {
    bus.sync(40, &IDS.iter().map(|id| (*id, vec![0])).collect())?;
    let f = bus.feedback(&IDS)?;
    let missing: Vec<_> = IDS
        .iter()
        .filter(|id| f.get(id).is_none_or(|r| r["torque"] != 0))
        .copied()
        .collect();
    Ok(
        json!({"state":if missing.is_empty(){"disabled"}else{"failed"},"message":if missing.is_empty(){"15 个舵机已失能（卸力）".to_string()}else{format!("已发送全部失能；未确认 ID {missing:?}")},"unconfirmedIds":missing}),
    )
}
pub fn publish(shared: &Shared, f: &Feedback, ids: &[u8], diagnostics: Value) {
    let now = crate::telemetry::monotonic();
    let rows: Vec<_> = ids
        .iter()
        .map(|id| {
            let mut r = f.get(id).cloned().unwrap_or(json!({"ageMs":0}));
            if let Some(received) = r
                .as_object_mut()
                .unwrap()
                .remove("_receivedMono")
                .and_then(|v| v.as_f64())
            {
                r["ageMs"] = json!((now - received).max(0.) * 1000.);
            }
            r["id"] = json!(id);
            r["online"] = json!(f.contains_key(id));
            r
        })
        .collect();
    let mut s = shared.write().unwrap();
    s.sample("joints",json!({"configuredIds":ids,"servos":rows,"error":"","scanMs":diagnostics["elapsedMs"],"targetHz":50,"readMode":"sync-read","diagnostics":diagnostics}),true,Instant::now());
    if let Some(v) = s.latest.get_mut("joints") {
        v["source"] = json!("hardware");
    }
}
pub fn status(shared: &Shared, v: Value) {
    shared.write().unwrap().control = v;
}
fn profile<T: Transport>(bus: &mut T, id: u8, f: &Value) -> Result<()> {
    if f["accelerationRaw"] != 0 {
        bus.sync(41, &BTreeMap::from([(id, vec![0])]))?;
    }
    if f["speedLimitRaw"] != 500 {
        bus.sync(46, &BTreeMap::from([(id, 500u16.to_le_bytes().to_vec())]))?;
    }
    Ok(())
}
pub fn execute<T: Transport>(
    bus: &mut T,
    command: &Value,
    cancel: &AtomicBool,
    shared: &Shared,
) -> Result<Value> {
    let action = command["action"].as_str().unwrap_or("");
    if action == "disable" {
        return unload(bus);
    }
    let began = Instant::now();
    let budget = if action == "stand" { 3. } else { 5. };
    let guard = || -> Result<()> {
        if cancel.load(Ordering::Acquire) {
            bail!("已取消动作，执行全部失能")
        };
        if began.elapsed().as_secs_f64() >= budget {
            bail!("{budget} 秒内未确认到位，停止并失能")
        };
        Ok(())
    };
    let mut attempted = false;
    let mut timing = Timing::default();
    let result = (|| -> Result<Value> {
        guard()?;
        status(
            shared,
            json!({"state":"preflight","action":action,"message":"检查实时反馈、目标及硬件限位","progress":0}),
        );
        if action == "angle" || action == "profile" {
            let degree_value = if action == "angle" {
                command["angleDeg"].clone()
            } else {
                json!(0)
            };
            let (id, degree) = angle_request(&command["id"], &degree_value)?;
            let f = bus.feedback(&[id])?;
            validate(id, f.get(&id), false)?;
            let (low, high, _) = configuration(bus, id)?;
            guard()?;
            let goal = if action == "angle" {
                Some(target(
                    id,
                    position(&f[&id])?,
                    &command["calibration"],
                    low,
                    high,
                    degree.to_radians(),
                    false,
                )?)
            } else {
                None
            };
            if let Some(g) = goal {
                attempted = true;
                goals(bus, &BTreeMap::from([(id, g)]))?;
            }
            guard()?;
            profile(bus, id, &f[&id])?;
            guard()?;
            if goal.is_some() {
                bus.sync(40, &BTreeMap::from([(id, vec![1])]))?;
            }
            guard()?;
            let after = bus.feedback(&IDS)?;
            publish(shared, &after, &IDS, Value::Null);
            validate(id, after.get(&id), goal.is_some())?;
            return if let Some(goal) = goal {
                Ok(
                    json!({"state":"commanded","action":action,"id":id,"angleDeg":degree,"target":goal,"message":format!("#{id} 已发送 {degree}°，已使能；到位情况看实时角度")}),
                )
            } else {
                if after[&id]["accelerationRaw"] != 0 || after[&id]["speedLimitRaw"] != 500 {
                    bail!("#{id} 参数回读不一致")
                }
                Ok(
                    json!({"state":"configured","action":action,"id":id,"accelerationRaw":0,"speedLimitRaw":500,"message":format!("#{id} 加速度0、速度500已回读确认，未写目标角度或使能")}),
                )
            };
        }
        if !["enable", "stand"].contains(&action) {
            bail!("未知舵机操作")
        }
        let ids = if action == "stand" {
            &IDS[..14]
        } else {
            &IDS[..]
        };
        let initial = bus.feedback(&IDS)?;
        let mut cfg = BTreeMap::new();
        for id in ids {
            guard()?;
            validate(*id, initial.get(id), false)?;
            let c = configuration(bus, *id)?;
            if c.2 == 0 || c.2 > 1000 {
                bail!("#{id} 输出限制超出硬件范围")
            };
            cfg.insert(*id, c);
        }
        let fresh = bus.feedback(&IDS)?;
        let mut starts = BTreeMap::new();
        let mut targets = BTreeMap::new();
        const STAND: [f64; 14] = [
            0.,
            0.0872664626,
            0.457924,
            0.004940,
            -0.452984,
            0.,
            -0.0872664626,
            -0.457924,
            -0.004940,
            0.452984,
            0.,
            0.,
            0.,
            0.,
        ];
        for (n, id) in ids.iter().enumerate() {
            validate(*id, fresh.get(id), false)?;
            let start = position(&fresh[id])?;
            encode(start)?;
            let (low, high, _) = cfg[id];
            if high > low && (start < low || start > high) {
                bail!("#{id} 当前位置超出舵机硬件限位")
            };
            starts.insert(*id, start);
            let a = command["standTargets"]
                .get(id.to_string())
                .and_then(Value::as_f64)
                .unwrap_or(STAND[n.min(13)]);
            targets.insert(
                *id,
                if action == "stand" {
                    target(*id, start, &command["calibration"], low, high, a, true)?
                } else {
                    start
                },
            );
        }
        guard()?;
        if action == "stand" && budget - began.elapsed().as_secs_f64() < 2.5 {
            bail!("总线预检查过慢，未执行运动")
        }
        attempted = true;
        goals(bus, &starts)?;
        bus.sync(41, &ids.iter().map(|id| (*id, vec![0])).collect())?;
        bus.sync(
            46,
            &ids.iter()
                .map(|id| (*id, 500u16.to_le_bytes().to_vec()))
                .collect(),
        )?;
        for id in ids {
            guard()?;
            let cap = cfg[id].2.to_le_bytes();
            bus.write(*id, 48, &cap)?;
            if bus.read(*id, 48, 2)? != cap {
                bail!("#{id} 最大输出设置未确认")
            }
        }
        guard()?;
        bus.sync(40, &ids.iter().map(|id| (*id, vec![1])).collect())?;
        let f = bus.feedback(&IDS)?;
        publish(shared, &f, &IDS, Value::Null);
        for id in ids {
            validate(*id, f.get(id), true)?;
        }
        if action == "enable" {
            return Ok(
                json!({"state":"enabled","action":action,"message":"15 个舵机已使能，保持当前位置","progress":1}),
            );
        }
        let trajectory = Instant::now();
        if budget - began.elapsed().as_secs_f64() < 2.35 {
            bail!("总线预处理过慢，无法在 3 秒内完成")
        }
        let mut next = trajectory;
        let mut settled = None;

        loop {
            guard()?;
            let ratio = (trajectory.elapsed().as_secs_f64() / 2.2).min(1.);
            let blend = ratio * ratio * (3. - 2. * ratio);
            let commanded = starts
                .iter()
                .map(|(id, s)| {
                    (
                        *id,
                        (*s as f64 + (targets[id] - s) as f64 * blend).round_ties_even() as i32,
                    )
                })
                .collect();
            goals(bus, &commanded)?;
            timing.record(Instant::now());
            let f = bus.feedback(&IDS)?;
            publish(shared, &f, &IDS, Value::Null);
            for id in ids {
                validate(*id, f.get(id), true)?;
            }
            let mut state = json!({"state":"moving","action":action,"message":"正在过渡到官方站姿","progress":ratio,"remainingSeconds":rounded((budget-began.elapsed().as_secs_f64()).max(0.),1)});
            for (key, value) in timing.stats().as_object().unwrap() {
                state[key] = value.clone();
            }
            if timing.count < 2 {
                state["commandMaxGapMs"] = Value::Null;
            }
            status(shared, state.clone());
            if ratio == 1.
                && ids.iter().all(|id| {
                    (position(&f[id]).unwrap_or(i32::MIN) as f64 - targets[id] as f64).abs()
                        <= 5. * 4096. / 360.
                })
            {
                let since = settled.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(100) {
                    guard()?;
                    state["state"] = json!("holding");
                    state["message"] = json!("已到官方站姿并使能保持");
                    return Ok(state);
                }
            } else {
                settled = None;
            }
            next = (next + Duration::from_millis(20)).max(Instant::now());
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
    })();
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            if attempted {
                let off = unload(bus)
                    .map(|v| v["message"].as_str().unwrap_or("").to_owned())
                    .unwrap_or_else(|e| format!("全部失能未确认：{e}"));
                if action == "stand" && timing.count > 0 {
                    let stats = timing.stats();
                    bail!(
                        "{e}；本次发令 {}Hz（目标50Hz），最大间隔 {}ms；{off}",
                        stats["commandHz"],
                        stats["commandMaxGapMs"]
                    )
                }
                bail!("{e}；{off}")
            }
            bail!("{e}；预检查未通过，未发送运动指令，原使能状态不变")
        }
    }
}
pub struct Job {
    pub command: Value,
    pub reply: oneshot::Sender<Result<Value>>,
    pub _permit: Option<OwnedSemaphorePermit>,
    pub off: bool,
}
#[derive(Clone)]
pub struct Owner {
    sender: Arc<Mutex<mpsc::Sender<Job>>>,
    pending_off: Arc<AtomicUsize>,
    pub done: Arc<AtomicBool>,
    pub busy: Arc<AtomicBool>,
    pub cancel: Arc<AtomicBool>,
    pub halt: Arc<AtomicBool>,
}
impl Owner {
    pub fn submit(
        &self,
        command: Value,
        permit: Option<OwnedSemaphorePermit>,
    ) -> Result<oneshot::Receiver<Result<Value>>> {
        let sender = self.sender.lock().unwrap();
        let off = command["action"] == "disable";
        if off {
            self.pending_off.fetch_add(1, Ordering::AcqRel);
            self.cancel.store(true, Ordering::Release);
        } else {
            if self.pending_off.load(Ordering::Acquire) > 0 {
                bail!("全部失能正在执行")
            };
            if self
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                bail!("已有舵机任务正在执行")
            }
            self.cancel.store(false, Ordering::Release);
            self.halt.store(false, Ordering::Release);
        }
        let (reply, rx) = oneshot::channel();
        if sender
            .send(Job {
                command,
                reply,
                _permit: permit,
                off,
            })
            .is_err()
        {
            if !off {
                self.busy.store(false, Ordering::Release);
            } else {
                self.pending_off.fetch_sub(1, Ordering::AcqRel);
            }
            bail!("舵机服务已退出")
        }
        Ok(rx)
    }
}
pub fn spawn(port: String, ids: Vec<u8>, shared: Shared, stop: Arc<AtomicBool>) -> Owner {
    let (tx, rx) = mpsc::channel::<Job>();
    let busy = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let halt = Arc::new(AtomicBool::new(false));
    let pending_off = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let owner = Owner {
        sender: Arc::new(Mutex::new(tx)),
        pending_off: pending_off.clone(),
        done: done.clone(),
        busy: busy.clone(),
        cancel: cancel.clone(),
        halt: halt.clone(),
    };
    std::thread::Builder::new().name("servo-owner".into()).spawn(move||{
        let mut bus=None;let mut stamps=std::collections::VecDeque::new();
        while !stop.load(Ordering::Acquire){
            if bus.is_none(){bus=Bus::open(&port).ok();}
            match rx.try_recv(){
                Ok(job)=>{
                    let result=if let Some(b)=bus.as_mut(){if job.command["action"]=="calibrate"{crate::guided::hardware(b,&job.command["plan"],std::path::Path::new(job.command["backupPath"].as_str().unwrap()),&cancel)}else if job.command["action"]=="policy"{crate::policy::execute(b,&job.command,&cancel,&halt,&shared)}else{execute(b,&job.command,&cancel,&shared)}}else{Err(anyhow::anyhow!("舵机串口不可用"))};
                    let state=if job.command["action"]=="calibrate"&&result.is_ok(){json!({"state":"idle","message":"硬件标定任务结束，请检查逐颗写入结果"})}else{result.as_ref().cloned().unwrap_or_else(|e|json!({"state":"failed","action":job.command["action"],"message":e.to_string()}))};
                    status(&shared,state.clone());shared.write().unwrap().log(if result.is_ok(){"INFO"}else{"WARN"},"servo-control",state["message"].as_str().unwrap_or(""));
                    if !job.off{busy.store(false,Ordering::Release);}else{pending_off.fetch_sub(1,Ordering::AcqRel);}let _=job.reply.send(result);
                },
                Err(mpsc::TryRecvError::Disconnected)=>break,
                Err(mpsc::TryRecvError::Empty)=>{
                    let start=Instant::now();
                    if let Some(b)=bus.as_mut(){match b.read_feedback(&ids){Ok(f)=>{stamps.push_back(start);if stamps.len()>100{stamps.pop_front();}let mut d=b.diagnostics.clone();d["observedScanHz"]=if stamps.len()>1{json!((stamps.len()-1) as f64/(start-*stamps.front().unwrap()).as_secs_f64())}else{Value::Null};publish(&shared,&f,&ids,d)},Err(e)=>{shared.write().unwrap().log("WARN","servos",&e.to_string());bus=None;}}}
                    else{shared.write().unwrap().sample("joints",json!({"configuredIds":ids,"servos":ids.iter().map(|id|json!({"id":id,"online":false,"ageMs":0})).collect::<Vec<_>>(),"error":"舵机串口不可用","targetHz":50,"readMode":"sync-read"}),true,Instant::now());}
                    std::thread::sleep(Duration::from_millis(if bus.is_some(){20}else{200}).saturating_sub(start.elapsed()));
                }
            }
        }
    done.store(true,Ordering::Release);}).expect("serial owner");
    owner
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timing_retains_total_count() {
        let mut timing = Timing::default();
        let start = Instant::now();
        for n in 0..1000 {
            timing.record(start + Duration::from_millis(n * 20));
        }
        let v = timing.stats();
        assert_eq!(v["commandCount"], 1000);
        assert_eq!(v["commandHz"], 50.);
        assert_eq!(v["commandMaxGapMs"], 20.);
    }
    #[test]
    fn goals_and_ranges() {
        let c = json!({"joints":{"references":{"34":2048},"directions":{}}});
        assert_eq!(
            target(34, 2048, &c, 0, 4095, 30f64.to_radians(), false).unwrap(),
            1707
        );
        assert!(target(34, 2048, &c, 0, 4095, -0.1, false).is_err());
        assert_eq!(encode(-16).unwrap(), vec![16, 128]);
        assert!(encode(32768).is_err());
        assert!(angle_request(&json!(true), &json!(0)).is_err());
    }
    #[test]
    fn no_invented_protection() {
        let f = json!({"fault":0,"voltage":5.9,"torque":1,"temperature":85,"load":1000});
        assert!(validate(12, Some(&f), true).is_ok());
        let mut f = f;
        f["voltage"] = json!(3.9);
        assert!(validate(12, Some(&f), true).is_err());
    }
    struct Mock {
        writes: Vec<(u8, BTreeMap<u8, Vec<u8>>)>,
        rows: Feedback,
        firmware: bool,
        fail_after_goal: bool,
    }
    impl Mock {
        fn new() -> Self {
            Self{writes:vec![],rows:IDS.into_iter().map(|id|(id,json!({"position":2048,"fault":0,"voltage":5.9,"torque":0,"accelerationRaw":5,"speedLimitRaw":300}))).collect(),firmware:true,fail_after_goal:false}
        }
    }
    impl Transport for Mock {
        fn calibrate(&mut self, _: u8, _: u16) -> Result<()> {
            bail!("Not used")
        }
        fn feedback(&mut self, ids: &[u8]) -> Result<Feedback> {
            if self.fail_after_goal && self.writes.iter().any(|(a, _)| *a == 42) {
                for row in self.rows.values_mut() {
                    if row["torque"] == 1 {
                        row["fault"] = json!(1)
                    }
                }
            };
            Ok(ids
                .iter()
                .filter_map(|id| self.rows.get(id).cloned().map(|v| (*id, v)))
                .collect())
        }
        fn read(&mut self, _: u8, address: u8, size: u8) -> Result<Vec<u8>> {
            let mut c = vec![0; size as usize];
            if address == 0 {
                c[0] = if self.firmware { 3 } else { 2 };
                c[1] = 46;
                c[33] = 4;
                c[16..18].copy_from_slice(&1000u16.to_le_bytes());
            } else if address == 48 {
                c.copy_from_slice(&1000u16.to_le_bytes());
            }
            Ok(c)
        }
        fn write(&mut self, id: u8, address: u8, data: &[u8]) -> Result<()> {
            self.sync(address, &BTreeMap::from([(id, data.to_vec())]))
        }
        fn sync(&mut self, address: u8, values: &BTreeMap<u8, Vec<u8>>) -> Result<()> {
            self.writes.push((address, values.clone()));
            for (id, v) in values {
                if address == 42 {
                    self.rows.get_mut(id).unwrap()["position"] =
                        json!(crate::servos::signed(u16::from_le_bytes([v[0], v[1]]), 15));
                }
                if address == 40 {
                    self.rows.get_mut(id).unwrap()["torque"] = json!(v[0]);
                }
                if address == 41 {
                    self.rows.get_mut(id).unwrap()["accelerationRaw"] = json!(v[0]);
                }
                if address == 46 {
                    self.rows.get_mut(id).unwrap()["speedLimitRaw"] =
                        json!(u16::from_le_bytes([v[0], v[1]]));
                }
            }
            Ok(())
        }
    }
    fn shared() -> Shared {
        Arc::new(std::sync::RwLock::new(crate::telemetry::Telemetry::new(
            true,
        )))
    }
    fn command() -> Value {
        json!({"action":"angle","id":34,"angleDeg":20,"calibration":{"joints":{"references":{"34":2048},"directions":{}}}})
    }
    #[test]
    fn angle_target_first_current_pid_untouched() {
        let mut bus = Mock::new();
        let result = execute(&mut bus, &command(), &AtomicBool::new(false), &shared()).unwrap();
        assert_eq!(result["state"], "commanded");
        assert_eq!(
            bus.writes.iter().map(|(a, _)| *a).collect::<Vec<_>>(),
            [42, 41, 46, 40]
        );
        assert!(bus.writes.iter().all(|(_, v)| v.keys().all(|id| *id == 34)));
    }
    #[test]
    fn preflight_failure_never_writes() {
        let mut bus = Mock::new();
        bus.firmware = false;
        assert!(execute(&mut bus, &command(), &AtomicBool::new(false), &shared()).is_err());
        assert!(bus.writes.is_empty());
    }
    #[test]
    fn disable_is_final_register_write() {
        let mut bus = Mock::new();
        bus.fail_after_goal = true;
        bus.rows.remove(&34);
        assert!(execute(&mut bus, &command(), &AtomicBool::new(false), &shared()).is_err());
        assert!(bus.writes.is_empty());
        let mut bus = Mock::new();
        unload(&mut bus).unwrap();
        assert_eq!(bus.writes.len(), 1);
        assert_eq!(bus.writes[0].0, 40);
        assert_eq!(bus.writes[0].1.len(), 15);
    }
    #[test]
    fn cancelled_preflight_never_writes() {
        let mut bus = Mock::new();
        assert!(execute(&mut bus, &command(), &AtomicBool::new(true), &shared()).is_err());
        assert!(bus.writes.is_empty());
    }
    #[test]
    fn enable_uses_eeprom_caps() {
        let mut bus = Mock::new();
        execute(
            &mut bus,
            &json!({"action":"enable"}),
            &AtomicBool::new(false),
            &shared(),
        )
        .unwrap();
        assert_eq!(bus.writes.first().unwrap().0, 42);
        assert_eq!(bus.writes.iter().filter(|(a, _)| *a == 48).count(), 15);
        assert!(bus
            .writes
            .iter()
            .all(|(a, _)| ![44, 45, 50, 51, 52].contains(a)));
        assert_eq!(bus.writes.last().unwrap().0, 40);
    }
    #[test]
    fn runtime_fault_unloads_all_as_last_write() {
        let mut bus = Mock::new();
        bus.fail_after_goal = true;
        let result = execute(&mut bus, &command(), &AtomicBool::new(false), &shared());
        assert!(result.is_err());
        let (a, values) = bus.writes.last().unwrap();
        assert_eq!(*a, 40);
        assert_eq!(values.len(), 15);
        assert!(values.values().all(|v| v == &[0]));
    }
    #[test]
    fn stand_50hz_mock_arrives_within_deadline() {
        let mut bus = Mock::new();
        let references: BTreeMap<_, _> = IDS.into_iter().map(|id| (id.to_string(), 2048)).collect();
        let began = Instant::now();
        let result=execute(&mut bus,&json!({"action":"stand","calibration":{"joints":{"references":references,"directions":{}}}}),&AtomicBool::new(false),&shared()).unwrap();
        assert_eq!(result["state"], "holding");
        assert!(began.elapsed().as_secs_f64() < 3.);
        assert!(result["commandHz"].as_f64().unwrap() > 45.);
        assert!(bus
            .writes
            .iter()
            .filter(|(a, _)| *a == 42)
            .skip(1)
            .all(|(_, v)| v.len() == 14 && !v.contains_key(&34)));
    }
}
