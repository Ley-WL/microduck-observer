//! Roller mode has an explicit installation hold; it never falls back to walking.
use anyhow::{ensure, Result};
use serde_json::Value;

pub const PREPARE: &str = "swizzle_prepare";
pub const ACTIVE: &str = "swizzle";
/// The wire turn demand is +/-0.25; full stick slews heading at 0.5 rad/s.
/// The actor still receives a bounded heading error, never a yaw-rate target.
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
        let error=wrap(*target-yaw).clamp(-0.25,0.25);
        *target=wrap(yaw+error); // anti-windup during a blocked/slow turn
        self.turning=turning;self.stale=!fresh;
        Ok([if fresh {input[0].clamp(-0.4,0.4)}else{0.},0.,error as f32])
    }
}
pub const CROUCH: &str = "swizzle_crouch";
pub const CROUCH_PERIOD: f64 = 5.;
pub const CROUCH_END: f64 = 3.5;
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
    #[test] fn heading_tracks_drift_and_latches_after_turn_release() {
        let mut h=Heading::default();
        assert_eq!(h.step(1.,[0.4,0.,0.],true,0.02).unwrap(),[0.4,0.,0.]);
        assert!((h.step(1.1,[0.4,0.,0.],true,0.02).unwrap()[2]+0.1).abs()<1e-5);
        assert!(h.step(1.,[0.4,0.,0.25],true,0.02).unwrap()[2]>0.);
        assert_eq!(h.step(1.2,[0.4,0.,0.],true,0.02).unwrap()[2],0.);
        assert!(h.step(1.25,[0.;3],true,0.02).unwrap()[2]<0.);
    }
    #[test] fn heading_wrap_timeout_reset_and_anti_windup() {
        let mut h=Heading::default();
        h.step(3.13,[0.;3],true,0.02).unwrap();
        assert!((h.step(-3.13,[0.;3],true,0.02).unwrap()[2]+0.0231853).abs()<1e-5);
        h=Heading::default();
        for _ in 0..1000 {assert!(h.step(0.,[0.4,0.,0.25],true,0.02).unwrap()[2]<=0.25);}
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
