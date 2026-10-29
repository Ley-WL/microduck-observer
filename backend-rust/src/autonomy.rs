//! First-stage autonomy: stand by, yield to manual input, bounded recovery.
use std::time::{Duration, Instant};
use serde_json::{json, Value};
pub const RECOVERY_TIMEOUT:Duration=Duration::from_secs(10);
#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub enum Mode { #[default] Off, Standby, Manual, Recovering }
#[derive(Default)]
pub struct Autonomy {
    pub explorer:crate::exploration::Explorer,
    pub behavior:crate::exploration::behavior::Scheduler,
    pub mode: Mode,
    pub session: String,
    pub reason: String,
    pub ready: bool,
    pub epoch:u64,
    observed: Option<Instant>,
    stable_since: Option<Instant>,
    neutral_since: Option<Instant>,
    recovery_started: Option<Instant>,
    recovery_stable: Option<Instant>,
    manual_held: bool,
}
impl Autonomy {
    pub fn enabled(&self)->bool {self.mode!=Mode::Off}
    pub fn vision_active(&self)->bool {self.mode==Mode::Standby && self.behavior.observing()}
    pub fn recovering(&self)->bool {self.mode==Mode::Recovering}
    pub fn start(&mut self,session:&str,now:Instant)->anyhow::Result<()> {
        anyhow::ensure!(!self.enabled() && self.ready && self.observed.is_some_and(|at|now.saturating_duration_since(at)<Duration::from_millis(150)),"请先稳定站立、摇杆回中并等待动作结束");
        self.explorer.reset();self.behavior.reset();self.epoch=self.epoch.wrapping_add(1);self.mode=Mode::Standby;self.session=session.into();self.reason.clear();self.manual_held=false;self.neutral_since=Some(now);self.recovery_started=None;self.recovery_stable=None;if self.explorer.selected {crate::tof::exploration_capture(true);}Ok(())
    }
    pub fn stop(&mut self,reason:&str) {if self.enabled() && self.explorer.selected {crate::tof::exploration_capture(false);}self.explorer.reset();self.behavior.reset();self.epoch=self.epoch.wrapping_add(1);self.mode=Mode::Off;self.reason=reason.into();self.recovery_started=None;self.recovery_stable=None;self.neutral_since=None;self.manual_held=false;}
    pub fn input(&mut self,active:bool,now:Instant) {
        if !self.enabled() || self.recovering(){return;}
        self.manual_held=active;
        if active {if self.mode!=Mode::Manual{self.explorer.reset();self.behavior.reset();self.epoch=self.epoch.wrapping_add(1);}self.mode=Mode::Manual;self.neutral_since=None;}else{self.neutral_since.get_or_insert(now);}
    }
    pub fn action(&mut self,now:Instant){self.input(true,now);self.input(false,now);}
    pub fn rebind(&mut self,session:&str,now:Instant) {if self.enabled(){self.session=session.into();self.input(true,now);}}
    pub fn fall(&mut self,now:Instant)->bool {
        if !self.enabled(){return false;}
        if !self.recovering(){self.explorer.reset();self.behavior.reset();self.epoch=self.epoch.wrapping_add(1);self.mode=Mode::Recovering;self.recovery_started=Some(now);self.recovery_stable=None;self.reason="跌倒恢复中".into();}
        true
    }
    pub fn expired(&self,now:Instant)->bool {self.recovery_started.is_some_and(|at|now.saturating_duration_since(at)>=RECOVERY_TIMEOUT)}
    pub fn invalid_feedback(&mut self){self.explorer.pause_feedback();self.behavior.pause();self.stable_since=None;self.recovery_stable=None;self.ready=false;}
    // Returns a single request to resume zero-speed balance after recovery.
    pub fn observe(&mut self,now:Instant,kind:&str,stable:bool,recovery_pose:bool,busy:bool)->bool {
        self.observed=Some(now);
        if kind=="xgoduck" && stable && !busy {self.stable_since.get_or_insert(now);}else{self.stable_since=None;}
        self.ready=self.stable_since.is_some_and(|at|now.saturating_duration_since(at)>=Duration::from_millis(500));
        if self.recovering() {
            if self.expired(now){return false;}
            if kind=="xgoduck_getup" && recovery_pose {self.recovery_stable.get_or_insert(now);}else{self.recovery_stable=None;}
            if self.recovery_stable.is_some_and(|at|now.saturating_duration_since(at)>=Duration::from_millis(500)) {
                self.mode=Mode::Standby;self.recovery_started=None;self.recovery_stable=None;self.reason="起身成功".into();self.manual_held=false;self.neutral_since=Some(now);return true;
            }
        } else if self.mode==Mode::Manual && !self.manual_held && self.ready && self.neutral_since.is_some_and(|at|now.saturating_duration_since(at)>=Duration::from_secs(1)) {self.mode=Mode::Standby;}
        false
    }
    pub fn status(&self)->Value {json!({"mixedBehavior":self.behavior.status(),"exploration":self.explorer.status(),"selectedMode":if self.explorer.selected {"explore"}else{"interaction"},"enabled":self.enabled(),"state":match self.mode {Mode::Off=>"off",Mode::Standby=>"standby",Mode::Manual=>"manual",Mode::Recovering=>"recovering"},"ready":self.ready,"behavior":match self.mode {Mode::Off=>"已关闭",Mode::Standby=>if self.explorer.selected {self.behavior.activity.map(|a|a.name()).unwrap_or("等待站稳选择行为")}else{"原地待命"},Mode::Manual=>"人工接管",Mode::Recovering=>"限时起身"},"controlSource":if self.mode==Mode::Standby || self.recovering(){"autonomous"}else{"manual"},"recoveryTimeoutSeconds":RECOVERY_TIMEOUT.as_secs(),"recoveryRemainingSeconds":self.recovery_started.map(|at|(RECOVERY_TIMEOUT.as_secs_f64()-at.elapsed().as_secs_f64()).max(0.)),"reason":self.reason})}
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn recovery_can_finish_after_six_seconds_and_reports_ten_second_limit(){
    let(mut a,t)=ready();a.start("s",t).unwrap();a.fall(t);
    assert!(!a.expired(t+Duration::from_secs(7)));
    assert!(!a.observe(t+Duration::from_secs(7),"xgoduck_getup",true,true,false));
    assert_eq!(a.status()["recoveryTimeoutSeconds"],10);
    assert!(a.observe(t+Duration::from_millis(7500),"xgoduck_getup",true,true,false));
    assert_eq!(a.mode,Mode::Standby);assert!(a.status()["recoveryRemainingSeconds"].is_null());
 }
 #[test] fn manual_stop_and_fall_clear_mixed_activity(){
    for action in 0..3 {
        let(mut a,t)=ready();a.start("s",t).unwrap();
        a.behavior.tick(crate::exploration::behavior::Input{now:t,active:true,kind:"xgoduck",progress:0.,sit_settle:2.,ready:true,busy:false,obstacle:false,round_done:false,available:[true;3]}).unwrap();
        assert!(a.behavior.activity.is_some());
        match action {0=>a.input(true,t),1=>a.stop("用户停止"),_=>{a.fall(t);}}
        assert!(a.behavior.activity.is_none());assert!(a.behavior.head.is_none());
    }
 }
 fn ready()->(Autonomy,Instant){let mut a=Autonomy::default();let t=Instant::now();a.observe(t,"xgoduck",true,false,false);let t=t+Duration::from_millis(600);a.observe(t,"xgoduck",true,false,false);(a,t)}
 #[test] fn start_requires_fresh_stable_standing(){let(mut a,t)=ready();assert!(a.start("s",t+Duration::from_millis(151)).is_err());a.observe(t,"sitstand_sit",true,false,false);assert!(a.start("s",t).is_err());let(mut a,t)=ready();a.start("s",t).unwrap();assert_eq!(a.mode,Mode::Standby);}
 #[test] fn manual_requires_explicit_neutral_and_one_second_and_finished_skill(){let(mut a,t)=ready();a.start("s",t).unwrap();a.input(true,t);a.observe(t+Duration::from_secs(5),"xgoduck",true,false,false);assert_eq!(a.mode,Mode::Manual);a.input(false,t+Duration::from_secs(5));a.observe(t+Duration::from_millis(5999),"xgoduck",true,false,false);assert_eq!(a.mode,Mode::Manual);a.observe(t+Duration::from_secs(7),"dance10",true,false,false);assert_eq!(a.mode,Mode::Manual);a.observe(t+Duration::from_secs(8),"xgoduck",true,false,false);a.observe(t+Duration::from_millis(8500),"xgoduck",true,false,false);assert_eq!(a.mode,Mode::Standby);}
 #[test] fn recovery_deadline_is_wall_clock_never_extended_by_retries_or_feedback(){let(mut a,t)=ready();a.start("s",t).unwrap();assert!(a.fall(t));a.fall(t+Duration::from_secs(4));a.invalid_feedback();a.input(true,t+Duration::from_secs(5));assert!(a.recovering());assert!(!a.expired(t+Duration::from_millis(9999)));assert!(a.expired(t+Duration::from_secs(10)));assert!(!a.observe(t+Duration::from_secs(10),"xgoduck_getup",true,true,false));}
 #[test] fn recovery_requires_consecutive_pose_and_only_once(){let(mut a,t)=ready();a.start("s",t).unwrap();a.fall(t);a.observe(t,"xgoduck_getup",true,true,false);a.invalid_feedback();assert!(!a.observe(t+Duration::from_millis(600),"xgoduck_getup",true,true,false));assert!(a.observe(t+Duration::from_millis(1100),"xgoduck_getup",true,true,false));assert!(!a.observe(t+Duration::from_millis(1200),"xgoduck_getup",true,true,false));assert_eq!(a.mode,Mode::Standby);}
 #[test] fn explicit_stop_cannot_resume_and_takeover_preserves_mode(){let(mut a,t)=ready();a.start("s",t).unwrap();a.rebind("new",t);assert_eq!(a.session,"new");assert_eq!(a.mode,Mode::Manual);a.stop("停止");a.input(false,t);a.observe(t+Duration::from_secs(2),"xgoduck",true,true,false);assert!(!a.enabled());assert!(!a.fall(t));}

 #[test] fn action_pulse_resumes_only_after_stable_skill_completion(){let(mut a,t)=ready();a.start("s",t).unwrap();a.action(t);a.observe(t+Duration::from_secs(3),"xgoduck_pick",false,false,true);assert_eq!(a.mode,Mode::Manual);a.observe(t+Duration::from_secs(4),"xgoduck",true,false,false);a.observe(t+Duration::from_millis(4500),"xgoduck",true,false,false);assert_eq!(a.mode,Mode::Standby);}
 #[test] fn control_failure_publishes_off_state_and_never_restores_mode(){let (a,t)=ready();let mut telemetry=crate::telemetry::Telemetry::new(false);telemetry.autonomy=a;telemetry.autonomy.start("s",t).unwrap();let shared=std::sync::Arc::new(std::sync::RwLock::new(telemetry));crate::control::status(&shared,json!({"state":"failed"}));let s=shared.read().unwrap();assert!(!s.autonomy.enabled());assert_eq!(s.control["autonomy"]["state"],"off");assert!(s.autonomy.reason.contains("故障"));}
}
