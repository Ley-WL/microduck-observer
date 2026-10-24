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
/// Scoped over model loading, preflight, HOME and execution, including error exits.
pub struct FullRateGuard(Arc<AtomicBool>);
impl FullRateGuard {
    pub fn enter(shared: &Shared) -> Self {
        let flag = shared.read().unwrap().imu_full_rate.clone();
        flag.store(true, Ordering::Release);
        Self(flag)
    }
}
impl Drop for FullRateGuard {
    fn drop(&mut self) { self.0.store(false, Ordering::Release); }
}
type Frame = (u8, u8, Vec<u8>);
/// Preserve register replies; discard superseded streaming samples before JSON work.
fn latest_idle_frames(frames: Vec<Frame>) -> Vec<Frame> {
    let last_q = frames.iter().rposition(|(h,k,_)| *h==0x55 && *k==2);
    let last_raw = frames.iter().rposition(|(h,k,_)| *h==0x55 && *k==3);
    frames.into_iter().enumerate().filter_map(|(i,f)| {
        (f.0!=0x55 || !matches!(f.1,2|3) || Some(i)==last_q || Some(i)==last_raw).then_some(f)
    }).collect()
}
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
#[derive(Default)]
struct ReaderTiming { last_read:Option<Instant>, max_read_gap_ms:f64, last_read_ms:f64, max_read_ms:f64, max_publish_wait_ms:f64, bytes:u64 }
impl ReaderTiming {
    fn read(&mut self,at:Instant,elapsed:f64,bytes:usize){
        if let Some(last)=self.last_read {self.max_read_gap_ms=self.max_read_gap_ms.max(at.duration_since(last).as_secs_f64()*1000.);}
        self.last_read=Some(at);self.last_read_ms=elapsed;self.max_read_ms=self.max_read_ms.max(elapsed);self.bytes+=bytes as u64;
    }
    fn publish(&mut self,shared:&Shared,topic:&str,data:Value,valid:bool,received:Instant){
        let began=Instant::now();let mut state=shared.write().unwrap();
        self.max_publish_wait_ms=self.max_publish_wait_ms.max(began.elapsed().as_secs_f64()*1000.);
        state.sample(topic,data,valid,received);
    }
    fn value(&self)->Value{json!({"maxReadGapMs":self.max_read_gap_ms,"lastSerialReadMs":self.last_read_ms,"maxSerialReadMs":self.max_read_ms,"maxPublishLockWaitMs":self.max_publish_wait_ms,"receivedBytes":self.bytes,"timestampBasis":"host_receive"})}
}
pub fn spawn_reader(port: String, shared: Shared, stop: Arc<AtomicBool>) {
    let full_rate = shared.read().unwrap().imu_full_rate.clone();
    std::thread::Builder::new().name("imu-reader".into()).spawn(move || {
        let began = Instant::now(); let mut errors=0u64; let mut counts=[0u64;2];
        let mut timing=ReaderTiming::default();
        let mut frames=std::collections::BTreeMap::<String,u64>::new(); let mut checksum=0u64;let mut unparsed=0u64;
        while !stop.load(Ordering::Acquire) {
            let result=(||->Result<()> {
                let mut reader=serialport::new(&port,115200).timeout(Duration::from_millis(20)).open()?;
                let mut parser=Parser::new();let(mut ag,mut gd)=(None,None);
                let mut fast=full_rate.load(Ordering::Acquire);
                let mut next_read=Instant::now();
                let mut queries=Instant::now()-Duration::from_secs(3);let mut last=Instant::now();let mut health=Instant::now()-Duration::from_secs(1);
                while !stop.load(Ordering::Acquire) {
                    // Read batches at 15Hz while idle. Check the model flag during
                    // the wait so starting a model does not wait a whole idle tick.
                    while !fast && Instant::now()<next_read && !stop.load(Ordering::Acquire) {
                        if full_rate.load(Ordering::Acquire) { break; }
                        std::thread::sleep(next_read.saturating_duration_since(Instant::now()).min(Duration::from_millis(5)));
                    }
                    if stop.load(Ordering::Acquire) { break; }
                    let requested=full_rate.load(Ordering::Acquire);
                    if requested && !fast {
                        // Never present idle-buffered bytes as a new full-rate sample.
                        reader.clear(serialport::ClearBuffer::Input)?;
                        parser=Parser::new();
                    }
                    fast=requested;
                    next_read=Instant::now()+crate::control::IDLE_FEEDBACK_PERIOD;
                    if queries.elapsed()>Duration::from_secs(2)&&(ag.is_none()||gd.is_none()){reader.write_all(&query(3))?;reader.write_all(&query(4))?;queries=Instant::now();}
                    let read_start=Instant::now();let mut buf=[0;4096];let n=match reader.read(&mut buf){Ok(n)=>n,Err(e)if e.kind()==std::io::ErrorKind::TimedOut=>0,Err(e)=>return Err(e.into())};
                    let now=Instant::now();timing.read(now,read_start.elapsed().as_secs_f64()*1000.,n);let bad=parser.bad;let parsed=parser.feed(&buf[..n]);checksum+=parser.bad-bad;
                    let parsed=if fast {parsed}else{latest_idle_frames(parsed)};
                    for(head,kind,p)in parsed {
                        last=now;if head==0xaf {if p.len()==1&&p[0]<4{if kind==3{gd=Some([250.,500.,1000.,2000.][p[0]as usize]);}if kind==4{ag=Some([2.,4.,8.,16.][p[0]as usize]);}}continue;}
                        *frames.entry(format!("{kind:02x}")).or_default()+=1;
                        if kind==2 {
                            match quaternion(&p) {Ok(q)=>{counts[0]+=1;let valid=(q.iter().map(|x|x*x).sum::<f64>().sqrt()-1.).abs()<0.05;timing.publish(&shared,"imu.orientation",json!({"frame":"sensor","device":"MS901M","timestampBasis":"host_receive","quaternion":q,"accuracy":null,"calibrationId":null,"mountingCalibrated":false}),valid,now);},Err(_)=>unparsed+=1}
                        }else if kind==3 {
                            if let (Some(a),Some(g))=(ag,gd){match raw(&p,a,g){Ok(mut v)=>{counts[1]+=1;for(k,x)in json!({"frame":"sensor","device":"MS901M","timestampBasis":"host_receive","accelAccuracy":null,"gyroAccuracy":null,"accelUnit":"m/s²","gyroUnit":"rad/s"}).as_object().unwrap(){v[k]=x.clone();}timing.publish(&shared,"imu.raw",v,true,now);},Err(_)=>unparsed+=1}}
                        }
                    }
                    if health.elapsed()>=Duration::from_millis(500) {
                        let t=began.elapsed().as_secs_f64().max(0.001);shared.write().unwrap().imu=json!({"device":port,"address":null,"productId":null,"state":"unavailable","sampleAgeMs":null,"counts":{"quaternion":counts[0],"accel":counts[1],"gyro":counts[1]},"observedHz":{"quaternion":counts[0]as f64/t,"accel":counts[1]as f64/t,"gyro":counts[1]as f64/t},"ioErrors":errors,"unparsedReports":unparsed,"droppedEvents":0,"mountingCalibrated":false,"model":"MS901M","baud":115200,"processingTargetHz":if fast {200}else{15},"rateMode":if fast {"model"}else{"idle"},"sensorRateChanged":false,"ranges":{"accelG":ag,"gyroDps":gd},"checksumErrors":checksum,"frames":frames,"readerTiming":timing.value(),"rangeState":if ag.is_some()&&gd.is_some(){"confirmed"}else{"unknown"}});health=now;
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
    fn idle_batch_keeps_latest_each_topic_and_register_replies() {
        let frames=vec![(0x55,2,vec![1]),(0xaf,3,vec![3]),
            (0x55,3,vec![2]),(0x55,2,vec![4]),(0x55,3,vec![5])];
        assert_eq!(latest_idle_frames(frames),vec![(0xaf,3,vec![3]),
            (0x55,2,vec![4]),(0x55,3,vec![5])]);
    }
    #[test]
    fn model_rate_restored_on_success_and_preflight_error() {
        let shared=Arc::new(std::sync::RwLock::new(crate::telemetry::Telemetry::new(true)));
        let flag=shared.read().unwrap().imu_full_rate.clone();
        assert!(!flag.load(Ordering::Acquire));
        let attempt=|fail:bool| -> Result<()> {
            let _guard=FullRateGuard::enter(&shared);
            assert!(flag.load(Ordering::Acquire));
            if fail {bail!("preflight rejected")}
            Ok(())
        };
        assert!(attempt(false).is_ok());assert!(!flag.load(Ordering::Acquire));
        assert!(attempt(true).is_err());assert!(!flag.load(Ordering::Acquire));
    }
    #[cfg(target_os="linux")]
    #[test]
    fn serial_stream_switches_idle_model_idle_without_replaying_backlog() {
        use serialport::SerialPort;
        let (mut master,slave)=serialport::TTYPort::pair().unwrap();
        let path=slave.name().unwrap();
        let shared=Arc::new(std::sync::RwLock::new(crate::telemetry::Telemetry::new(true)));
        let stop=Arc::new(AtomicBool::new(false));
        let writer_stop=stop.clone();
        let writer=std::thread::spawn(move || {
            let mut frame=vec![0x55,0x55,2,8,0,64,0,0,0,0,0,0];
            frame.push(frame.iter().fold(0u8,|a,b|a.wrapping_add(*b)));
            while !writer_stop.load(Ordering::Acquire) {
                if master.write_all(&frame).is_err() {break;}
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        spawn_reader(path,shared.clone(),stop.clone());
        let seq=||shared.read().unwrap().latest.get("imu.orientation")
            .and_then(|v|v["seq"].as_u64()).unwrap_or(0);
        std::thread::sleep(Duration::from_millis(600));
        let idle=seq();
        let guard=FullRateGuard::enter(&shared);
        std::thread::sleep(Duration::from_millis(400));
        let active=seq()-idle;
        drop(guard);
        std::thread::sleep(Duration::from_millis(100));
        let begin=seq();
        std::thread::sleep(Duration::from_millis(400));
        let idle_again=seq()-begin;
        stop.store(true,Ordering::Release);writer.join().unwrap();
        assert!((3..=11).contains(&idle),"idle samples: {idle}");
        assert!(active>=20,"model samples: {active}");
        assert!((2..=8).contains(&idle_again),"idle again samples: {idle_again}");
    }
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
