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
pub struct Playback { child: Child, started: Instant, duration: Duration }
impl Playback {
    pub fn start() -> Result<Self> {Self::start_voice("random")}
    pub fn start_voice(mode:&str) -> Result<Self> {
        ready()?;
        let voice = VOICE.get().unwrap();
        let mut rng=rand::thread_rng();
        let tag=match mode {"single"|"double"=>Some("chirp"),"long"=>Some("wheee"),_=>None};
        let (group,variant)=if let Some(tag)=tag {
            let group=microduck_sounds::TAGS.iter().position(|(t,_)|*t==tag).unwrap();
            (group,rng.gen_range(0..voice.groups[group].len()))
        } else {choose(voice,&mut rng)};
        let selected = &voice.groups[group][variant];
        let repeated=if mode=="double" {Some(double_clip(selected)?)} else {None};
        let selected=repeated.as_ref().unwrap_or(selected);
        let mut child = Command::new("/usr/bin/aplay").args(["-q", "-D", "plughw:CARD=aic3104,DEV=0"])
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        let mut input = child.stdin.take().unwrap();
        let bytes = selected.bytes.clone();
        std::thread::spawn(move || { let _ = input.write_all(&bytes); });
        Ok(Self { child, started: Instant::now(), duration: selected.duration })
    }
    pub fn open_duration(&self) -> Duration { self.duration + Duration::from_millis(100) }
    pub fn cycle_duration(&self) -> Duration { self.open_duration() + Duration::from_millis(150) }
    pub fn degrees(&self, elapsed: Duration) -> f64 { if elapsed < self.open_duration() {30.} else {-5.} }
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
