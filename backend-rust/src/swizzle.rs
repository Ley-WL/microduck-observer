//! Roller mode has an explicit installation hold; it never falls back to walking.
use anyhow::{ensure, Result};
use serde_json::Value;

pub const PREPARE: &str = "swizzle_prepare";
pub const ACTIVE: &str = "swizzle";
pub fn is_mode(kind: &str) -> bool { matches!(kind, PREPARE | ACTIVE) }
pub fn control_mode(control: &Value) -> bool {
    control["activeSkill"].as_str().is_some_and(is_mode)
        && control["state"] != "disabled"
}
pub fn ready(control: &Value) -> bool {
    control["state"] == "policy" && control["activeSkill"] == ACTIVE
        && control["feedbackHolding"] != true
}
pub fn transition(active: &str, next: &str) -> bool {
    active == PREPARE && next == ACTIVE
}
/// Throttle and heading error, NOT m/s and rad/s. Keep the first hardware
/// trial inside the evaluated forward/backward and moderate-turn envelope.
pub fn command(body: &Value) -> Result<[f32; 3]> {
    let value = crate::orientation::vector::<3>(&body["twist"])?;
    ensure!(value.iter().all(|v| v.is_finite()) && value[0].abs() <= 0.4
        && value[1] == 0. && value[2].abs() <= 0.25, "轮滑输入须为推力±0.4、横移0、航向误差±0.25rad");
    Ok(value.map(|v| v as f32))
}
pub fn validate(kind: &str, meta: &Value) -> Result<()> {
    ensure!(is_mode(kind) && meta["runtimeKind"] == "hd1910_swizzle"
        && meta["task"] == "Mjlab-Velocity-Swizzle-MicroDuck"
        && meta["actionScale"] == 1 && meta["actionEmaOldWeight"] == 0
        && meta["voltageAdaptation"] == false && meta["targetLimits"] == "observer"
        && meta["commandEncoding"] == "throttle,zero,heading_error"
        && meta["observationNormalization"] == "embedded_in_onnx_once",
        "轮滑执行合同不匹配");
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    use serde_json::json;
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
}
