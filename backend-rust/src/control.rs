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
/// Never feed a single fault-bearing packet into inference. Re-read only the
/// affected IDs on the serial owner, without sending another position target.
/// A clean repeat replaces it; a repeated fault is left for normal validation.
/// A missing repeat forces feedback hold instead of reusing the suspect sample.
pub fn confirm_policy_faults<F>(fresh: &mut Feedback, diagnostics: &mut Value, mut read: F) -> Result<bool>
where F: FnMut(&[u8]) -> Result<(Feedback, Value)> {
    let ids: Vec<u8> = fresh.iter().filter(|(_, row)| row["fault"].as_u64() != Some(0))
        .map(|(&id, _)| id).collect();
    if ids.is_empty() { return Ok(false); }
    let suspect: Feedback = ids.iter().map(|id| (*id, fresh.remove(id).unwrap())).collect();
    let (confirmed, repeat_diagnostics) = read(&ids)?;
    let missing: Vec<u8> = ids.iter().filter(|id| !confirmed.contains_key(id)).copied().collect();
    diagnostics["faultConfirmation"] = json!({"suspect":suspect,"feedback":confirmed,
        "read":repeat_diagnostics,"unconfirmedIds":missing});
    for id in &ids {
        if let Some(row) = confirmed.get(id) { fresh.insert(*id, row.clone()); }
    }
    Ok(!missing.is_empty())
}
const PERIOD: Duration = Duration::from_millis(20);
/// Preserve the original 50Hz schedule; expired ticks are skipped, never replayed.
pub struct FrameClock {
    next: Instant,
    period: Duration,
    pub skipped: u64,
}
impl FrameClock {
    pub fn new(now: Instant) -> Self { Self::with_period(now, PERIOD) }
    pub fn with_period(now: Instant, period: Duration) -> Self {
        assert!(!period.is_zero()); Self { next: now, period, skipped: 0 }
    }
    fn advance(&mut self, now: Instant) -> Instant {
        self.next += self.period;
        if self.next < now {
            let missed = now.duration_since(self.next).as_nanos()/self.period.as_nanos()+1;
            self.skipped += missed as u64;
            self.next += self.period * missed as u32;
        }
        self.next
    }
    pub fn wait(&mut self) {
        let now = Instant::now();
        let next = self.advance(now);
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
}
/// A complete sample may bridge three missed reads; its timestamps remain unchanged.
/// The existing 150ms feedback age limit still applies after a scheduler stall.
#[derive(Default)]
pub struct FeedbackCoast {
    pub last: Option<Feedback>,
    at: Option<Instant>,
    pub misses: u32,
}
impl FeedbackCoast {
    pub fn sample(&mut self, fresh: &Feedback, ids: &[u8], now: Instant) -> Option<Feedback> {
        if ids.iter().all(|id| fresh.contains_key(id)) {
            self.last = Some(fresh.clone()); self.at = Some(now); self.misses = 0;
            return self.last.clone();
        }
        self.misses = self.misses.saturating_add(1);
        if self.misses <= 3 && self.at.is_some_and(|at| now.saturating_duration_since(at) < Duration::from_millis(150)) {
            self.last.clone()
        } else { None }
    }
    pub fn diagnostics(&self, mut value: Value, holding: bool) -> Value {
        if !value.is_object() { value = json!({}); }
        value["consecutiveFeedbackFailures"] = json!(self.misses);
        value["coasted"] = json!(!holding && self.misses > 0);
        value["feedbackHolding"] = json!(holding);
        value
    }
}
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
    fn control_feedback(&mut self, ids: &[u8]) -> Result<Feedback> { self.feedback(ids) }
    fn configuration_bytes(&mut self, id: u8) -> Result<Vec<u8>> { self.read(id, 0, 40) }
    fn tick_feedback(&mut self, ids: &[u8]) -> Result<Feedback> { self.feedback(ids) }
    fn policy_tick_feedback(&mut self, ids: &[u8]) -> Result<Feedback> { self.tick_feedback(ids) }
    fn fast_feedback(&mut self, ids: &[u8]) -> Result<Feedback> { self.tick_feedback(ids) }
    fn diagnostics(&self) -> Value { Value::Null }
    fn serial_trace(&self) -> Value { Value::Null }
    fn save_control_failure(&mut self, _evidence: &Value) -> Result<()> { Ok(()) }
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
    fn control_feedback(&mut self, ids: &[u8]) -> Result<Feedback> { self.read_control_feedback(ids) }
    fn configuration_bytes(&mut self, id: u8) -> Result<Vec<u8>> { self.read_configuration(id) }
    fn tick_feedback(&mut self, ids: &[u8]) -> Result<Feedback> { self.read_tick(ids) }
    fn policy_tick_feedback(&mut self, ids: &[u8]) -> Result<Feedback> { self.read_policy_tick(ids) }
    fn fast_feedback(&mut self, ids: &[u8]) -> Result<Feedback> { self.read_fast(ids) }
    fn diagnostics(&self) -> Value { self.diagnostics.clone() }
    fn serial_trace(&self) -> Value { json!(self.trace) }
    fn save_control_failure(&mut self, evidence: &Value) -> Result<()> {
        let calibration = std::path::PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE")
            .unwrap_or_else(|_| "/var/lib/microduck-observer/calibration.json".into()));
        let parent = calibration.parent().ok_or_else(|| anyhow::anyhow!("标定路径没有父目录"))?;
        let temporary = parent.join("control-failure-latest.json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(evidence)?)?;
        std::fs::rename(temporary, parent.join("control-failure-latest.json"))?;
        Ok(())
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
        // Choose the nearest equivalent only among encodable, hardware-valid
        // turns. Unbounded rounding can turn a valid 3656 into invalid -440.
        let (minimum, maximum) = if high > low {
            ((low as f64).max(-32767.), (high as f64).min(32767.))
        } else { (-32767., 32767.) };
        let first = ((minimum - goal) / 4096.).ceil();
        let last = ((maximum - goal) / 4096.).floor();
        if first > last {
            bail!("#{id} 目标超出编码范围或舵机硬件限位")
        }
        let turn = ((start as f64 - goal) / 4096.).round_ties_even().clamp(first, last);
        goal += turn * 4096.;
    }
    if goal.abs() > 32767. || (high > low && (goal < low as f64 || goal > high as f64)) {
        bail!("#{id} 目标超出编码范围或舵机硬件限位")
    }
    Ok(goal as i32)
}
pub fn configuration<T: Transport>(bus: &mut T, id: u8) -> Result<(i32, i32, u16)> {
    let c = bus.configuration_bytes(id)?;
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
pub fn goal_payload(value: i32) -> Result<Vec<u8>> {
    let mut bytes = encode(value)?;
    bytes.extend([0, 0, 0, 0]);
    Ok(bytes)
}
pub fn goals<T: Transport>(bus: &mut T, values: &BTreeMap<u8, i32>) -> Result<()> {
    let bytes = values
        .iter()
        .map(|(id, v)| Ok((*id, goal_payload(*v)?)))
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
            if let Some(profile) = r.as_object_mut().unwrap().remove("_profileMono").and_then(|v|v.as_f64()) {
                r["profileAgeMs"] = json!((now-profile).max(0.)*1000.);
            }
            r["id"] = json!(id);
            r["online"] = json!(f.contains_key(id));
            r
        })
        .collect();
    let mut s = shared.write().unwrap();
    s.sample("joints",json!({"configuredIds":ids,"servos":rows,"error":"","scanMs":diagnostics["elapsedMs"],"targetHz":diagnostics["feedbackTargetHz"].as_u64().unwrap_or(50),"readMode":"sync-read","diagnostics":diagnostics}),true,Instant::now());
    if let Some(v) = s.latest.get_mut("joints") {
        v["source"] = json!("hardware");
    }
}
pub fn status(shared: &Shared, v: Value) {
    shared.write().unwrap().control = v;
}
pub fn poll_tick<T: Transport>(bus: &mut T, ids: &[u8]) -> (Feedback, Value) {
    match bus.tick_feedback(ids) {
        Ok(f) => (f, bus.diagnostics()),
        Err(e) => (Feedback::new(), json!({"kind":"read","mono":crate::telemetry::monotonic(),
            "requestedIds":ids,"receivedIds":[],"missingIds":ids,"readError":e.to_string(),
            "rxBytes":0,"checksumErrors":0,"discardedBytes":0})),
    }
}
pub fn poll_policy_tick<T: Transport>(bus: &mut T, ids: &[u8]) -> (Feedback, Value) {
    match bus.policy_tick_feedback(ids) {
        Ok(f) => (f, bus.diagnostics()),
        Err(e) => (Feedback::new(), json!({"kind":"read","mono":crate::telemetry::monotonic(),
            "requestedIds":ids,"receivedIds":[],"missingIds":ids,"readError":e.to_string(),
            "rxBytes":0,"checksumErrors":0,"discardedBytes":0})),
    }
}
fn profile<T: Transport>(bus: &mut T, id: u8, f: &Value) -> Result<()> {
    profiles(bus, &[id], &BTreeMap::from([(id, f.clone())]))
}
pub const MANUAL_GAINS: (u8,u8) = (32,40);
pub const POLICY_GAINS: (u8,u8) = (6,20);
pub fn profiles<T: Transport>(bus: &mut T, ids: &[u8], feedback: &Feedback) -> Result<()> {
    profiles_with_gains(bus,ids,feedback,MANUAL_GAINS)
}
pub fn profiles_with_gains<T: Transport>(bus: &mut T, ids: &[u8], feedback: &Feedback, gains: (u8,u8)) -> Result<()> {
    let acceleration: BTreeMap<_,_> = ids.iter().filter(|id| feedback[id]["accelerationRaw"] != 0).map(|id| (*id,vec![0])).collect();
    let speed: BTreeMap<_,_> = ids.iter().filter(|id| feedback[id]["speedLimitRaw"] != 0).map(|id| (*id,0u16.to_le_bytes().to_vec())).collect();
    if !acceleration.is_empty() { bus.sync(41,&acceleration)?; }
    if !speed.is_empty() { bus.sync(46,&speed)?; }
    let gains: BTreeMap<_,_> = ids.iter().filter_map(|id| {
        let (kp,kd) = gains;
        (feedback[id]["kpRaw"] != kp || feedback[id]["kdRaw"] != kd)
            .then_some((*id, vec![kp,kd]))
    }).collect();
    if !gains.is_empty() { bus.sync(50,&gains)?; }
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
    let mut last_pose_diagnostics = String::new();
    let mut pose_frames = std::collections::VecDeque::new();
    let mut pose_preflight = Value::Null;
    let mut previous_command: BTreeMap<u8, i32> = BTreeMap::new();
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
            let f = if action == "angle" { bus.control_feedback(&[id])? } else { bus.feedback(&[id])? };
            let preflight_read_length = bus.diagnostics()["readLength"].clone();
            validate(id, f.get(&id), false)?;
            let (low, high, _) = configuration(bus, id)?;
            let configuration_cached = bus.diagnostics()["configurationCacheHit"].clone();
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
            let needs_enable = goal.is_some() && f[&id]["torque"] != 1;
            if needs_enable {
                attempted = true;
                // Align before enabling: never resume a stale stored goal.
                goals(bus, &BTreeMap::from([(id, position(&f[&id])?)]))?;
            }
            guard()?;
            profile(bus, id, &f[&id])?;
            guard()?;
            if needs_enable {
                bus.sync(40, &BTreeMap::from([(id, vec![1])]))?;
                let enabled = bus.feedback(&[id])?;
                validate(id, enabled.get(&id), true)?;
            }
            guard()?;
            if let Some(g) = goal {
                attempted = true;
                // HD1910 mode 4: a following torque=1 write can retain the old
                // internal target. The requested position must be the last write.
                goals(bus, &BTreeMap::from([(id, g)]))?;
            }
            guard()?;
            if let Some(goal) = goal {
                // Short feedback, three bounded observations. Never resend the
                // target/enable or invent a fresh row from a missing response.
                let mut confirmed = false;
                let mut clock = FrameClock::new(Instant::now());
                let confirmation_deadline = Instant::now() + Duration::from_millis(150);
                for _ in 0..3 {
                    guard()?;
                    clock.wait();
                    guard()?;
                    if Instant::now() >= confirmation_deadline { break; }
                    let after = bus.fast_feedback(&IDS)?;
                    publish(shared, &after, &IDS, bus.diagnostics());
                    guard()?;
                    for (other, row) in &after {
                        validate(*other, Some(row), *other == id)?;
                    }
                    if let Some(row) = after.get(&id) {
                        validate(id, Some(row), true)?;
                        confirmed = true;
                        break;
                    }
                }
                return Ok(json!({"state":"commanded","action":action,"id":id,"angleDeg":degree,"target":goal,"feedbackConfirmed":confirmed,"preflightReadLength":preflight_read_length,"configurationCached":configuration_cached,"message":if confirmed {format!("#{id} 目标已发送，反馈已确认；到位情况看实时角度")} else {format!("#{id} 目标已发送，暂未确认反馈；请核对实时角度，勿视为已到位")}}));
            }
            let after = bus.feedback(&IDS)?;
            publish(shared, &after, &IDS, Value::Null);
            validate(id, after.get(&id), false)?;
            if after[&id]["accelerationRaw"] != 0 || after[&id]["speedLimitRaw"] != 0 {
                bail!("#{id} 参数回读不一致")
            }
            return Ok(
                json!({"state":"configured","action":action,"id":id,"accelerationRaw":0,"speedLimitRaw":0,"message":format!("#{id} XgoDuck加速度0、速度0及运行增益已配置，未写目标角度或使能")}),
            );
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
        if action == "stand" && budget - began.elapsed().as_secs_f64() < 1.65 {
            bail!("总线预检查过慢，未执行运动")
        }
        attempted = true;
        goals(bus, &starts)?;
        profiles(bus, ids, &fresh)?;
        for id in ids {
            guard()?;
            let cap = cfg[id].2.to_le_bytes();
            if fresh[id]["torqueLimitRaw"] != cfg[id].2 {
                bus.write(*id, 48, &cap)?;
                if bus.read(*id, 48, 2)? != cap {
                    bail!("#{id} 最大输出设置未确认")
                }
            }
        }
        guard()?;
        let disabled: BTreeMap<_,_> = ids.iter().filter(|id| fresh[id]["torque"] != 1).map(|id| (*id,vec![1])).collect();
        if !disabled.is_empty() { bus.sync(40, &disabled)?; }
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
        pose_preflight = json!({"feedback":f,"starts":starts,"targets":targets,"configuration":cfg});
        previous_command = starts.clone();
        let trajectory = Instant::now();
        if budget - began.elapsed().as_secs_f64() < 1.65 {
            bail!("总线预处理过慢，无法在 3 秒内完成")
        }
        let mut clock = FrameClock::new(trajectory);
        let mut coast = FeedbackCoast::default();
        coast.sample(&f, ids, trajectory);
        let mut settled = None;

        loop {
            guard()?;
            let (fresh, diagnostics) = poll_tick(bus, &IDS);
            for (id,row) in &fresh { validate(*id, Some(row), ids.contains(id))?; }
            let sampled = coast.sample(&fresh, ids, Instant::now());
            let holding = sampled.is_none();
            let f = sampled.unwrap_or_else(|| coast.last.as_ref().unwrap().clone());
            let diagnostics = coast.diagnostics(diagnostics, holding);
            publish(shared, &fresh, &IDS, diagnostics.clone());
            // Manual poses and model HOME use a linear 1.5s trajectory at 50Hz.
            // Only policy inference applies the action EMA.
            let ratio = (trajectory.elapsed().as_secs_f64() / 1.5).min(1.0);
            let errors: BTreeMap<_,_> = ids.iter().map(|id| {
                let delta = (position(&f[id]).unwrap_or(i32::MIN) as f64 - targets[id] as f64) * 360. / 4096.;
                (*id, rounded(delta, 2))
            }).collect();
            let pending: Vec<_> = ids.iter().filter(|id| {
                (position(&f[id]).unwrap_or(i32::MIN) as f64-targets[id] as f64).abs()>5.*4096./360.
            }).map(|id| format!("#{} {}°",id,errors[id])).collect();
            last_pose_diagnostics = format!("卸力前末帧未到位[{}]；连续缺帧{}；反馈保持{}；到位计时{}ms",pending.join(","),coast.misses,holding,settled.map(|at:Instant| at.elapsed().as_millis()).unwrap_or(0));
            let commanded: BTreeMap<u8,i32> = starts
                .iter()
                .map(|(id, s)| {
                    (
                        *id,
                        if holding { position(&f[id]).unwrap_or(*s) }
                        else { (*s as f64 + (targets[id]-s) as f64 * ratio).round_ties_even() as i32 },
                    )
                })
                .collect();
            // The read precedes this frame's write: compare fresh register 67 with
            // the previous command, not with a newer interpolated target or cached 42.
            pose_frames.push_back(json!({"elapsedMs":began.elapsed().as_secs_f64()*1000.,
                "ratio":ratio,"freshFeedback":fresh,"previousCommand":previous_command,
                "nextCommand":commanded,"diagnostics":diagnostics,
                "positionErrorsDeg":errors,"holding":holding}));
            if pose_frames.len()>160 { pose_frames.pop_front(); }
            goals(bus, &commanded)?;
            previous_command = commanded;
            timing.record(Instant::now());
            let mut state = json!({"state":"moving","action":action,"message":"正在匀速分段调整姿势","progress":ratio,"remainingSeconds":rounded((budget-began.elapsed().as_secs_f64()).max(0.),1)});
            state["positionErrorsDeg"] = json!(errors);
            state["poseConfirmation"] = json!(last_pose_diagnostics);
            state["consecutiveFeedbackFailures"] = json!(coast.misses);
            state["feedbackHolding"] = json!(holding);
            state["skippedControlTicks"] = json!(clock.skipped);
            if holding { state["message"] = json!("反馈持续缺失，暂停目标下发并保持最后测得姿态"); }
            for (key, value) in timing.stats().as_object().unwrap() {
                state[key] = value.clone();
            }
            if timing.count < 2 {
                state["commandMaxGapMs"] = Value::Null;
            }
            status(shared, state.clone());
            if coast.misses == 0 && ratio == 1.
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
            clock.wait();
        }
    })();
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            if attempted {
                // Capture before unload changes both the feedback and serial ring.
                let mut evidence = json!({"eventTimeMs":crate::telemetry::epoch_ms(),
                    "action":action,"error":e.to_string(),"command":command,
                    "preflight":pose_preflight,"frames":pose_frames,
                    "lastPoseDiagnostics":last_pose_diagnostics,"stats":timing.stats(),
                    "serialTraceBeforeUnload":bus.serial_trace()});
                let off = unload(bus)
                    .map(|v| v["message"].as_str().unwrap_or("").to_owned())
                    .unwrap_or_else(|e| format!("全部失能未确认：{e}"));
                if action == "stand" {
                    evidence["unloadMessage"] = json!(off);
                    if let Err(save_error) = bus.save_control_failure(&evidence) {
                        shared.write().unwrap().log("WARN","servo-control",&format!("到位故障记录保存失败：{save_error}"));
                    }
                }
                if action == "stand" && timing.count > 0 {
                    let stats = timing.stats();
                    bail!(
                        "{e}；{last_pose_diagnostics}；本次发令 {}Hz（目标50Hz），最大间隔 {}ms；{off}",
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
        let mut bus=None;let mut stamps=std::collections::VecDeque::new();let mut clock=FrameClock::with_period(Instant::now(), Duration::from_millis(10));
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
                    if let Some(b)=bus.as_mut(){match b.read_tick(&ids){Ok(f)=>{stamps.push_back(start);if stamps.len()>100{stamps.pop_front();}let mut d=b.diagnostics.clone();d["feedbackTargetHz"]=json!(100);d["skippedScanTicks"]=json!(clock.skipped);d["observedScanHz"]=if stamps.len()>1{json!((stamps.len()-1) as f64/(start-*stamps.front().unwrap()).as_secs_f64())}else{Value::Null};publish(&shared,&f,&ids,d)},Err(e)=>{shared.write().unwrap().log("WARN","servos",&e.to_string());bus=None;}}}
                    else{shared.write().unwrap().sample("joints",json!({"configuredIds":ids,"servos":ids.iter().map(|id|json!({"id":id,"online":false,"ageMs":0})).collect::<Vec<_>>(),"error":"舵机串口不可用","targetHz":50,"readMode":"sync-read"}),true,Instant::now());}
                    if bus.is_some(){clock.wait();}else{std::thread::sleep(Duration::from_millis(200).saturating_sub(start.elapsed()));clock=FrameClock::with_period(Instant::now(),Duration::from_millis(10));}
                }
            }
        }
    done.store(true,Ordering::Release);}).expect("serial owner");
    owner
}
#[cfg(test)]
mod tests {
    #[test]
    fn single_corrupt_fault_is_replaced_only_by_fresh_confirmation() {
        let mut rows = Feedback::from([(14, json!({"fault":143,"voltage":12.,"_receivedMono":1.}))]);
        let mut diagnostic = json!({"checksumErrors":3});
        let holding = confirm_policy_faults(&mut rows, &mut diagnostic, |ids| {
            assert_eq!(ids, &[14]);
            Ok((Feedback::from([(14,json!({"fault":0,"voltage":7.3,"_receivedMono":2.}))]),json!({})))
        }).unwrap();
        assert!(!holding);
        assert_eq!(rows[&14]["fault"],0);
        assert_eq!(rows[&14]["_receivedMono"],2.);
        assert_eq!(diagnostic["faultConfirmation"]["suspect"]["14"]["fault"],143);
        assert_eq!(diagnostic["checksumErrors"],3);
    }
    #[test]
    fn confirmed_fault_and_voltage_violation_still_fail_validation() {
        let mut rows = Feedback::from([(14,json!({"fault":4,"voltage":7.3}))]);
        confirm_policy_faults(&mut rows,&mut json!({}), |_| Ok((rows_for_fault(4,7.3),json!({})))).unwrap();
        assert!(validate(14,rows.get(&14),false).unwrap_err().to_string().contains("故障码"));
        let mut rows = rows_for_fault(143,12.);
        confirm_policy_faults(&mut rows,&mut json!({}), |_| Ok((rows_for_fault(0,8.5),json!({})))).unwrap();
        assert!(validate(14,rows.get(&14),false).unwrap_err().to_string().contains("电压"));
    }
    fn rows_for_fault(fault:u8,voltage:f64) -> Feedback {
        Feedback::from([(14,json!({"fault":fault,"voltage":voltage}))])
    }
    #[test]
    fn missing_fault_confirmation_removes_suspect_and_requires_hold() {
        let mut rows=rows_for_fault(143,12.);
        assert!(confirm_policy_faults(&mut rows,&mut json!({}), |_| Ok((Feedback::new(),json!({})))).unwrap());
        assert!(!rows.contains_key(&14));
    }
    #[test]
    fn healthy_policy_feedback_never_adds_a_read() {
        let mut rows=rows_for_fault(0,7.3);
        assert!(!confirm_policy_faults(&mut rows,&mut json!({}), |_| panic!("unexpected bus read")).unwrap());
    }
    use super::*;
    #[test]
    fn xgoduck_goal_packet_and_restored_temporary_gains() {
        assert_eq!(goal_payload(-16).unwrap(), vec![16,128,0,0,0,0]);
        let mut bus = Mock::new();
        bus.rows.get_mut(&10).unwrap()["kpRaw"]=json!(6);
        bus.rows.get_mut(&10).unwrap()["kdRaw"]=json!(20);
        bus.rows.get_mut(&34).unwrap()["kpRaw"]=json!(10);
        let rows=bus.rows.clone();
        profiles(&mut bus,&[10,34],&rows).unwrap();
        let gain_write=bus.writes.iter().find(|(address,_)|*address==50).unwrap();
        assert_eq!(gain_write.1[&10],vec![32,40]);
        assert_eq!(gain_write.1[&34],vec![32,40]);
        let count=bus.writes.len();
        let updated=bus.rows.clone();
        profiles(&mut bus,&[10,34],&updated).unwrap();
        assert_eq!(bus.writes.len(),count);
        assert!(bus.writes.iter().all(|(address,_)|*address>=40));
    }
    #[test]
    fn policy_gains_switch_without_enable_or_goal_write_then_manual_restores() {
        let mut bus=Mock::new();
        for row in bus.rows.values_mut() { row["accelerationRaw"]=json!(0);row["speedLimitRaw"]=json!(0); }
        let ids=crate::policy::ORDER;
        let feedback=bus.rows.clone();
        profiles_with_gains(&mut bus,&ids,&feedback,POLICY_GAINS).unwrap();
        assert_eq!(bus.writes.len(),1);
        assert_eq!(bus.writes[0].0,50);
        assert_eq!(bus.writes[0].1.len(),14);
        assert!(bus.writes[0].1.values().all(|v| v==&vec![6,20]));
        assert!(!bus.writes[0].1.contains_key(&34));
        let feedback=bus.rows.clone();
        profiles(&mut bus,&ids,&feedback).unwrap();
        assert_eq!(bus.writes.len(),2);
        assert_eq!(bus.writes[1].0,50);
        assert!(bus.writes[1].1.values().all(|v|v==&vec![32,40]));
    }
    #[test]
    fn three_failed_reads_coast_without_mixing_or_restamping_then_hold_and_recover() {
        let now=Instant::now();
        let initial=Feedback::from([(12,json!({"position":100,"_receivedMono":1.0})),(14,json!({"position":200,"_receivedMono":1.0}))]);
        let partial=Feedback::from([(12,json!({"position":999,"_receivedMono":2.0}))]);
        let mut coast=FeedbackCoast::default();
        assert_eq!(coast.sample(&initial,&[12,14],now),Some(initial.clone()));
        for n in 1..=3 {
            assert_eq!(coast.sample(&partial,&[12,14],now+PERIOD*n),Some(initial.clone()));
            assert_eq!(coast.misses,n);
        }
        assert_eq!(coast.sample(&partial,&[12,14],now+PERIOD*4),None);
        let diagnostic=coast.diagnostics(json!({"missingIds":[14]}),true);
        assert_eq!(diagnostic["missingIds"],json!([14]));
        assert_eq!(diagnostic["feedbackHolding"],true);
        assert_eq!(coast.sample(&initial,&[12,14],now+PERIOD*5),Some(initial));
        assert_eq!(coast.misses,0);
    }
    #[test]
    fn scheduler_stall_cannot_extend_old_feedback_grace() {
        let now=Instant::now();let mut coast=FeedbackCoast::default();
        coast.sample(&Feedback::from([(12,json!({"position":1}))]),&[12],now);
        assert!(coast.sample(&Feedback::new(),&[12],now+Duration::from_millis(151)).is_none());
    }
    #[test]
    fn expired_ticks_are_skipped_on_original_schedule_without_a_burst() {
        let now=Instant::now();let mut clock=FrameClock::new(now);
        assert_eq!(clock.advance(now+Duration::from_millis(65)),now+Duration::from_millis(80));
        assert_eq!(clock.skipped,3);
        assert_eq!(clock.advance(now+Duration::from_millis(83)),now+Duration::from_millis(100));
        assert_eq!(clock.skipped,3);
    }
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
    fn nearest_turn_must_be_inside_hardware_and_encoding_limits() {
        let cal = json!({"joints":{"references":{"32":1996},"directions":{}}});
        let angle = (-145.9114870879065f64).to_radians();
        assert_eq!(target(32,1368,&cal,0,4095,angle,true).unwrap(),3656);
        assert_eq!(target(32,-430,&cal,-4096,4095,angle,true).unwrap(),-440);
        assert!(target(32,1368,&cal,1000,2000,angle,true).is_err());
        let unlimited = target(32,32767,&cal,0,0,angle,true).unwrap();
        assert!(unlimited.abs()<=32767);
        assert_eq!((unlimited-3656)%4096,0);
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
        fast_missing: usize,
        fast_reads: usize,
        stuck_id: Option<u8>,
        saved_failure: Option<Value>,
    }
    impl Mock {
        fn new() -> Self {
            Self{writes:vec![],rows:IDS.into_iter().map(|id|(id,json!({"position":2048,"fault":0,"voltage":5.9,"torque":0,"accelerationRaw":5,"speedLimitRaw":300,"kpRaw":32,"kiRaw":0,"kdRaw":40}))).collect(),firmware:true,fail_after_goal:false,fast_missing:0,fast_reads:0,stuck_id:None,saved_failure:None}
        }
    }
    impl Transport for Mock {
        fn save_control_failure(&mut self, evidence: &Value) -> Result<()> {
            self.saved_failure=Some(evidence.clone()); Ok(())
        }
        fn fast_feedback(&mut self, ids: &[u8]) -> Result<Feedback> {
            self.fast_reads += 1;
            let mut rows = self.feedback(ids)?;
            if self.fast_reads <= self.fast_missing { rows.remove(&34); }
            Ok(rows)
        }
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
                    assert_eq!(v.len(),6);
                    assert_eq!(&v[2..], &[0,0,0,0]);
                    self.rows.get_mut(id).unwrap()["goalCurrentRaw"] = json!(0);
                    self.rows.get_mut(id).unwrap()["speedLimitRaw"] = json!(0);
                    let target=crate::servos::signed(u16::from_le_bytes([v[0],v[1]]),15);
                    self.rows.get_mut(id).unwrap()["target"]=json!(target);
                    if self.stuck_id!=Some(*id) { self.rows.get_mut(id).unwrap()["position"]=json!(target); }
                }
                if address == 50 {
                    self.rows.get_mut(id).unwrap()["kpRaw"] = json!(v[0]);
                    self.rows.get_mut(id).unwrap()["kdRaw"] = json!(v[1]);
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
    struct Dropping {
        bus: Mock,
        tick: u32,
        voltage_fault: bool,
        held_targets: Option<BTreeMap<u8,Vec<u8>>>,
    }
    impl Transport for Dropping {
        fn calibrate(&mut self,id:u8,target:u16)->Result<()> { self.bus.calibrate(id,target) }
        fn feedback(&mut self,ids:&[u8])->Result<Feedback> { self.bus.feedback(ids) }
        fn tick_feedback(&mut self,ids:&[u8])->Result<Feedback> {
            self.tick+=1;
            let mut rows=self.bus.feedback(ids)?;
            if self.tick<=4 { rows.remove(&12); }
            if self.voltage_fault { rows.get_mut(&14).unwrap()["voltage"]=json!(3.9); }
            Ok(rows)
        }
        fn read(&mut self,id:u8,address:u8,size:u8)->Result<Vec<u8>> { self.bus.read(id,address,size) }
        fn write(&mut self,id:u8,address:u8,data:&[u8])->Result<()> { self.bus.write(id,address,data) }
        fn sync(&mut self,address:u8,values:&BTreeMap<u8,Vec<u8>>)->Result<()> {
            if self.tick==4 && address==42 { self.held_targets=Some(values.clone()); }
            self.bus.sync(address,values)
        }
    }
    #[test]
    fn stand_bridges_three_misses_holds_on_fourth_and_recovers_without_reenabling() {
        let mut bus=Dropping {bus:Mock::new(),tick:0,voltage_fault:false,held_targets:None};
        for row in bus.bus.rows.values_mut() {
            row["torque"]=json!(1);row["accelerationRaw"]=json!(0);
            row["speedLimitRaw"]=json!(0);row["torqueLimitRaw"]=json!(1000);
        }
        let references:BTreeMap<_,_>=IDS.into_iter().map(|id|(id.to_string(),2048)).collect();
        let result=execute(&mut bus,&json!({"action":"stand","calibration":{"joints":{"references":references,"directions":{}}}}),&AtomicBool::new(false),&shared()).unwrap();
        assert_eq!(result["state"],"holding");
        assert!(bus.held_targets.unwrap().values().all(|value|value==&goal_payload(2048).unwrap()));
        assert!(bus.bus.writes.iter().all(|(address,_)|*address==42));
    }
    #[test]
    fn valid_voltage_fault_during_a_partial_read_bypasses_coast_and_unloads() {
        let mut bus=Dropping {bus:Mock::new(),tick:0,voltage_fault:true,held_targets:None};
        let references:BTreeMap<_,_>=IDS.into_iter().map(|id|(id.to_string(),2048)).collect();
        let result=execute(&mut bus,&json!({"action":"stand","calibration":{"joints":{"references":references,"directions":{}}}}),&AtomicBool::new(false),&shared());
        assert!(result.unwrap_err().to_string().contains("3.9"));
        assert_eq!(bus.tick,1);
        assert_eq!(bus.bus.writes.last().unwrap().0,40);
        assert!(bus.bus.writes.last().unwrap().1.values().all(|value|value==&[0]));
    }
    #[test]
    fn angle_aligns_then_enables_then_sends_six_byte_target() {
        let mut bus = Mock::new();
        let result = execute(&mut bus, &command(), &AtomicBool::new(false), &shared()).unwrap();
        assert_eq!(result["state"], "commanded");
        assert_eq!(
            bus.writes.iter().map(|(a, _)| *a).collect::<Vec<_>>(),
            [42, 41, 46, 40, 42]
        );
        assert_eq!(bus.writes[0].1[&34], goal_payload(2048).unwrap());
        assert_ne!(bus.writes[4].1[&34], bus.writes[0].1[&34]);
        assert!(bus.writes.iter().all(|(_, v)| v.keys().all(|id| *id == 34)));
    }
    #[test]
    fn angle_retries_short_feedback_without_resending_or_unloading() {
        for missing in [1, 2, 3, 100] {
            let mut bus = Mock::new();
            bus.fast_missing = missing;
            let state = shared();
            let result = execute(&mut bus, &command(), &AtomicBool::new(false), &state).unwrap();
            assert_eq!(result["feedbackConfirmed"], missing < 3);
            assert_eq!(bus.fast_reads, (missing + 1).min(3));
            assert_eq!(bus.writes.iter().map(|(a, _)| *a).collect::<Vec<_>>(), [42,41,46,40,42]);
            assert_eq!(bus.rows[&34]["torque"], 1);
            let telemetry = state.read().unwrap();
            assert_eq!(telemetry.latest["joints"]["data"]["configuredIds"].as_array().unwrap().len(), 15);
            let rows = telemetry.latest["joints"]["data"]["servos"].as_array().unwrap();
            assert_eq!(rows.iter().find(|row| row["id"] == 34).unwrap()["online"], missing < 3);
        }
    }
    #[test]
    fn angle_partial_feedback_does_not_hide_a_real_fault() {
        let mut bus = Mock::new();
        bus.fast_missing = 100;
        bus.rows.get_mut(&12).unwrap()["voltage"] = json!(3.9);
        let error = execute(&mut bus, &command(), &AtomicBool::new(false), &shared()).unwrap_err();
        assert!(error.to_string().contains("3.9"));
        assert_eq!(bus.fast_reads, 1);
        assert_eq!(bus.writes.last().unwrap().0, 40);
        assert!(bus.writes.last().unwrap().1.values().all(|value| value == &[0]));
    }
    #[test]
    fn bulk_profiles_group_only_changed_fields_and_do_not_write_enable_or_current() {
        let mut bus=Mock::new();
        bus.rows.get_mut(&10).unwrap()["accelerationRaw"]=json!(0);
        bus.rows.get_mut(&11).unwrap()["speedLimitRaw"]=json!(0);
        let rows=bus.rows.clone();
        profiles(&mut bus,&[10,11,12],&rows).unwrap();
        assert_eq!(bus.writes.iter().map(|(address,_)|*address).collect::<Vec<_>>(),[41,46]);
        assert_eq!(bus.writes[0].1.keys().copied().collect::<Vec<_>>(),[11,12]);
        assert_eq!(bus.writes[1].1.keys().copied().collect::<Vec<_>>(),[10,12]);
        let fresh=bus.rows.clone();
        profiles(&mut bus,&[10,11,12],&fresh).unwrap();
        assert_eq!(bus.writes.len(),2);
    }
    #[test]
    fn angle_fast_feedback_fault_still_unloads() {
        let mut bus = Mock::new();
        bus.rows.get_mut(&34).unwrap()["torque"] = json!(1);
        bus.fail_after_goal = true;
        assert!(execute(&mut bus, &command(), &AtomicBool::new(false), &shared()).is_err());
        assert_eq!(bus.fast_reads, 1);
        assert_eq!(bus.writes.last().unwrap().0, 40);
        assert!(bus.writes.last().unwrap().1.values().all(|value| value == &[0]));
    }
    #[test]
    fn angle_on_enabled_servo_never_rewrites_enable() {
        let mut bus = Mock::new();
        bus.rows.get_mut(&34).unwrap()["torque"] = json!(1);
        bus.rows.get_mut(&34).unwrap()["accelerationRaw"] = json!(0);
        bus.rows.get_mut(&34).unwrap()["speedLimitRaw"] = json!(0);
        execute(&mut bus, &command(), &AtomicBool::new(false), &shared()).unwrap();
        assert_eq!(bus.writes.iter().map(|(a, _)| *a).collect::<Vec<_>>(), [42]);
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
    fn timeout_evidence_retains_live_target_and_enabled_feedback_before_unload() {
        let mut bus=Mock::new(); bus.stuck_id=Some(30);
        let references:BTreeMap<_,_>=IDS.into_iter().map(|id|(id.to_string(),if id==30 { 2200 } else { 2048 })).collect();
        let result=execute(&mut bus,&json!({"action":"stand","calibration":{"joints":{"references":references,"directions":{}}}}),&AtomicBool::new(false),&shared());
        assert!(result.unwrap_err().to_string().contains("#30"));
        let evidence=bus.saved_failure.as_ref().unwrap();
        let frames=evidence["frames"].as_array().unwrap();
        assert!(frames.len()>60 && frames.len()<=160);
        let last=frames.last().unwrap();
        assert_eq!(last["freshFeedback"]["30"]["position"],2048);
        assert_eq!(last["freshFeedback"]["30"]["target"],2200);
        assert_eq!(last["previousCommand"]["30"],2200);
        assert_eq!(last["freshFeedback"]["30"]["torque"],1);
        assert_eq!(bus.rows[&30]["torque"],0);
        assert_eq!(bus.writes.last().unwrap().0,40);
    }
    #[test]
    fn manual_pose_advances_linearly_at_50hz_and_confirms_final_target() {
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
        let moves: Vec<_> = bus.writes.iter().filter(|(a,_)| *a==42).skip(1).collect();
        assert!(!moves.is_empty());
        let final_goal=&moves.last().unwrap().1[&12];
        assert_ne!(&moves[0].1[&12],final_goal);
        assert!(moves.len()>60);
        let midpoint=&moves[moves.len()/2].1[&12];
        assert_ne!(midpoint,final_goal);
        assert_ne!(midpoint,&moves[0].1[&12]);
    }
}
