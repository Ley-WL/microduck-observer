use anyhow::{bail, Result};
use serde_json::{json, Value};
use serialport::{ClearBuffer, SerialPort};
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
        json!({"position":signed(word(16),15),"voltage":data[22]as f64/10.,"temperature":data[23],"currentRaw":signed(word(29),15),"load":signed(word(20),10),"torque":data[0],"fault":data[25]|error,"accelerationRaw":data[1],"goalPositionRaw":signed(word(2),15),"goalCurrentRaw":signed(word(4),15),"speedLimitRaw":signed(word(6),15),"torqueLimitRaw":word(8),"kpRaw":data[10],"kdRaw":data[11],"kiRaw":data[12],"target":signed(word(27),15),"angle":null}),
    )
}
pub struct Parser {
    pub pending: Vec<u8>,
    pub bad: u64,
    pub discarded: u64,
}
impl Parser {
    pub fn new() -> Self {
        Self {
            pending: vec![],
            bad: 0,
            discarded: 0,
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
            let f: Vec<u8> = self.pending.drain(..size).collect();
            if f[2..].iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 255 {
                self.bad += 1;
                continue;
            }
            frames.push(f);
        }
        frames
    }
}
pub struct Bus {
    pub serial: Box<dyn SerialPort>,
    pub diagnostics: Value,
    pub trace: std::collections::VecDeque<Value>,
}
impl Bus {
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
        self.serial.clear(ClearBuffer::Input)?;
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
    pub fn write_register(&mut self, id: u8, address: u8, data: &[u8]) -> Result<()> {
        let mut params = vec![address];
        params.extend(data);
        self.exchange(id, 3, &params, 0)?;
        Ok(())
    }
    pub fn sync_write(&mut self, address: u8, values: &BTreeMap<u8, Vec<u8>>) -> Result<()> {
        let size = values
            .values()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty write"))?
            .len();
        if size == 0 || values.values().any(|v| v.len() != size) {
            bail!("Invalid sync write");
        }
        let mut params = vec![address, size as u8];
        for (id, v) in values {
            params.push(*id);
            params.extend(v);
        }
        let tx = packet(254, 0x83, &params);
        self.serial.write_all(&tx)?;
        self.record(&tx);
        Ok(())
    }
    pub fn open(port: &str) -> Result<Self> {
        Ok(Self {
            serial: serialport::new(port, 1_000_000)
                .timeout(Duration::from_millis(2))
                .open()?,
            diagnostics: Value::Null,
            trace: std::collections::VecDeque::new(),
        })
    }
    pub fn read_feedback(&mut self, ids: &[u8]) -> Result<BTreeMap<u8, Value>> {
        let before = self.serial.bytes_to_read()?;
        self.serial.clear(ClearBuffer::Input)?;
        let mut params = vec![40, 31];
        params.extend(ids);
        let tx = packet(254, 0x82, &params);
        self.serial.write_all(&tx)?;
        self.record(&tx);
        let start = Instant::now();
        let mut p = Parser::new();
        let mut rows = BTreeMap::new();
        let mut raw: Vec<u8> = vec![];
        while start.elapsed() < Duration::from_millis(20) && rows.len() < ids.len() {
            let mut buf = [0; 1024];
            match self.serial.read(&mut buf) {
                Ok(n) => {
                    raw.extend(&buf[..n]);
                    for f in p.feed(&buf[..n]) {
                        let id = f[2];
                        if !ids.contains(&id) || rows.contains_key(&id) || f.len() != 37 {
                            continue;
                        }
                        let mut v = decode(&f[5..36], f[4])?;
                        v["_receivedMono"] = json!(crate::telemetry::monotonic());
                        v["readMs"] = json!(start.elapsed().as_secs_f64() * 1000.);
                        v["ageMs"] = json!(0);
                        rows.insert(id, v);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e.into()),
            }
        }
        for row in rows.values_mut() {
            row["ageMs"] = json!((start.elapsed().as_secs_f64() * 1000.
                - row["readMs"].as_f64().unwrap_or(0.))
            .max(0.));
        }
        self.diagnostics = json!({"kind":"read","mono":crate::telemetry::monotonic(),"requestedIds":ids,"receivedIds":rows.keys().collect::<Vec<_>>(),"missingIds":ids.iter().filter(|id|!rows.contains_key(id)).collect::<Vec<_>>(),"elapsedMs":start.elapsed().as_secs_f64()*1000.,"rxBytes":raw.len(),"rxHex":hex(&raw),"txHex":hex(&tx),"checksumErrors":p.bad,"discardedBytes":p.discarded,"discardedBeforeRead":before});
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
    use super::*;
    use crate::telemetry::IDS;
    #[test]
    fn checksum_fragmentation() {
        let mut p = Parser::new();
        let frame = packet(24, 0, &[0; 31]);
        assert!(p.feed(&frame[..7]).is_empty());
        assert_eq!(p.feed(&frame[7..]), vec![frame]);
        assert_eq!(p.bad, 0);
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
