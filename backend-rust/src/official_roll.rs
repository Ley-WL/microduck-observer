//! robotd 942e58e3: complete a timed skill, refresh chaining, then hand back
//! to the gait. Hardware calibration and fault handling remain in the owner.
pub const DEFAULT_TUNING: crate::walk_profile::WalkProfile = crate::walk_profile::WalkProfile{scale:0.9,head:0.5,legs:0.7};
pub const DURATION: f64 = 1.0;
pub const CHAIN_WINDOW: f64 = 0.15;
pub const REVISION: &str = "robotd-942e58e3-timed-roll-v1";
#[derive(Default)]
pub struct Clock { chain: f64 }
#[derive(Debug,PartialEq)]
pub enum Boundary { Running, Replay, Gait }
impl Clock {
    pub fn refresh(&mut self) {self.chain=CHAIN_WINDOW;}
    pub fn pause(&mut self) {self.chain=0.;}
    pub fn boundary(&mut self,seconds:f64)->Boundary {
        if seconds<DURATION-1e-9 {return Boundary::Running;}
        let replay=self.chain>1e-9;self.chain=0.;
        if replay {Boundary::Replay}else{Boundary::Gait}
    }
    pub fn wrote(&mut self) {self.chain=(self.chain-0.02).max(0.);}
}
/// In robotd a skill with no overrides inherits the resolved gait tuning.
/// This migration resolves both gait and roulade to robotd's default tuning.
/// Archived HD1910 training metadata remains unchanged for provenance.
pub fn filter(profile:crate::walk_profile::WalkProfile,previous:&[f32;14],raw:&[f32;14],initialized:bool)->[f32;14] {
    profile.filter(previous,raw,initialized)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn release_never_cuts_short_and_expiry_precedes_next_inference() {
        let mut c=Clock::default();
        for n in 0..50 {assert_eq!(c.boundary(n as f64*0.02),Boundary::Running);c.wrote();}
        assert_eq!(c.boundary(1.),Boundary::Gait);
    }
    #[test] fn only_recent_held_requests_chain_and_feedback_pause_revokes_them() {
        let mut c=Clock::default();c.refresh();
        for _ in 0..7 {c.wrote();}assert_eq!(c.boundary(1.),Boundary::Replay);
        assert_eq!(c.boundary(1.),Boundary::Gait);
        c.refresh();for _ in 0..8 {c.wrote();}assert_eq!(c.boundary(1.),Boundary::Gait);
        c.refresh();c.pause();assert_eq!(c.boundary(1.),Boundary::Gait);
    }
    #[test] fn target_filter_keeps_raw_history_separate_and_continues_at_handoff() {
        let p=DEFAULT_TUNING;
        assert_eq!((p.scale,p.head,p.legs),(0.9,0.5,0.7));
        let raw=[-1.;14];let next=filter(p,&[1.;14],&raw,true);
        for i in 0..14 {assert!((next[i]-if (5..9).contains(&i){0.05}else{-0.33}).abs()<1e-6);}
        assert_eq!(raw,[-1.;14]);assert_eq!(filter(p,&[1.;14],&raw,false),[-0.9;14]);
    }
}
