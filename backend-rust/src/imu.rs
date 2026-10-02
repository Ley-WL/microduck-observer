use crate::telemetry::Shared;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub struct Parser {
    buffer: Vec<u8>,
    pub bad: u64,
    pub discarded: u64,
}
impl Parser {
    pub fn new() -> Self {
        Self {
            buffer: vec![],
            bad: 0,
            discarded: 0,
        }
    }
    pub fn feed(&mut self, data: &[u8]) -> Vec<(u8, u8, Vec<u8>)> {
        self.buffer.extend(data);
        let mut out = vec![];
        while self.buffer.len() >= 4 {
            let b = &self.buffer;
            if b[0] != 0x55 || ![0x55, 0xaf].contains(&b[1]) || b[3] > 64 {
                self.buffer.remove(0);
                self.discarded += 1;
                continue;
            }
            let n = b[3] as usize + 5;
            if b.len() < n {
                break;
            }
            if b[..n - 1].iter().fold(0u8, |a, b| a.wrapping_add(*b)) != b[n - 1] {
                self.bad += 1;
                self.buffer.remove(0);
                continue;
            }
            out.push((b[1], b[2], b[4..n - 1].to_vec()));
            self.buffer.drain(..n);
        }
        out
    }
}
fn query(reg: u8) -> Vec<u8> {
    let mut p = vec![0x55, 0xaf, reg | 0x80, 1, 0];
    p.push(p.iter().fold(0u8, |a, b| a.wrapping_add(*b)));
    p
}
pub fn quaternion(p: &[u8]) -> Result<Vec<f64>> {
    if p.len() != 8 {
        bail!("Invalid quaternion length")
    }
    let w: Vec<f64> = p
        .chunks_exact(2)
        .map(|a| i16::from_le_bytes([a[0], a[1]]) as f64 / 32768.)
        .collect();
    Ok(vec![w[1], w[2], w[3], w[0]])
}
pub fn raw(p: &[u8], a: f64, g: f64) -> Result<Value> {
    if p.len() != 12 {
        bail!("Invalid raw length")
    }
    let v: Vec<f64> = p
        .chunks_exact(2)
        .map(|a| i16::from_le_bytes([a[0], a[1]]) as f64 / 32768.)
        .collect();
    Ok(
        json!({"accel":v[..3].iter().map(|x|x*a*9.80665).collect::<Vec<_>>(),"gyro":v[3..].iter().map(|x|x*g.to_radians()).collect::<Vec<_>>() }),
    )
}
pub fn spawn_reader(port: String, shared: Shared, stop: Arc<AtomicBool>) {
    std::thread::Builder::new().name("imu-reader".into()).spawn(move || {
        let began = Instant::now(); let mut errors=0u64; let mut counts=[0u64;2];
        let mut frames=std::collections::BTreeMap::<String,u64>::new(); let mut checksum=0u64;let mut unparsed=0u64;
        while !stop.load(Ordering::Acquire) {
            let result=(||->Result<()> {
                let mut reader=serialport::new(&port,115200).timeout(Duration::from_millis(20)).open()?;
                let mut parser=Parser::new();let(mut ag,mut gd)=(None,None);
                let mut queries=Instant::now()-Duration::from_secs(3);let mut last=Instant::now();let mut health=Instant::now()-Duration::from_secs(1);
                while !stop.load(Ordering::Acquire) {
                    if queries.elapsed()>Duration::from_secs(2)&&(ag.is_none()||gd.is_none()){reader.write_all(&query(3))?;reader.write_all(&query(4))?;queries=Instant::now();}
                    let mut buf=[0;4096];let n=match reader.read(&mut buf){Ok(n)=>n,Err(e)if e.kind()==std::io::ErrorKind::TimedOut=>0,Err(e)=>return Err(e.into())};
                    let now=Instant::now();let bad=parser.bad;let parsed=parser.feed(&buf[..n]);checksum+=parser.bad-bad;
                    for(head,kind,p)in parsed {
                        last=now;if head==0xaf {if p.len()==1&&p[0]<4{if kind==3{gd=Some([250.,500.,1000.,2000.][p[0]as usize]);}if kind==4{ag=Some([2.,4.,8.,16.][p[0]as usize]);}}continue;}
                        *frames.entry(format!("{kind:02x}")).or_default()+=1;
                        if kind==2 {
                            match quaternion(&p) {Ok(q)=>{counts[0]+=1;let valid=(q.iter().map(|x|x*x).sum::<f64>().sqrt()-1.).abs()<0.05;shared.write().unwrap().sample("imu.orientation",json!({"frame":"sensor","device":"MS901M","timestampBasis":"host_receive","quaternion":q,"accuracy":null,"calibrationId":null,"mountingCalibrated":false}),valid,now);},Err(_)=>unparsed+=1}
                        }else if kind==3 {
                            if let (Some(a),Some(g))=(ag,gd){match raw(&p,a,g){Ok(mut v)=>{counts[1]+=1;for(k,x)in json!({"frame":"sensor","device":"MS901M","timestampBasis":"host_receive","accelAccuracy":null,"gyroAccuracy":null,"accelUnit":"m/s²","gyroUnit":"rad/s"}).as_object().unwrap(){v[k]=x.clone();}shared.write().unwrap().sample("imu.raw",v,true,now);},Err(_)=>unparsed+=1}}
                        }
                    }
                    if health.elapsed()>=Duration::from_millis(500) {
                        let t=began.elapsed().as_secs_f64().max(0.001);shared.write().unwrap().imu=json!({"device":port,"address":null,"productId":null,"state":"unavailable","sampleAgeMs":null,"counts":{"quaternion":counts[0],"accel":counts[1],"gyro":counts[1]},"observedHz":{"quaternion":counts[0]as f64/t,"accel":counts[1]as f64/t,"gyro":counts[1]as f64/t},"ioErrors":errors,"unparsedReports":unparsed,"droppedEvents":0,"mountingCalibrated":false,"model":"MS901M","baud":115200,"ranges":{"accelG":ag,"gyroDps":gd},"checksumErrors":checksum,"frames":frames,"rangeState":if ag.is_some()&&gd.is_some(){"confirmed"}else{"unknown"}});health=now;
                    }
                    if last.elapsed()>Duration::from_secs(3){bail!("IMU 数据流中断")}
                }Ok(())
            })();
            if let Err(e)=result {errors+=1;let mut s=shared.write().unwrap();s.log("ERROR","imu",&format!("MS901M 读取失败，3秒后重试：{e}"));if s.imu.is_null(){s.imu=json!({"device":port,"address":null,"productId":null,"state":"unavailable","sampleAgeMs":null,"counts":{},"observedHz":{},"ioErrors":errors,"unparsedReports":unparsed,"droppedEvents":0,"mountingCalibrated":false,"model":"MS901M","baud":115200,"ranges":{"accelG":null,"gyroDps":null},"checksumErrors":checksum,"frames":frames,"rangeState":"unknown"});}else{s.imu["ioErrors"]=json!(errors);}}
            for _ in 0..30 {if stop.load(Ordering::Acquire){break;}std::thread::sleep(Duration::from_millis(100));}
        }
    }).expect("IMU thread");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quaternion_order() {
        assert_eq!(
            quaternion(&[0, 64, 0, 192, 0, 64, 0, 192]).unwrap(),
            vec![-0.5, 0.5, -0.5, 0.5]
        );
    }
    #[test]
    fn queries_readonly() {
        assert_eq!(query(3), vec![0x55, 0xaf, 0x83, 1, 0, 0x88]);
    }
    #[test]
    fn units() {
        let v = raw(&[0, 32, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0], 4., 2000.).unwrap();
        assert!((v["accel"][0].as_f64().unwrap() - 9.80665).abs() < 1e-8);
    }
}
