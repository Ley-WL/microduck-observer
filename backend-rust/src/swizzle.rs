//! Roller mode has an explicit installation hold; it never falls back to walking.
use anyhow::{ensure, Result};
use serde_json::Value;

pub const PREPARE: &str = "swizzle_prepare";
pub const ACTIVE: &str = "swizzle";
/// A supported upright body is sufficient: limp legs need not already be HOME.
#[derive(Default)]
pub struct ResumeGate { since: Option<std::time::Instant>, last: Option<std::time::Instant> }
impl ResumeGate {
    pub fn observe(&mut self, obs: &[f32], now: std::time::Instant) -> bool {
        let contiguous=self.last.is_some_and(|last| now>=last && now.duration_since(last)<=std::time::Duration::from_millis(60));
        if !contiguous {self.since=None;}
        self.last=Some(now);
        let stable=obs.len()>=6 && obs[..6].iter().all(|v|v.is_finite()) && {
            let norm=obs[3..6].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
            let gyro=obs[..3].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
            (0.5..=1.5).contains(&norm) && (-(obs[5] as f64)/norm).clamp(-1.,1.).acos().to_degrees()<10. && gyro<0.5
        };
        if !stable {self.since=None;return false;}
        now.duration_since(*self.since.get_or_insert(now))>=std::time::Duration::from_millis(500)
    }
}
pub fn resumes_after_fall(kind:&str)->bool {matches!(kind,ACTIVE|CROUCH)}
pub fn resume_ready(control:&Value)->bool {
    control["state"]=="policy" && control["policyPhase"]=="roller-fallen-wait"
        && control["rollerResumeReady"]==true
        && control["activeSkill"].as_str().is_some_and(resumes_after_fall)
}
/// The wire turn demand is +/-0.25; full stick slews heading at 0.5 rad/s.
/// The actor receives the trained +/-0.5 rad heading error, never a yaw-rate target.
#[derive(Default)]
pub struct Heading {
    pub target: Option<f64>,
    turning: bool,
    stale: bool,
}
fn wrap(angle: f64) -> f64 {
    (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}
impl Heading {
    pub fn step(&mut self, yaw: f64, input: [f32;3], fresh: bool, dt: f64) -> Result<[f32;3]> {
        ensure!(yaw.is_finite() && dt.is_finite() && dt>=0. && input.iter().all(|v|v.is_finite()), "轮滑航向输入无效");
        let turn=if fresh {input[2].clamp(-0.25,0.25) as f64}else{0.};
        let turning=turn.abs()>1e-6;
        let target=self.target.get_or_insert(yaw);
        // Releasing a turn holds the current bearing, without chasing a residual
        // target. Stale input latches only once so subsequent drift is corrected.
        if (self.turning && !turning) || (!fresh && !self.stale) {*target=yaw;}
        *target=wrap(*target+turn*2.*dt.min(0.05));
        let error=wrap(*target-yaw).clamp(-0.5,0.5);
        *target=wrap(yaw+error); // anti-windup during a blocked/slow turn
        self.turning=turning;self.stale=!fresh;
        Ok([if fresh {input[0].clamp(-0.4,0.4)}else{0.},0.,error as f32])
    }
}
pub const CROUCH: &str = "swizzle_crouch";
pub const CROUCH_PERIOD: f64 = 5.;
pub const CROUCH_END: f64 = 3.5;
pub const CROUCH_HOLD: f64 = 2.5;
#[derive(Default)]
pub struct CrouchClock { pub returning: bool }
impl CrouchClock {
    pub fn request_return(&mut self) {self.returning=true;}
    /// Keep inferring at the end of the trained low plateau until A. Never
    /// jump phase or freeze servo targets; feedback still drives the actor.
    pub fn advance(&self, progress:f64)->f64 {
        (progress+0.02).min(if self.returning {CROUCH_END}else{CROUCH_HOLD})
    }
}
pub fn crouch_return_ready(control:&Value)->bool {
    control["state"]=="policy" && control["activeSkill"]==CROUCH
        && control["feedbackHolding"]!=true && control["rollerCrouchReturning"]!=true
}
pub fn is_mode(kind: &str) -> bool { matches!(kind, PREPARE | ACTIVE | CROUCH) }
pub fn manual_head(kind: &str) -> bool { matches!(kind, PREPARE | ACTIVE) }
pub fn crouch_command(seconds:f64)->[f32;3] {
    let phase=std::f64::consts::TAU*(seconds/CROUCH_PERIOD).clamp(0.,0.7);
    [phase.cos() as f32,phase.sin() as f32,0.]
}
// Startup idle and a failed preflight may already be physically limp.
// The UART owner still verifies all 15 real torque flags before enabling.
pub fn preparation_idle(control: &Value) -> bool {
    matches!(control["state"].as_str(),Some("idle"|"disabled"|"failed"))
}
pub fn control_mode(control: &Value) -> bool {
    control["activeSkill"].as_str().is_some_and(is_mode)
        && !preparation_idle(control)
}
pub fn ready(control: &Value) -> bool {
    control["state"] == "policy" && control["activeSkill"] == ACTIVE
        && control["feedbackHolding"] != true
}
pub fn transition(active: &str, next: &str) -> bool {
    (active == PREPARE && next == ACTIVE) || (active == ACTIVE && next == CROUCH)
}
/// Throttle and turn demand, NOT m/s or a direct model heading error. Keep the first hardware
/// trial inside the evaluated forward/backward and moderate-turn envelope.
pub fn command(body: &Value) -> Result<[f32; 3]> {
    let value = crate::orientation::vector::<3>(&body["twist"])?;
    ensure!(value.iter().all(|v| v.is_finite()) && value[0].abs() <= 0.4
        && value[1] == 0. && value[2].abs() <= 0.25, "轮滑输入须为推力±0.4、横移0、转向意图±0.25");
    Ok(value.map(|v| v as f32))
}
pub fn validate(kind: &str, meta: &Value) -> Result<()> {
    let correct=if kind==CROUCH {
        meta["runtimeKind"]=="hd1910_roller_crouch" && meta["task"]=="Mjlab-RollerCrouch-Flat-MicroDuck"
            && meta["phasePeriodSeconds"]==5 && meta["endPhase"]==0.7 && meta["durationSeconds"]==3.5
            && meta["commandEncoding"]=="cos(2*pi*phase),sin(2*pi*phase),zero"
    } else {matches!(kind,PREPARE|ACTIVE) && meta["runtimeKind"]=="hd1910_swizzle"
        && meta["task"]=="Mjlab-Velocity-Swizzle-MicroDuck" && meta["commandEncoding"]=="throttle,zero,heading_error"};
    ensure!(correct
        && meta["actionScale"] == 1 && meta["actionEmaOldWeight"] == 0
        && meta["voltageAdaptation"] == false && meta["targetLimits"] == "observer"
        && meta["observationNormalization"] == "embedded_in_onnx_once",
        "轮滑执行合同不匹配");
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;
    #[test] fn crouch_keeps_inference_phase_low_until_a_then_finishes_rise() {
        let mut clock=CrouchClock::default();let mut phase=0.;
        for _ in 0..30000 {phase=clock.advance(phase);assert!(phase<=CROUCH_HOLD);}
        assert_eq!(phase,CROUCH_HOLD);assert!(!clock.returning);
        clock.request_return();
        for _ in 0..49 {phase=clock.advance(phase);assert!(phase<CROUCH_END);}
        phase=clock.advance(phase);assert_eq!(phase,CROUCH_END);
        clock.request_return();assert_eq!(clock.advance(phase),CROUCH_END);
        let mut early=CrouchClock::default();early.request_return();
        assert_eq!(early.advance(0.2),0.22); // no phase jump, even if A arrives during descent
        assert_eq!(CrouchClock::default().advance(CROUCH_HOLD),CROUCH_HOLD);
    }
    #[test] fn fall_resume_is_only_for_running_roller_actors() {
        for kind in [ACTIVE,CROUCH] {assert!(resumes_after_fall(kind));}
        for kind in [PREPARE,"xgoduck","xgoduck_getup","sitstand_sit",""] {assert!(!resumes_after_fall(kind));}
    }
    #[test] fn supported_upright_resume_requires_half_second_of_continuous_samples() {
        use std::time::{Duration,Instant};
        let t=Instant::now();let mut gate=ResumeGate::default();
        let pose=[0.,0.,0.,0.,0.,-1.];
        for n in 0..25 {assert!(!gate.observe(&pose,t+Duration::from_millis(n*20)));}
        assert!(gate.observe(&pose,t+Duration::from_millis(500)));
        assert!(!gate.observe(&pose,t+Duration::from_millis(580)));
        assert!(!gate.observe(&pose,t));
    }
    #[test] fn tilted_moving_invalid_or_missing_samples_restart_resume_dwell() {
        use std::time::{Duration,Instant};
        let pose=[0.,0.,0.,0.,0.,-1.];let t=Instant::now();
        for bad in [vec![],vec![0.;6],vec![0.,0.,0.,1.,0.,0.],vec![0.5,0.,0.,0.,0.,-1.],vec![f32::NAN,0.,0.,0.,0.,-1.],vec![0.,0.,0.,0.,0.,f32::INFINITY]] {
            let mut gate=ResumeGate::default();
            for n in 0..25 {assert!(!gate.observe(&pose,t+Duration::from_millis(n*20)));}
            assert!(!gate.observe(&bad,t+Duration::from_millis(500)));
            assert!(!gate.observe(&pose,t+Duration::from_millis(520)));
            for n in 27..51 {assert!(!gate.observe(&pose,t+Duration::from_millis(n*20)));}
            assert!(gate.observe(&pose,t+Duration::from_millis(1020)));
        }
    }
    #[test] fn heading_tracks_drift_and_latches_after_turn_release() {
        let mut h=Heading::default();
        assert_eq!(h.step(1.,[0.4,0.,0.],true,0.02).unwrap(),[0.4,0.,0.]);
        assert!((h.step(1.1,[0.4,0.,0.],true,0.02).unwrap()[2]+0.1).abs()<1e-5);
        assert!(h.step(1.,[0.4,0.,0.25],true,0.02).unwrap()[2]>0.);
        assert_eq!(h.step(1.2,[0.4,0.,0.],true,0.02).unwrap()[2],0.);
        assert!(h.step(1.25,[0.;3],true,0.02).unwrap()[2]<0.);
    }
    #[test] fn heading_full_training_range_preserves_slew_and_release() {
        for sign in [-1.,1.] {
            let mut h=Heading::default();
            let input=[0.4,0.,sign*0.25];
            assert!((h.step(0.,input,true,0.02).unwrap()[2]-sign*0.01).abs()<1e-6);
            for _ in 0..100 {h.step(0.,input,true,0.02).unwrap();}
            assert_eq!(h.step(0.,input,true,0.02).unwrap()[2],sign*0.5);
            assert_eq!(h.step(0.2,[0.4,0.,0.],true,0.02).unwrap()[2],0.);
            assert_eq!(h.step(0.2,input,false,0.02).unwrap(),[0.;3]);
        }
    }
    #[test] fn heading_wrap_timeout_reset_and_anti_windup() {
        let mut h=Heading::default();
        h.step(3.13,[0.;3],true,0.02).unwrap();
        assert!((h.step(-3.13,[0.;3],true,0.02).unwrap()[2]+0.0231853).abs()<1e-5);
        h=Heading::default();
        for _ in 0..1000 {assert!(h.step(0.,[0.4,0.,0.25],true,0.02).unwrap()[2]<=0.5);}
        assert_eq!(h.step(0.,[0.4,0.,0.25],true,0.02).unwrap()[2],0.5);
        assert_eq!(h.step(0.4,[0.4,0.,0.25],false,0.02).unwrap(),[0.;3]);
        assert!((h.step(0.5,[0.4,0.,0.25],false,0.02).unwrap()[2]+0.1).abs()<1e-5);
        h=Heading::default();
        assert_eq!(h.step(2.,[0.;3],true,0.02).unwrap(),[0.;3]);
        assert!(h.step(f64::NAN,[0.;3],true,0.02).is_err());
        assert!(h.step(0.,[0.;3],true,-1.).is_err());
    }
    #[test] fn limp_idle_and_failed_preflight_can_retry_without_becoming_active_roller_mode() {
        for state in ["idle","disabled","failed"] {
            let control=json!({"state":state,"activeSkill":PREPARE});
            assert!(preparation_idle(&control));assert!(!control_mode(&control));
        }
        for state in ["preflight","moving","policy","holding","stopping","disabling"] {
            let control=json!({"state":state,"activeSkill":PREPARE});
            assert!(!preparation_idle(&control));assert!(control_mode(&control));
        }
        assert!(!preparation_idle(&json!({})));
    }
    #[test] fn installation_only_enters_roller_and_never_walks() {
        assert!(transition(PREPARE, ACTIVE));
        for from in [PREPARE, ACTIVE, "xgoduck", "sitstand_sit"] {
            for next in ["xgoduck", "xgoduck_getup", "sitstand_stand", "xgoduck_roulade", PREPARE] {
                assert!(!transition(from, next));
            }
        }
        assert!(!ready(&json!({"state":"policy","activeSkill":PREPARE})));
        assert!(ready(&json!({"state":"policy","activeSkill":ACTIVE})));
        assert!(!ready(&json!({"state":"policy","activeSkill":ACTIVE,"feedbackHolding":true})));
    }
    #[test] fn throttle_is_signed_and_heading_is_bounded() {
        assert_eq!(command(&json!({"twist":[-0.4,0,0.25]})).unwrap(),[-0.4,0.,0.25]);
        for twist in [json!([0.41,0,0]),json!([0,0.01,0]),json!([0,0,0.251]),json!([0,0,"NaN"])] {
            assert!(command(&json!({"twist":twist})).is_err());
        }
    }
    #[test] fn crouch_only_enters_from_skating_and_phase_is_the_official_one_shot() {
        assert!(transition(ACTIVE,CROUCH));assert!(!transition(PREPARE,CROUCH));
        assert!(!transition(CROUCH,CROUCH));assert!(!transition(CROUCH,ACTIVE));
        assert_eq!(crouch_command(0.),[1.,0.,0.]);
        let middle=crouch_command(1.25);assert!(middle[0].abs()<1e-6);assert!((middle[1]-1.).abs()<1e-6);
        assert_eq!(crouch_command(99.),crouch_command(CROUCH_END));
        assert!(!manual_head(CROUCH));assert!(manual_head(ACTIVE));
    }
}
