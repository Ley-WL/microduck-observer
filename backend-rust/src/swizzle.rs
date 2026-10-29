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
const STATIONARY_TURN_RATE:f64=0.9;
const GLIDING_TURN_GAIN:f64=2.;
const DRIVE_LIMIT:f64=0.4;
const DRIVE_SLEW:f64=0.8; // zero to full drive in 0.5 s
const TURN_GLIDE_START:f64=0.15_f32 as f64; // match the f32 wire boundary exactly
const TURN_RATE_BLEND_START:f64=0.2;
const STALL_ERROR:f64=0.4;
const STALL_YAW_RATE:f64=0.08;
const STALL_DWELL:f64=0.5;
const TURN_ASSIST_SECONDS:f64=1.2;
const TURN_ASSIST_SETTLE:f64=0.2;
fn smoothstep(value:f64)->f64 {let x=value.clamp(0.,1.);x*x*(3.-2.*x)}
fn turning_throttle(throttle:f64)->f64 {
    throttle.signum()*DRIVE_LIMIT*smoothstep((throttle.abs()-TURN_GLIDE_START)/(DRIVE_LIMIT-TURN_GLIDE_START))
}
fn heading_rate(throttle:f64,turn:f64)->f64 {
    if turn.abs()<=1e-6 {return 0.;}
    let blend=smoothstep((throttle.abs()-TURN_RATE_BLEND_START)/(DRIVE_LIMIT-TURN_RATE_BLEND_START));
    turn.signum()*(STATIONARY_TURN_RATE*(1.-blend)+turn.abs()*GLIDING_TURN_GAIN*blend)
}
/// Low drive plus steering prioritizes turning; larger drive blends into gliding.
/// The actor still receives +/-0.5 rad heading error, never a yaw-rate target.
#[derive(Default)]
pub struct Heading {
    pub target: Option<f64>,
    turning: bool,
    stale: bool,
    drive: f64,
    last_yaw: Option<f64>,
    last_turn: f64,
    stalled: f64,
    assist_remaining: f64,
}
fn wrap(angle: f64) -> f64 {
    (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}
impl Heading {
    pub fn assist_remaining_seconds(&self)->f64 {self.assist_remaining}
    pub fn step(&mut self, yaw: f64, input: [f32;3], fresh: bool, dt: f64) -> Result<[f32;3]> {
        ensure!(yaw.is_finite() && dt.is_finite() && dt>=0. && input.iter().all(|v|v.is_finite()), "轮滑航向输入无效");
        let turn=if fresh {input[2].clamp(-0.25,0.25) as f64}else{0.};
        let turning=turn.abs()>1e-6;
        let dt=dt.min(0.05);
        let changed=!turning || self.last_turn*turn<0.;
        if changed {self.assist_remaining=0.;}
        let still=dt>0. && self.last_yaw.is_some_and(|last|wrap(yaw-last).abs()<STALL_YAW_RATE*dt);
        let error=self.target.map_or(0.,|target|wrap(target-yaw).abs());
        self.stalled=if turning && still && error>=STALL_ERROR && self.assist_remaining<=0. && !changed {
            self.stalled+dt
        }else{0.};
        if self.stalled>=STALL_DWELL {
            self.assist_remaining=TURN_ASSIST_SECONDS;
            self.target=Some(yaw);
            self.stalled=0.;
        }
        self.last_yaw=Some(yaw);self.last_turn=turn;
        let assisting=self.assist_remaining>0.;
        let settling=self.assist_remaining>TURN_ASSIST_SECONDS-TURN_ASSIST_SETTLE;
        let input_throttle=if fresh {input[0].clamp(-0.4,0.4) as f64}else{0.};
        let desired=if assisting {0.}else if turning {turning_throttle(input_throttle)}else{input_throttle};
        // Release/stale drive is zero immediately; fresh acceleration and reversal
        // are bounded. A stalled turn briefly removes drive and retries from yaw.
        self.drive=if desired==0. {0.}else {
            self.drive+(desired-self.drive).clamp(-DRIVE_SLEW*dt,DRIVE_SLEW*dt)
        };
        let rate=if settling {0.}else if assisting {turn.signum()*STATIONARY_TURN_RATE}else{heading_rate(self.drive,turn)};
        if settling {self.target=Some(yaw);}
        self.assist_remaining=(self.assist_remaining-dt).max(0.);
        let target=self.target.get_or_insert(yaw);
        // Releasing a turn holds the current bearing, without chasing a residual
        // target. Stale input latches only once so subsequent drift is corrected.
        if (self.turning && !turning) || (!fresh && !self.stale) {*target=yaw;}
        *target=wrap(*target+rate*dt);
        let error=wrap(*target-yaw).clamp(-0.5,0.5);
        *target=wrap(yaw+error); // anti-windup during a blocked/slow turn
        self.turning=turning;self.stale=!fresh;
        Ok([self.drive as f32,0.,error as f32])
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
        assert_eq!(h.step(1.,[0.4,0.,0.],true,0.02).unwrap(),[0.016,0.,0.]);
        assert!((h.step(1.1,[0.4,0.,0.],true,0.02).unwrap()[2]+0.1).abs()<1e-5);
        assert!(h.step(1.,[0.4,0.,0.25],true,0.02).unwrap()[2]>0.);
        assert_eq!(h.step(1.2,[0.4,0.,0.],true,0.02).unwrap()[2],0.);
        assert!(h.step(1.25,[0.;3],true,0.02).unwrap()[2]<0.);
    }
    #[test] fn heading_full_training_range_preserves_slew_and_release() {
        for sign in [-1.,1.] {
            let mut h=Heading::default();
            let input=[0.4,0.,sign*0.25];
            assert!((h.step(0.,input,true,0.02).unwrap()[2]-sign*0.018).abs()<1e-6);
            for n in 1..=100 {h.step(sign as f64*n as f64*0.002,input,true,0.02).unwrap();}
            assert_eq!(h.step(sign as f64*0.202,input,true,0.02).unwrap()[2],sign*0.5);
            assert_eq!(h.step(0.2,[0.4,0.,0.],true,0.02).unwrap()[2],0.);
            assert_eq!(h.step(0.2,input,false,0.02).unwrap(),[0.;3]);
        }
    }
    #[test] fn stationary_turn_uses_fixed_rate_for_small_half_and_full_stick() {
        for sign in [-1.,1.] {for amplitude in [0.0001,0.025,0.125,0.25] {
            let mut h=Heading::default();let input=[0.,0.,sign*amplitude];
            let command=h.step(0.,input,true,0.02).unwrap();
            assert_eq!(command[0],0.);assert_eq!(command[1],0.);
            assert!((command[2]-sign*0.018).abs()<1e-6);
            for _ in 0..40 {h.step(0.,input,true,0.02).unwrap();}
            assert_eq!(h.step(0.,input,true,0.02).unwrap()[2],sign*0.5);
        }}
    }
    #[test] fn low_diagonal_prioritizes_turning_and_full_glide_stays_proportional() {
        for sign in [-1.,1.] {
            for throttle in [-0.15,-0.1,-0.0001,0.,0.0001,0.1,0.15] {
                let out=Heading::default().step(0.,[throttle,0.,sign*0.25],true,0.02).unwrap();
                assert_eq!(out[0],0.);assert!((out[2]-sign*0.018).abs()<1e-6);
            }
            let mut h=Heading::default();
            for _ in 0..25 {h.step(0.,[sign*0.4,0.,0.],true,0.02).unwrap();}
            let out=h.step(0.,[sign*0.4,0.,0.125],true,0.02).unwrap();
            assert_eq!(out[0],sign*0.4);assert!((out[2]-0.005).abs()<1e-6);
        }
    }
    #[test] fn drive_ramp_release_stale_and_reversal_are_bounded() {
        for sign in [-1.,1.] {
            let mut h=Heading::default();
            for n in 1..=25 {
                let out=h.step(0.,[sign*0.4,0.,0.],true,0.02).unwrap();
                assert!((out[0]-sign*0.016*n as f32).abs()<1e-6);
            }
            assert!((h.step(0.,[-sign*0.4,0.,0.],true,0.02).unwrap()[0]-sign*0.384).abs()<1e-6);
            assert_eq!(h.step(0.,[0.;3],true,0.).unwrap()[0],0.);
            h.step(0.,[sign*0.4,0.,0.],true,0.02).unwrap();
            assert_eq!(h.step(0.,[sign*0.4,0.,0.],false,0.02).unwrap(),[0.;3]);
            assert_eq!(h.drive,0.);
        }
    }
    #[test] fn turning_map_is_continuous_symmetric_and_bounded() {
        let mut previous=0.;
        for i in 0..=4000 {
            let input=i as f64/10000.;let mapped=turning_throttle(input);
            assert!(mapped>=previous && mapped<=0.4);
            assert!(mapped-previous<0.00025);
            assert!((turning_throttle(-input)+mapped).abs()<1e-12);
            for turn in [0.025,0.125,0.25] {
                let rate=heading_rate(mapped,turn);
                assert!(rate>=turn*2.-1e-12 && rate<=0.9+1e-12);
                assert!((heading_rate(-mapped,-turn)+rate).abs()<1e-12);
                if i>0 {assert!((rate-heading_rate(previous,turn)).abs()<0.003);}
            }
            previous=mapped;
        }
        assert_eq!(turning_throttle(0.),0.);
        assert_eq!(heading_rate(0.000001,0.25),heading_rate(0.,0.25));
    }
    #[test] fn stationary_turn_release_stale_input_and_tiny_noise_do_not_keep_slewing() {
        for fresh in [false,true] {
            let mut h=Heading::default();h.step(0.,[0.,0.,0.25],true,0.02).unwrap();
            let released=if fresh {[0.;3]}else{[0.,0.,0.25]};
            assert_eq!(h.step(0.3,released,fresh,0.02).unwrap(),[0.;3]);
            assert!((h.step(0.4,released,fresh,0.02).unwrap()[2]+0.1).abs()<1e-6);
        }
        for turn in [-0.0000001,0.,0.0000001] {
            assert_eq!(Heading::default().step(0.,[0.,0.,turn],true,0.02).unwrap(),[0.;3]);
        }
    }
    #[test] fn heading_mode_switch_reversal_and_long_cycle_keep_reference_and_dt_cap() {
        let mut h=Heading::default();
        h.step(0.,[0.,0.,0.25],true,0.02).unwrap();let first=h.target.unwrap();
        assert!((first-0.018).abs()<1e-9);
        h.step(0.,[0.4,0.,0.25],true,0.02).unwrap();
        assert!((h.target.unwrap()-first-0.018).abs()<1e-9);
        assert!((h.drive-0.016).abs()<1e-9);
        let previous=h.target.unwrap();h.step(0.,[0.,0.,-0.025],true,1.).unwrap();
        assert!((h.target.unwrap()-previous+0.045).abs()<1e-9);assert_eq!(h.drive,0.);
        let previous=h.target.unwrap();h.step(0.,[0.4,0.,0.25],true,0.).unwrap();
        assert_eq!(h.target.unwrap(),previous);assert_eq!(h.drive,0.);
        assert_eq!(h.step(0.2,[0.;3],true,0.02).unwrap(),[0.;3]);
    }
    #[test] fn persistent_stalled_turn_removes_drive_settles_and_retries() {
        for sign in [-1.,1.] {
            let mut h=Heading::default();let input=[0.25,0.,sign*0.25];
            let mut triggered=false;
            for _ in 0..150 {
                let out=h.step(0.,input,true,0.02).unwrap();
                if h.assist_remaining_seconds()>0. {
                    assert_eq!(out,[0.;3]);triggered=true;break;
                }
            }
            assert!(triggered);
            let mut retried=false;
            while h.assist_remaining_seconds()>0. {
                let out=h.step(0.,input,true,0.02).unwrap();
                assert_eq!(out[0],0.);assert!(out[2].abs()<=0.5);
                if out[2]*sign>0. {retried=true;}
            }
            assert!(retried);
            assert!(h.step(0.3,input,true,0.02).unwrap()[0]>0.);
        }
    }
    #[test] fn assist_cancels_on_release_stale_or_opposite_turn() {
        for (input,fresh) in [([0.;3],true),([0.4,0.,0.25],false),([0.4,0.,-0.25],true)] {
            let mut h=Heading::default();
            h.assist_remaining=1.1;h.last_turn=0.25;h.turning=true;h.target=Some(0.5);
            h.step(0.,input,fresh,0.02).unwrap();
            assert_eq!(h.assist_remaining_seconds(),0.);assert_eq!(h.stalled,0.);
            if !fresh || input==[0.;3] {assert_eq!(h.drive,0.);assert_eq!(h.target,Some(0.));}
        }
    }
    #[test] fn motion_or_small_error_resets_stall_dwell_and_yaw_wrap_is_shortest_path() {
        let mut h=Heading::default();
        for n in 0..300 {
            let yaw=wrap(3.13+n as f64*0.004);
            h.step(yaw,[0.25,0.,0.25],true,0.02).unwrap();
            assert_eq!(h.assist_remaining_seconds(),0.);
        }
        h.stalled=0.48;h.target=Some(0.1);h.last_yaw=Some(0.);
        h.step(0.,[0.25,0.,0.25],true,0.02).unwrap();assert_eq!(h.stalled,0.);
        h.stalled=0.48;h.target=Some(0.5);h.last_yaw=Some(0.);
        h.step(0.,[0.25,0.,0.25],true,0.).unwrap();assert_eq!(h.assist_remaining_seconds(),0.);
    }
    #[test] fn heading_wrap_timeout_reset_and_anti_windup() {
        let mut h=Heading::default();
        h.step(3.13,[0.;3],true,0.02).unwrap();
        assert!((h.step(-3.13,[0.;3],true,0.02).unwrap()[2]+0.0231853).abs()<1e-5);
        h=Heading::default();
        for n in 0..1000 {assert!(h.step(n as f64*0.002,[0.4,0.,0.25],true,0.02).unwrap()[2]<=0.5);}
        assert_eq!(h.step(2.,[0.4,0.,0.25],true,0.02).unwrap()[2],0.5);
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
