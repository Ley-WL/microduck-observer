use anyhow::{bail, Result};
use serde_json::{json, Value};
use serialport::SerialPort;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    time::{Duration, Instant},
};
pub fn packet(id: u8, instruction: u8, params: &[u8]) -> Vec<u8> {
    let mut p = vec![255, 255, id, (params.len() + 2) as u8, instruction];
    p.extend(params);
    let sum = p[2..].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    p.push(!sum);
    p
}
pub fn signed(v: u16, bit: u32) -> i32 {
    if v & (1 << bit) != 0 {
        -i32::from(v & ((1 << bit) - 1))
    } else {
        i32::from(v)
    }
}
pub fn decode(data: &[u8], error: u8) -> Result<Value> {
    if data.len() != 31 {
        bail!("Expected registers 40..70")
    }
    let word = |n| u16::from_le_bytes([data[n], data[n + 1]]);
    Ok(
        json!({"position":signed(word(16),15),"velocityRaw":signed(word(18),15),"voltage":data[22]as f64/10.,"temperature":data[23],"currentRaw":signed(word(29),15),"load":signed(word(20),10),"torque":data[0],"fault":data[25]|error,"accelerationRaw":data[1],"goalPositionRaw":signed(word(2),15),"goalCurrentRaw":signed(word(4),15),"speedLimitRaw":signed(word(6),15),"torqueLimitRaw":word(8),"kpRaw":data[10],"kdRaw":data[11],"kiRaw":data[12],"target":signed(word(27),15),"angle":null}),
    )
}
pub fn decode_fast(data: &[u8], error: u8) -> Result<Value> {
    if data.len() != 15 { bail!("Expected registers 56..70"); }
    let word = |n| u16::from_le_bytes([data[n], data[n+1]]);
    Ok(json!({"position":signed(word(0),15),"velocityRaw":signed(word(2),15),
        "load":signed(word(4),10),"voltage":data[6] as f64/10.,"temperature":data[7],
        "fault":data[9]|error,"target":signed(word(11),15),"currentRaw":signed(word(13),15)}))
}
pub struct Parser {
    pub pending: Vec<u8>,
    pub bad: u64,
    pub discarded: u64,
    pub rejected: Vec<Vec<u8>>,
}
impl Parser {
    pub fn new() -> Self {
        Self {
            pending: vec![],
            bad: 0,
            discarded: 0,
            rejected: vec![],
        }
    }
    pub fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.pending.extend(data);
        let mut frames = vec![];
        loop {
            if self.pending.len() < 4 {
                break;
            }
            if self.pending[..2] != [255, 255] || !(2..=64).contains(&self.pending[3]) {
                self.pending.remove(0);
                self.discarded += 1;
                continue;
            }
            let size = self.pending[3] as usize + 4;
            if self.pending.len() < size {
                break;
            }
            let f = &self.pending[..size];
            if f[2..].iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 255 {
                self.bad += 1;
                if self.rejected.len() < 8 {
                    self.rejected.push(f.to_vec());
                }
                // A truncated reply can include the next reply's header. Only discard
                // the first byte of an invalid candidate, then search again; never
                // consume the declared length until the entire frame validates.
                self.pending.remove(0);
                self.discarded += 1;
                continue;
            }
            frames.push(self.pending.drain(..size).collect());
        }
        frames
    }
}
pub struct Bus {
    pub serial: Box<dyn SerialPort>,
    pub diagnostics: Value,
    pub trace: std::collections::VecDeque<Value>,
    profiles: BTreeMap<u8, Value>,
    profile_scan: Option<Instant>,
    configurations: BTreeMap<u8, (Instant, Vec<u8>)>,
}
fn reusable_profile(row: &Value, now: f64) -> bool {
    row["torque"] == 1 && row["accelerationRaw"] == 0 && row["speedLimitRaw"] == 0 && row["kpRaw"] == 32 && row["kdRaw"] == 40
        && row["_profileMono"].as_f64().is_some_and(|at| (0.0..1.0).contains(&(now-at)))
}
fn reusable_configuration(at: Instant, now: Instant) -> bool {
    now.checked_duration_since(at).is_some_and(|age| age < Duration::from_secs(1))
}
impl Bus {
    // Consume leftover replies before starting another transaction, as rustypot does.
    // Never silently append a previous request's reply to a new sample.
    fn prepare_input(&mut self) -> Result<Vec<u8>> {
        let mut discarded = vec![];
        for _ in 0..3 {
            let waiting = self.serial.bytes_to_read()? as usize;
            if waiting == 0 { return Ok(discarded); }
            let mut bytes = vec![0; waiting.min(1024)];
            let n = self.serial.read(&mut bytes)?;
            discarded.extend_from_slice(&bytes[..n]);
            std::thread::sleep(Duration::from_millis(5));
        }
        if self.serial.bytes_to_read()? != 0 { bail!("串口残留回复持续到达，未发送新请求"); }
        Ok(discarded)
    }
    fn record(&mut self, packet: &[u8]) {
        self.trace.push_back(
            json!({"kind":"write","mono":crate::telemetry::monotonic(),"txHex":hex(packet)}),
        );
        if self.trace.len() > 8 {
            self.trace.pop_front();
        }
    }
    pub fn exchange(
        &mut self,
        id: u8,
        instruction: u8,
        params: &[u8],
        expected: usize,
    ) -> Result<Vec<u8>> {
        if instruction != 2 {
            self.configurations.clear();
            self.profile_scan = None;
        }
        self.prepare_input()?;
        let tx = packet(id, instruction, params);
        self.serial.write_all(&tx)?;
        self.record(&tx);
        let start = Instant::now();
        let mut parser = Parser::new();
        while start.elapsed() < Duration::from_millis(200) {
            let mut buffer = [0; 128];
            match self.serial.read(&mut buffer) {
                Ok(n) => {
                    for f in parser.feed(&buffer[..n]) {
                        if f[2] != id || f.len() != expected + 6 {
                            continue;
                        }
                        if f[4] != 0 {
                            bail!("#{id} 返回故障 {}", f[4]);
                        }
                        return Ok(f[5..f.len() - 1].to_vec());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e.into()),
            }
        }
        bail!("#{id} 指令应答超时")
    }
    pub fn read_register(&mut self, id: u8, address: u8, size: u8) -> Result<Vec<u8>> {
        self.exchange(id, 2, &[address, size], size as usize)
    }
    pub fn read_configuration(&mut self, id: u8) -> Result<Vec<u8>> {
        if let Some((at, data)) = self.configurations.get(&id) {
            if reusable_configuration(*at, Instant::now()) {
                self.diagnostics["configurationCacheHit"] = json!(true);
                return Ok(data.clone());
            }
        }
        let data = self.read_register(id, 0, 40)?;
        self.diagnostics["configurationCacheHit"] = json!(false);
        if data.len() == 40 && data[..2] == [3,46] && data[33] == 4 {
            self.configurations.insert(id, (Instant::now(), data.clone()));
        }
        Ok(data)
    }
    pub fn write_register(&mut self, id: u8, address: u8, data: &[u8]) -> Result<()> {
        let mut params = vec![address];
        params.extend(data);
        self.exchange(id, 3, &params, 0)?;
        if matches!(address, 40 | 41 | 46 | 48 | 50 | 51 | 52) { self.profile_scan = None; }
        Ok(())
    }
    pub fn sync_write(&mut self, address: u8, values: &BTreeMap<u8, Vec<u8>>) -> Result<()> {
        if address < 40 { self.configurations.clear(); self.profile_scan = None; }
        // Invalidate before a potentially partial/failed write as well.
        if matches!(address, 40 | 41 | 46 | 48 | 50 | 51 | 52) { self.profile_scan = None; }
        let size = values
            .values()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty write"))?
            .len();
        if size == 0 || values.values().any(|v| v.len() != size) {
            bail!("Invalid sync write");
        }
        if address == 42 && size == 6 && values.keys().any(|id| self.profiles.get(id)
            .is_none_or(|row| row["goalCurrentRaw"] != 0 || row["speedLimitRaw"] != 0)) {
            self.profile_scan = None;
        }
        let mut params = vec![address, size as u8];
        for (id, v) in values {
            params.push(*id);
            params.extend(v);
        }
        let tx = packet(254, 0x83, &params);
        let discarded = self.prepare_input()?;
        if !discarded.is_empty() {
            self.trace.push_back(json!({"kind":"drain-before-write","mono":crate::telemetry::monotonic(),"rxHex":hex(&discarded),"rxBytes":discarded.len()}));
        }
        self.serial.write_all(&tx)?;
        self.record(&tx);
        if matches!(address, 40 | 41 | 46 | 48 | 50 | 51 | 52) { self.profile_scan = None; }
        Ok(())
    }
    pub fn open(port: &str) -> Result<Self> {
        Ok(Self {
            serial: serialport::new(port, 1_000_000)
                .timeout(Duration::from_millis(2))
                .open()?,
            diagnostics: Value::Null,
            trace: std::collections::VecDeque::new(),
            profiles: BTreeMap::new(),
            profile_scan: None,
            configurations: BTreeMap::new(),
        })
    }
    pub fn read_feedback(&mut self, ids: &[u8]) -> Result<BTreeMap<u8, Value>> {
        let blocks = self.read_blocks(ids, 40, 31)?;
        let mut rows = BTreeMap::new();
        for (id, (data, received, read_ms)) in blocks {
            let mut row = decode(&data[1..], data[0])?;
            row["id"] = json!(id);
            row["_receivedMono"] = json!(received);
            row["_profileMono"] = json!(received);
            row["readMs"] = json!(read_ms);
            row["profileAgeMs"] = json!(0);
            row["profileFresh"] = json!(true);
            row["ageMs"] = json!((crate::telemetry::monotonic()-received).max(0.)*1000.);
            self.profiles.insert(id, row.clone());
            rows.insert(id, row);
        }
        Ok(rows)
    }
    /// HD1910 position/velocity/load/voltage/fault/target/current occupy 56..70.
    /// Configuration and torque-enable at 40..55 are refreshed once a second.
    pub fn read_tick(&mut self, ids: &[u8]) -> Result<BTreeMap<u8, Value>> {
        self.read_tick_budget(ids,Duration::from_millis(30))
    }
    pub fn read_policy_tick(&mut self, ids: &[u8]) -> Result<BTreeMap<u8, Value>> {
        self.read_tick_budget(ids,Duration::from_millis(6))
    }
    fn read_tick_budget(&mut self, ids: &[u8], budget: Duration) -> Result<BTreeMap<u8, Value>> {
        if self.profile_scan.is_none_or(|at| at.elapsed() >= Duration::from_secs(1)) {
            let rows = self.read_feedback(ids)?;
            // Only a scheduled whole-bus scan advances this timer. A selected
            // joint's preflight must not postpone all other profile refreshes.
            self.profile_scan = Some(Instant::now());
            return Ok(rows);
        }
        self.read_fast_budget(ids,budget)
    }
    pub fn read_control_feedback(&mut self, ids: &[u8]) -> Result<BTreeMap<u8, Value>> {
        let now = crate::telemetry::monotonic();
        if self.profile_scan.is_some() && ids.iter().all(|id| self.profiles.get(id).is_some_and(|row| reusable_profile(row, now))) {
            // Positions/fault/voltage always come from a new bus read.
            let rows = self.read_fast(ids)?;
            if ids.iter().any(|id| !rows.contains_key(id)) {
                for id in ids { self.profiles.remove(id); self.configurations.remove(id); }
                self.profile_scan = None;
            }
            Ok(rows)
        } else { self.read_feedback(ids) }
    }
    pub fn read_fast(&mut self, ids: &[u8]) -> Result<BTreeMap<u8, Value>> {
        self.read_fast_budget(ids,Duration::from_millis(30))
    }
    fn read_fast_budget(&mut self, ids: &[u8], budget: Duration) -> Result<BTreeMap<u8, Value>> {
        let blocks = self.read_blocks_budget(ids,56,15,budget)?;
        let now = crate::telemetry::monotonic();
        let mut rows = BTreeMap::new();
        for (id, (data, received, read_ms)) in blocks {
            if let Some(profile) = self.profiles.get(&id) {
                let mut row = profile.clone();
                let fresh = decode_fast(&data[1..], data[0])?;
                for (key, value) in fresh.as_object().unwrap() { row[key] = value.clone(); }
                row["_receivedMono"] = json!(received);
                row["readMs"] = json!(read_ms);
                row["ageMs"] = json!((now-received).max(0.)*1000.);
                row["profileAgeMs"] = json!((now-row["_profileMono"].as_f64().unwrap()).max(0.)*1000.);
                row["profileFresh"] = json!(false);
                rows.insert(id, row);
            }
        }
        self.diagnostics["profileMissingIds"] = json!(ids.iter().filter(|id| !self.profiles.contains_key(id)).collect::<Vec<_>>());
        self.diagnostics["missingIds"] = json!(ids.iter().filter(|id| !rows.contains_key(id)).collect::<Vec<_>>());
        Ok(rows)
    }
    fn read_blocks(&mut self, ids: &[u8], address: u8, length: u8) -> Result<BTreeMap<u8, (Vec<u8>, f64, f64)>> {
        self.read_blocks_budget(ids,address,length,Duration::from_millis(30))
    }
    fn read_blocks_budget(&mut self, ids: &[u8], address: u8, length: u8, budget: Duration) -> Result<BTreeMap<u8, (Vec<u8>, f64, f64)>> {
        let discarded = self.prepare_input()?;
        let mut params = vec![address, length];
        params.extend(ids);
        let tx = packet(254, 0x82, &params);
        self.serial.write_all(&tx)?;
        self.record(&tx);
        // Linux tcdrain on this UART adds about 12ms (readonly A/B verified).
        // FIFO ordering already preserves goal -> read-request; do not block on
        // tcdrain. This budget includes the request's actual transmit latency.
        let tx_drain_ms=0.0;
        let start = Instant::now();
        let mut p = Parser::new();
        let mut rows = BTreeMap::new();
        let mut raw: Vec<u8> = vec![];
        if budget<Duration::from_millis(30) { self.serial.set_timeout(Duration::from_millis(1))?; }
        while start.elapsed() < budget && rows.len() < ids.len() {
            let mut buf = [0; 1024];
            match self.serial.read(&mut buf) {
                Ok(n) => {
                    raw.extend(&buf[..n]);
                    for f in p.feed(&buf[..n]) {
                        let id = f[2];
                        if !ids.contains(&id) || rows.contains_key(&id) || f.len() != length as usize + 6 {
                            continue;
                        }
                        rows.insert(id, (f[4..f.len()-1].to_vec(), crate::telemetry::monotonic(), start.elapsed().as_secs_f64()*1000.));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => { let _=self.serial.set_timeout(Duration::from_millis(2));return Err(e.into()); },
            }
        }
        self.serial.set_timeout(Duration::from_millis(2))?;
        self.diagnostics = json!({"txDrainMs":tx_drain_ms,"replyBudgetMs":budget.as_secs_f64()*1000.,"kind":"read","readAddress":address,"readLength":length,"mono":crate::telemetry::monotonic(),"requestedIds":ids,"receivedIds":rows.keys().collect::<Vec<_>>(),"missingIds":ids.iter().filter(|id|!rows.contains_key(id)).collect::<Vec<_>>(),"elapsedMs":start.elapsed().as_secs_f64()*1000.,"rxBytes":raw.len(),"rxHex":hex(&raw),"txHex":hex(&tx),"checksumErrors":p.bad,"discardedBytes":p.discarded,"discardedBeforeRead":discarded.len(),"discardedBeforeReadHex":hex(&discarded),"pendingHex":hex(&p.pending),"rejectedFrames":p.rejected.iter().map(|f|json!({"id":f[2],"reason":"checksum","hex":hex(f)})).collect::<Vec<_>>()});
        self.trace.push_back(self.diagnostics.clone());
        if self.trace.len() > 8 {
            self.trace.pop_front();
        }
        Ok(rows)
    }
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}
#[cfg(test)]
mod tests {
    #[test]
    fn restored_gain_profile_applies_to_mouth_and_other_joints() {
        let mut row=json!({"id":34,"torque":1,"accelerationRaw":0,"speedLimitRaw":0,"kpRaw":32,"kdRaw":40,"_profileMono":1.});
        assert!(reusable_profile(&row,1.1));
        row["id"]=json!(14);
        assert!(reusable_profile(&row,1.1));
        row["kpRaw"]=json!(6);
        assert!(!reusable_profile(&row,1.1));
    }
    #[test]
    fn control_profile_reuse_requires_enabled_correct_profile_and_real_recent_timestamp() {
        let row=json!({"torque":1,"accelerationRaw":0,"speedLimitRaw":0,"kpRaw":32,"kdRaw":40,"_profileMono":1.});
        assert!(reusable_profile(&row,1.999));
        assert!(!reusable_profile(&row,2.));
        assert!(!reusable_profile(&row,0.9));
        for (key,value) in [("torque",0),("torque",128),("accelerationRaw",5),("speedLimitRaw",300)] {
            let mut invalid=row.clone();invalid[key]=json!(value);
            assert!(!reusable_profile(&invalid,1.1));
        }
        assert!(!reusable_profile(&json!({"torque":1,"accelerationRaw":0,"speedLimitRaw":0,"kpRaw":32,"kdRaw":40}),1.));
    }
    #[test]
    fn configuration_cache_expiry_cannot_be_extended_by_reads_or_a_future_timestamp() {
        let at=Instant::now();
        assert!(reusable_configuration(at,at+Duration::from_millis(999)));
        assert!(!reusable_configuration(at,at+Duration::from_secs(1)));
        assert!(!reusable_configuration(at,at-Duration::from_millis(1)));
    }
    use super::*;
    use crate::telemetry::IDS;
    #[test]
    fn fast_block_matches_full_register_decode_including_fault_and_signed_fields() {
        let mut data = [0;31];
        for (offset,value) in [(16,0x8010u16),(18,0x8011),(20,0x0407),(27,2048),(29,0x8002)] {
            data[offset..offset+2].copy_from_slice(&value.to_le_bytes());
        }
        data[22]=59;data[23]=80;data[25]=4;
        let full=decode(&data,2).unwrap();
        let fast=decode_fast(&data[16..],2).unwrap();
        for (key,value) in fast.as_object().unwrap() { assert_eq!(&full[key],value,"{key}"); }
        assert_eq!(fast["fault"],6);
        assert!(decode_fast(&data[16..30],0).is_err());
        assert_eq!(packet(254,0x82,&[56,15,10,11])[5..7],[56,15]);
        assert_eq!((15+6)*15,315);
    }
    #[test]
    fn checksum_fragmentation() {
        let mut p = Parser::new();
        let frame = packet(24, 0, &[0; 31]);
        assert!(p.feed(&frame[..7]).is_empty());
        assert_eq!(p.feed(&frame[7..]), vec![frame]);
        assert_eq!(p.bad, 0);
    }
    #[test]
    fn real_corruption_preserves_every_remaining_valid_reply() {
        let cases: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/servo-resync.json"
        )).unwrap();
        for case in cases.as_array().unwrap() {
            let text = case["rxHex"].as_str().unwrap();
            let bytes: Vec<u8> = (0..text.len()).step_by(2)
                .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect();
            let expected: Vec<u8> = case["expectedIds"].as_array().unwrap().iter()
                .map(|id| id.as_u64().unwrap() as u8).collect();
            for chunk_size in 1..=bytes.len() {
                let mut parser = Parser::new();
                let mut ids = vec![];
                for chunk in bytes.chunks(chunk_size) {
                    ids.extend(parser.feed(chunk).iter().map(|frame| frame[2]));
                }
                assert_eq!(ids, expected, "source={} chunk={chunk_size}", case["source"]);
                assert!(parser.bad > 0);
            }
        }
    }
    #[test]
    fn valid_payload_with_header_bytes_is_not_split() {
        let mut payload = [0; 31];
        payload[8..10].copy_from_slice(&[255, 255]);
        let frame = packet(12, 0, &payload);
        let mut parser = Parser::new();
        assert_eq!(parser.feed(&frame), vec![frame]);
        assert_eq!(parser.bad, 0);
    }
    #[test]
    fn signed_feedback() {
        let mut data = [0; 31];
        data[0] = 1;
        data[16..18].copy_from_slice(&0x8010u16.to_le_bytes());
        data[22] = 59;
        let v = decode(&data, 0).unwrap();
        assert_eq!(v["position"], -16);
        assert_eq!(v["voltage"], 5.9);
        assert_eq!(v["torque"], 1);
    }
    #[test]
    fn sync_read_wire() {
        let p = packet(254, 0x82, &[40, 31, 10, 11]);
        assert_eq!(&p[2..p.len() - 1], &[254, 6, 0x82, 40, 31, 10, 11]);
        assert_eq!(p[2..].iter().fold(0u8, |a, b| a.wrapping_add(*b)), 255);
        assert_eq!(IDS.len(), 15);
    }
}
