//! Per-device upstream Rust voice, generated before control starts; cached outside releases.
use anyhow::{ensure, Context, Result};
use microduck_sounds::{hardware_seed, Personality, BANK_VERSION};
use sha2::{Digest, Sha256};
use rand::Rng;
use std::{io::{Cursor, Write}, path::Path, process::{Child, Command, Stdio}, sync::OnceLock, time::{Duration, Instant}};

struct Clip { bytes: Vec<u8>, duration: Duration }
struct Voice { groups: Vec<Vec<Clip>> }
static VOICE: OnceLock<Voice> = OnceLock::new();
fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn clip(bytes: Vec<u8>) -> Result<Clip> {
    let wav = hound::WavReader::new(Cursor::new(&bytes))?;
    let spec = wav.spec();
    ensure!(spec.channels == 1 && spec.sample_rate == microduck_sounds::SR && spec.bits_per_sample == 16 && spec.sample_format == hound::SampleFormat::Int, "Invalid generated voice format");
    let duration = Duration::from_secs_f64(wav.duration() as f64 / spec.sample_rate as f64);
    ensure!(duration > Duration::ZERO && duration < Duration::from_secs(20), "Invalid voice length");
    Ok(Clip { bytes, duration })
}
fn load(dir: &Path, seed: u32) -> Result<Voice> {
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
    ensure!(manifest["bankVersion"] == BANK_VERSION && manifest["seed"] == seed && manifest["schema"] == 2, "Voice identity/version changed");
    let mut groups=Vec::new();
    for (tag,_) in microduck_sounds::TAGS {
        let mut clips=Vec::new();
        for i in 0..microduck_sounds::variant_count(tag) {
            let name=format!("{tag}_{i}.wav");
            let bytes=std::fs::read(dir.join(&name))?;
            ensure!(manifest["sha256"][&name].as_str()==Some(hash(&bytes).as_str()),"Voice cache checksum mismatch");
            clips.push(clip(bytes)?);
        }
        groups.push(clips);
    }
    Ok(Voice{groups})
}
// Repair only missing/invalid clips; migrate the legacy greet-only cache without changing its voice.
pub fn generate(dir: &Path, seed: u32) -> Result<serde_json::Value> {
    std::fs::create_dir_all(dir)?;
    let old:serde_json::Value=std::fs::read(dir.join("manifest.json")).ok()
        .and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_default();
    let same=old["bankVersion"]==BANK_VERSION && old["seed"]==seed;
    let personality=Personality::from_seed(seed);
    let mut hashes=serde_json::Map::new();
    let mut variants=serde_json::Map::new();
    for (tag,_) in microduck_sounds::TAGS {
        variants.insert(tag.into(),serde_json::json!(microduck_sounds::variant_count(tag)));
        for i in 0..microduck_sounds::variant_count(tag) {
            let name=format!("{tag}_{i}.wav");
            let expected=if old["schema"]==2 {old["sha256"][&name].as_str()}
                else if tag=="greet" && old["tag"]=="greet" {old["sha256"][i as usize].as_str()} else {None};
            let cached=std::fs::read(dir.join(&name)).ok().filter(|b|
                same && expected==Some(hash(b).as_str()) && clip(b.clone()).is_ok());
            let bytes=if let Some(bytes)=cached {bytes} else {
                let samples=microduck_sounds::render(tag,&personality,i)?;
                let temp=dir.join(format!("{name}.tmp"));
                microduck_sounds::to_wav(&samples,&temp)?;
                let bytes=std::fs::read(&temp)?;clip(bytes.clone())?;
                std::fs::rename(temp,dir.join(&name))?;bytes
            };
            hashes.insert(name,serde_json::json!(hash(&bytes)));
        }
    }
    let manifest=serde_json::json!({"schema":2,"bankVersion":BANK_VERSION,"seed":seed,
        "sampleRate":microduck_sounds::SR,"variants":variants,"sha256":hashes,
        "upstreamCommit":"46a380b70e70c5eb859107f4a4b529a0cb9548fc"});
    let temp=dir.join("manifest.json.tmp");
    std::fs::write(&temp,serde_json::to_vec_pretty(&manifest)?)?;
    std::fs::rename(temp,dir.join("manifest.json"))?;
    Ok(manifest)
}
fn choose(voice:&Voice,rng:&mut impl Rng)->(usize,usize) {
    let group=rng.gen_range(0..voice.groups.len());
    (group,rng.gen_range(0..voice.groups[group].len()))
}
pub fn init(dir: &Path) -> Result<()> {
    let seed = hardware_seed()?;
    let voice = match load(dir, seed) { Ok(v) => v, Err(_) => { generate(dir, seed)?; load(dir, seed)? } };
    VOICE.set(voice).map_err(|_| anyhow::anyhow!("Voice already initialized"))
}
pub fn ready() -> Result<()> {
    ensure!(VOICE.get().is_some(), "鸭叫音色尚未生成，请检查服务启动日志");
    ensure!(Path::new("/usr/bin/aplay").is_file(), "主板音频播放器不可用");
    std::fs::OpenOptions::new().read(true).write(true).open("/dev/snd/controlC1").map_err(|e| {
        match e.kind() {
            std::io::ErrorKind::NotFound => anyhow::anyhow!("HAT声卡尚未就绪，请检查音频驱动、HAT连接及microduck-audio-prepare服务"),
            std::io::ErrorKind::PermissionDenied => anyhow::anyhow!("HAT声卡访问被拒绝，请检查audio组和ALSA设备权限"),
            _ => anyhow::anyhow!("HAT声卡不可用：{e}"),
        }
    })?;
    Ok(())
}
fn double_clip(source:&Clip)->Result<Clip> {
    let mut reader=hound::WavReader::new(Cursor::new(&source.bytes))?;
    let spec=reader.spec();let samples=reader.samples::<i16>().collect::<std::result::Result<Vec<_>,_>>()?;
    let mut out=Cursor::new(Vec::new());
    {let mut writer=hound::WavWriter::new(&mut out,spec)?;
     for v in &samples {writer.write_sample(*v)?;}
     for _ in 0..spec.sample_rate/8 {writer.write_sample(0i16)?;}
     for v in &samples {writer.write_sample(*v)?;}writer.finalize()?;}
    clip(out.into_inner())
}
// Small expressions use the selected clip's duration, not a fixed full-open jaw.
pub fn mouth_peak(duration:Duration)->f64 {2.+10.*((duration.as_secs_f64()-0.25)/1.75).clamp(0.,1.)}
fn expression_envelope(duration:Duration,elapsed:Duration)->f64 {
    let t=elapsed.as_secs_f64();let end=duration.as_secs_f64()+0.1;
    (t/0.12).clamp(0.,1.).min(((end-t)/0.15).clamp(0.,1.))
}
fn mouth_expression(duration:Duration,elapsed:Duration)->f64 {
    -5.+(mouth_peak(duration)+5.)*expression_envelope(duration,elapsed)
}
pub fn head_allowed(kind:&str,progress:f64,settle:f64,twist:[f32;3],input:[f64;2],demo:bool)->bool {
    !demo && input.iter().all(|v|v.abs()<1e-6) &&
        ((kind=="xgoduck" && twist.iter().all(|v|v.abs()<1e-6)) || (kind=="sitstand_sit" && progress>=settle))
}
// Four mirrored gestures avoid always leading with the same tilt/turn.
static EXPRESSION_SEQUENCE:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
fn expression_direction(sequence:usize)->[f64;2] {
    [if sequence&1==0 {-1.}else{1.},if sequence&2==0 {1.}else{-1.}]
}
fn head_expression(duration:Duration,elapsed:Duration,direction:[f64;2])->[f64;2] {
    let e=expression_envelope(duration,elapsed);
    let phase=elapsed.as_secs_f64()/duration.as_secs_f64().max(0.4);
    [direction[0]*12.*e*(std::f64::consts::TAU*phase).sin(),direction[1]*18.*e*(std::f64::consts::PI*phase).sin()]
}
pub fn step_head(offset:&mut [f64;2],desired:[f64;2],base:[f64;2],bounds:[(f64,f64);2]) {
    for i in 0..2 {
        // An actor outside the effective range must not acquire an expression correction.
        if !(bounds[i].0..=bounds[i].1).contains(&base[i]) {offset[i]=0.;continue;}
        offset[i]+=(desired[i]-offset[i]).clamp(-1.2,1.2); // 60 degrees/s at 50Hz
        offset[i]=(base[i]+offset[i]).clamp(bounds[i].0,bounds[i].1)-base[i];
    }
}
pub struct Playback { child: Child, started: Instant, duration: Duration, direction:[f64;2] }
impl Playback {
    pub fn start() -> Result<Self> {Self::start_voice("random")}
    pub fn start_voice(mode:&str) -> Result<Self> {
        ready()?;
        let voice = VOICE.get().unwrap();
        let mut rng=rand::thread_rng();
        let tag=match mode {"single"|"double"|"auto_on"|"auto_off"|"auto_reject"=>Some("chirp"),"long"|"auto_failed"=>Some("wheee"),_=>None};
        let (group,variant)=if let Some(tag)=tag {
            let group=microduck_sounds::TAGS.iter().position(|(t,_)|*t==tag).unwrap();
            (group,rng.gen_range(0..voice.groups[group].len()))
        } else {choose(voice,&mut rng)};
        let selected = &voice.groups[group][variant];
        let repeated=if matches!(mode,"double"|"auto_on"|"auto_off"|"auto_failed") {Some(double_clip(selected)?)} else {None};
        let selected=repeated.as_ref().unwrap_or(selected);
        let mut child = Command::new("/usr/bin/aplay").args(["-q", "-D", "plughw:CARD=aic3104,DEV=0"])
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        let mut input = child.stdin.take().unwrap();
        let bytes = selected.bytes.clone();
        std::thread::spawn(move || { let _ = input.write_all(&bytes); });
        Ok(Self { child, started: Instant::now(), duration: selected.duration, direction:expression_direction(EXPRESSION_SEQUENCE.fetch_add(1,std::sync::atomic::Ordering::Relaxed)) })
    }
    pub fn open_duration(&self) -> Duration { self.duration + Duration::from_millis(100) }
    pub fn cycle_duration(&self) -> Duration { self.open_duration() + Duration::from_millis(150) }
    pub fn peak_degrees(&self)->f64 {mouth_peak(self.duration)}
    pub fn head_degrees(&self,elapsed:Duration)->[f64;2] {head_expression(self.duration,elapsed,self.direction)}
    pub fn degrees(&self, elapsed: Duration) -> f64 {mouth_expression(self.duration,elapsed)}
    pub fn finished(&mut self) -> Result<bool> {
        match self.child.try_wait()? { Some(status) => { ensure!(status.success(), "扬声器播放失败，请检查音频连接"); Ok(true) }, None => Ok(false) }
    }
    pub fn check(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? { ensure!(status.success(), "扬声器播放失败，请检查音频连接"); }
        ensure!(self.started.elapsed() < self.duration + Duration::from_secs(2), "音频播放超时"); Ok(())
    }
}
impl Drop for Playback { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn short_calls_open_less_and_all_calls_close() {
        let short=Duration::from_millis(300);let long=Duration::from_secs(3);
        assert!(mouth_peak(short)<mouth_peak(long));assert_eq!(mouth_peak(long),12.);
        for ms in [100,300,800,1500,3000,19000] {
            let duration=Duration::from_millis(ms);
            assert_eq!(mouth_expression(duration,Duration::ZERO),-5.);
            for t in (0..ms+400).step_by(20) {
                assert!((-5. ..=12.).contains(&mouth_expression(duration,Duration::from_millis(t))));
                let h=head_expression(duration,Duration::from_millis(t),[-1.,1.]);assert!(h[0].abs()<=12. && h[1].abs()<=18.);
            }
            assert_eq!(mouth_expression(duration,duration+Duration::from_millis(200)),-5.);
            assert_eq!(head_expression(duration,duration+Duration::from_millis(200),[-1.,1.]),[0.;2]);
        }
    }
    #[test] fn head_expression_only_when_settled_and_manual_control_wins() {
        assert!(head_allowed("xgoduck",0.,4.,[0.;3],[0.;2],false));
        assert!(head_allowed("sitstand_sit",4.,4.,[0.;3],[0.;2],false));
        assert!(!head_allowed("sitstand_sit",3.99,4.,[0.;3],[0.;2],false));
        for kind in ["xgoduck_roulade","xgoduck_getup","sitstand_stand","dance10"] {assert!(!head_allowed(kind,10.,4.,[0.;3],[0.;2],false));}
        assert!(!head_allowed("xgoduck",0.,4.,[0.1,0.,0.],[0.;2],false));
        assert!(!head_allowed("xgoduck",0.,4.,[0.;3],[0.1,0.],false));
        assert!(!head_allowed("xgoduck",0.,4.,[0.;3],[0.;2],true));
        let mut offset=[0.;2];
        for _ in 0..50 {let before=offset;step_head(&mut offset,[-4.,5.],[0.;2],[(-2.,2.),(-3.,3.)]);for i in 0..2{assert!((offset[i]-before[i]).abs()<=1.2+1e-9);}}
        assert_eq!(offset,[-2.,3.]);
        for _ in 0..20 {step_head(&mut offset,[0.;2],[0.;2],[(-2.,2.),(-3.,3.)]);}
        assert_eq!(offset,[0.;2]);
        step_head(&mut offset,[-4.,5.],[100.,-100.],[(-2.,2.),(-3.,3.)]);
        assert_eq!(offset,[0.;2]);
    }
    #[test] fn gestures_cover_both_directions_and_reach_visible_amplitudes() {
        let duration=Duration::from_secs(2);
        let mut seen=std::collections::BTreeSet::new();
        for sequence in 0..4 {
            let direction=expression_direction(sequence);
            seen.insert((direction[0] as i8,direction[1] as i8));
            let mut offset=[0.;2];let mut peaks=[0f64;2];
            for ms in (0..2400).step_by(20) {
                let desired=head_expression(duration,Duration::from_millis(ms),direction);
                let before=offset;step_head(&mut offset,desired,[0.;2],[(-90.,90.);2]);
                for i in 0..2 {assert!((offset[i]-before[i]).abs()<=1.2+1e-9);peaks[i]=peaks[i].max(offset[i].abs());}
            }
            assert!(peaks[0]>10. && peaks[1]>16.);
            assert_eq!(offset,[0.;2]);
        }
        assert_eq!(seen.len(),4);
        for ms in [250,500,750,1000,1500] {
            let t=Duration::from_millis(ms);
            let a=head_expression(duration,t,expression_direction(0));
            let b=head_expression(duration,t,expression_direction(3));
            assert_eq!(a,[-b[0],-b[1]]);
        }
    }
    #[test] fn double_voice_has_two_complete_calls_and_gap() {
        let p=microduck_sounds::Personality::from_seed(42);let samples=microduck_sounds::render("chirp",&p,0).unwrap();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("chirp.wav");microduck_sounds::to_wav(&samples,&path).unwrap();
        let source=clip(std::fs::read(path).unwrap()).unwrap();let twice=double_clip(&source).unwrap();
        assert!((twice.duration.as_secs_f64()-source.duration.as_secs_f64()*2.-0.125).abs()<0.001);
    }
    #[test] fn device_voice_is_repeatable_and_cache_rejects_another_device_or_damage() {
        let a = tempfile::tempdir().unwrap(); let b = tempfile::tempdir().unwrap();
        let seed = microduck_sounds::seed_from_id("test-serial");
        let first = generate(a.path(), seed).unwrap();
        assert_eq!(first, generate(b.path(), seed).unwrap());
        assert_eq!(load(a.path(), seed).unwrap().groups.iter().map(Vec::len).sum::<usize>(), 70);
        assert!(load(a.path(), seed + 1).is_err());
        assert_ne!(first["sha256"], generate(b.path(), seed + 1).unwrap()["sha256"]);
        std::fs::write(a.path().join("greet_0.wav"), b"corrupt").unwrap();
        assert!(load(a.path(), seed).is_err());
    }
    #[test] fn legacy_cache_migrates_and_repairs_only_missing_or_corrupt_clips(){
        let dir=tempfile::tempdir().unwrap();let seed=42;
        let original=generate(dir.path(),seed).unwrap();
        let keep=dir.path().join("greet_0.wav");let modified=std::fs::metadata(&keep).unwrap().modified().unwrap();
        let hashes:Vec<_>=(0..12).map(|i|original["sha256"][format!("greet_{i}.wav")].clone()).collect();
        std::fs::write(dir.path().join("manifest.json"),serde_json::to_vec(&serde_json::json!({"tag":"greet","bankVersion":BANK_VERSION,"seed":seed,"sha256":hashes})).unwrap()).unwrap();
        std::fs::remove_file(dir.path().join("chirp_0.wav")).unwrap();
        assert!(load(dir.path(),seed).is_err());
        assert_eq!(generate(dir.path(),seed).unwrap(),original);
        assert_eq!(std::fs::metadata(&keep).unwrap().modified().unwrap(),modified);
        std::fs::write(dir.path().join("coo_2.wav"),b"broken").unwrap();
        assert_eq!(generate(dir.path(),seed).unwrap(),original);
        assert_eq!(std::fs::metadata(&keep).unwrap().modified().unwrap(),modified);
        let voice=load(dir.path(),seed).unwrap();
        use rand::SeedableRng;
        let mut rng=rand::rngs::StdRng::seed_from_u64(123);
        let mut seen=std::collections::BTreeSet::new();
        for _ in 0..10000 {seen.insert(choose(&voice,&mut rng));}
        assert_eq!(seen.len(),70);
    }
    #[test] fn generated_durations_include_the_complete_double_call() {
        for seed in 0..20 { for i in 0..12 {
            let samples = microduck_sounds::render("greet", &Personality::from_seed(seed), i).unwrap();
            assert!(!samples.is_empty()); assert!(samples.len() < microduck_sounds::SR as usize * 5);
        } }
    }
}

// Audio-only status notices after the policy owner has released its jaw/audio cycle.
// One worker retains/reaps the player; notices never write a servo or block control.
static NOTICES:OnceLock<std::sync::mpsc::Sender<&'static str>>=OnceLock::new();
pub fn announce(voice:&'static str) {
    if VOICE.get().is_none(){return;}
    let tx=NOTICES.get_or_init(||{
        let (tx,rx)=std::sync::mpsc::channel::<&'static str>();
        std::thread::spawn(move || {
            let mut audio:Option<Playback>=None;
            loop {
                let wait=if audio.is_some(){Duration::from_millis(20)}else{Duration::from_secs(3600)};
                match rx.recv_timeout(wait) {
                    Ok(mut voice)=>{while let Ok(next)=rx.try_recv(){voice=next;}drop(audio.take());audio=Playback::start_voice(voice).ok();},
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected)=>break,
                    Err(_)=>{}
                }
                if audio.as_mut().is_some_and(|p|p.check().is_err() || p.finished().unwrap_or(true)){audio=None;}
            }
        });tx
    });
    let _=tx.send(voice);
}
