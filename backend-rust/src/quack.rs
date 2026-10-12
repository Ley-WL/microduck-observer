//! Per-device upstream Rust voice, generated before control starts; cached outside releases.
use anyhow::{ensure, Context, Result};
use microduck_sounds::{hardware_seed, Personality, BANK_VERSION};
use sha2::{Digest, Sha256};
use std::{io::{Cursor, Write}, path::Path, process::{Child, Command, Stdio}, sync::{atomic::{AtomicUsize, Ordering}, OnceLock}, time::{Duration, Instant}};

struct Clip { bytes: Vec<u8>, duration: Duration }
struct Voice { clips: Vec<Clip> }
static VOICE: OnceLock<Voice> = OnceLock::new();
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn clip(bytes: Vec<u8>) -> Result<Clip> {
    let wav = hound::WavReader::new(Cursor::new(&bytes))?;
    let spec = wav.spec();
    ensure!(spec.channels == 1 && spec.sample_rate == microduck_sounds::SR && spec.bits_per_sample == 16 && spec.sample_format == hound::SampleFormat::Int, "Invalid generated voice format");
    let duration = Duration::from_secs_f64(wav.duration() as f64 / spec.sample_rate as f64);
    ensure!(duration > Duration::ZERO && duration < Duration::from_secs(5), "Invalid voice length");
    Ok(Clip { bytes, duration })
}
fn load(dir: &Path, seed: u32) -> Result<Voice> {
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
    ensure!(manifest["bankVersion"] == BANK_VERSION && manifest["seed"] == seed, "Voice identity/version changed");
    let mut clips = Vec::new();
    for i in 0..microduck_sounds::variant_count("greet") {
        let bytes = std::fs::read(dir.join(format!("greet_{i}.wav")))?;
        ensure!(manifest["sha256"][i as usize].as_str() == Some(hash(&bytes).as_str()), "Voice cache checksum mismatch");
        clips.push(clip(bytes)?);
    }
    Ok(Voice { clips })
}
pub fn generate(dir: &Path, seed: u32) -> Result<serde_json::Value> {
    std::fs::create_dir_all(dir)?;
    let personality = Personality::from_seed(seed);
    let mut hashes = Vec::new();
    for i in 0..microduck_sounds::variant_count("greet") {
        let samples = microduck_sounds::render("greet", &personality, i)?;
        let temp = dir.join(format!("greet_{i}.wav.tmp"));
        microduck_sounds::to_wav(&samples, &temp)?;
        let bytes = std::fs::read(&temp)?;
        clip(bytes.clone())?;
        hashes.push(hash(&bytes));
        std::fs::rename(temp, dir.join(format!("greet_{i}.wav")))?;
    }
    let manifest = serde_json::json!({"bankVersion":BANK_VERSION,"seed":seed,"tag":"greet","sampleRate":microduck_sounds::SR,"variants":hashes.len(),"sha256":hashes,"upstreamCommit":"46a380b70e70c5eb859107f4a4b529a0cb9548fc"});
    let temp = dir.join("manifest.json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(&manifest)?)?;
    std::fs::rename(temp, dir.join("manifest.json"))?;
    Ok(manifest)
}
pub fn init(dir: &Path) -> Result<()> {
    let seed = hardware_seed()?;
    let voice = match load(dir, seed) { Ok(v) => v, Err(_) => { generate(dir, seed)?; load(dir, seed)? } };
    VOICE.set(voice).map_err(|_| anyhow::anyhow!("Voice already initialized"))
}
pub fn ready() -> Result<()> {
    ensure!(VOICE.get().is_some(), "鸭叫音色尚未生成，请检查服务启动日志");
    ensure!(Path::new("/usr/bin/aplay").is_file(), "主板音频播放器不可用");
    std::fs::OpenOptions::new().read(true).write(true).open("/dev/snd/controlC1").context("主板扬声器不可用或缺少音频权限")?;
    Ok(())
}
pub struct Playback { child: Child, started: Instant, duration: Duration }
impl Playback {
    pub fn start() -> Result<Self> {
        ready()?;
        let voice = VOICE.get().unwrap();
        let selected = &voice.clips[NEXT.fetch_add(1, Ordering::Relaxed) % voice.clips.len()];
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
    #[test] fn device_voice_is_repeatable_and_cache_rejects_another_device_or_damage() {
        let a = tempfile::tempdir().unwrap(); let b = tempfile::tempdir().unwrap();
        let seed = microduck_sounds::seed_from_id("test-serial");
        let first = generate(a.path(), seed).unwrap();
        assert_eq!(first, generate(b.path(), seed).unwrap());
        assert_eq!(load(a.path(), seed).unwrap().clips.len(), 12);
        assert!(load(a.path(), seed + 1).is_err());
        assert_ne!(first["sha256"], generate(b.path(), seed + 1).unwrap()["sha256"]);
        std::fs::write(a.path().join("greet_0.wav"), b"corrupt").unwrap();
        assert!(load(a.path(), seed).is_err());
    }
    #[test] fn generated_durations_include_the_complete_double_call() {
        for seed in 0..20 { for i in 0..12 {
            let samples = microduck_sounds::render("greet", &Personality::from_seed(seed), i).unwrap();
            assert!(!samples.is_empty()); assert!(samples.len() < microduck_sounds::SR as usize * 5);
        } }
    }
}
