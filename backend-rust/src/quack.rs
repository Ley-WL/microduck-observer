//! One server-owned mouth cycle with bounded, nonblocking ALSA playback.
use anyhow::{Result,ensure};
use std::{io::Write,process::{Child,Command,Stdio},time::{Duration,Instant}};
pub const DURATION:Duration=Duration::from_millis(700);
pub fn degrees(elapsed:Duration)->f64 {if elapsed<Duration::from_millis(550) {30.} else {-5.}}
pub fn ready()->Result<()> {
    ensure!(std::path::Path::new("/usr/bin/aplay").is_file(),"主板音频播放器不可用");
    std::fs::OpenOptions::new().read(true).write(true).open("/dev/snd/controlC1")
        .map_err(|_|anyhow::anyhow!("主板扬声器不可用或缺少音频权限"))?;
    Ok(())
}
pub struct Playback {child:Child,started:Instant}
impl Playback {
    pub fn start()->Result<Self> {
        ready()?;
        let mut child=Command::new("/usr/bin/aplay").args(["-q","-D","plughw:CARD=aic3104,DEV=0"])
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        let mut input=child.stdin.take().unwrap();
        std::thread::spawn(move||{let _=input.write_all(include_bytes!("../assets/quack.wav"));});
        Ok(Self{child,started:Instant::now()})
    }
    pub fn check(&mut self)->Result<()> {
        if let Some(status)=self.child.try_wait()? {ensure!(status.success(),"扬声器播放失败，请检查音频连接");}
        ensure!(self.started.elapsed()<Duration::from_secs(2),"音频播放超时");Ok(())
    }
}
impl Drop for Playback {fn drop(&mut self){let _=self.child.kill();let _=self.child.wait();}}
#[cfg(test)] mod tests {use super::*;
 #[test] fn one_cycle_closes_and_stays_closed(){assert_eq!(degrees(Duration::ZERO),30.);assert_eq!(degrees(Duration::from_millis(549)),30.);assert_eq!(degrees(Duration::from_millis(550)),-5.);assert_eq!(degrees(DURATION),-5.);}
 #[test] fn sound_is_bounded_pcm(){let b=include_bytes!("../assets/quack.wav");assert_eq!(&b[..4],b"RIFF");assert_eq!(&b[8..12],b"WAVE");assert!(b.len()<30000);}
}
