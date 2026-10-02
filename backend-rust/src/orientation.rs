use anyhow::{bail, Result};
use nalgebra::{Matrix3, Quaternion, UnitQuaternion, Vector3};
use serde_json::Value;
pub fn vector<const N: usize>(v: &Value) -> Result<[f64; N]> {
    let a = v
        .as_array()
        .filter(|a| a.len() == N)
        .ok_or_else(|| anyhow::anyhow!("传感器向量无效"))?;
    let mut out = [0.; N];
    for (n, x) in a.iter().enumerate() {
        out[n] = x
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| anyhow::anyhow!("传感器向量无效"))?;
    }
    Ok(out)
}
pub fn rotation(q: &Value) -> Result<Matrix3<f64>> {
    let [x, y, z, w] = vector(q)?;
    let norm = (x * x + y * y + z * z + w * w).sqrt();
    if !(0.95..1.05).contains(&norm) {
        bail!("IMU 四元数无效")
    }
    Ok(UnitQuaternion::new_normalize(Quaternion::new(w, x, y, z))
        .to_rotation_matrix()
        .into_inner())
}
pub fn imu_patch(state: &Value, pose: &str, q: &Value, boot: &str) -> Result<Value> {
    use serde_json::json;
    if !["imu-roll", "imu-pitch"].contains(&pose) {
        let mut result = json!({"quaternion":q,"bootId":boot,"time":chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),"targetQuaternion":if pose=="supine"{vec![0.,-(0.5f64).sqrt(),0.,(0.5f64).sqrt()]}else{vec![0.,0.,0.,1.]}});
        if let Some(m) = state["imu"]
            .get("mountingQuaternion")
            .filter(|v| !v.is_null())
        {
            result["mountingQuaternion"] = m.clone();
        }
        return Ok(result);
    }
    let old = &state["imu"];
    if old["bootId"] != boot
        || old["quaternion"].is_null()
        || old
            .get("targetQuaternion")
            .is_some_and(|q| q != &json!([0, 0, 0, 1]))
    {
        bail!("请先采集本次会话的躯干水平参考")
    }
    let up = Vector3::new(0., 0., 1.);
    let initial = rotation(&old["quaternion"])?.transpose() * up;
    let tilted = rotation(q)?.transpose() * up;
    let angle = initial.dot(&tilted).clamp(-1., 1.).acos();
    if angle <= 70f64.to_radians() || angle >= 110f64.to_radians() {
        bail!(
            "躯干重力倾角 {:.1}°，请从站立参考侧翻或前翻约90°；水平转向不计入倾角",
            angle.to_degrees()
        )
    }
    let axis = tilted.cross(&initial).normalize();
    let mut result = old.clone();
    let o = result
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid IMU calibration"))?;
    o.remove("validForBoot");
    o.remove("mountingQuaternion");
    let mut samples = if old["mountingSamplesMethod"] == "gravity-v1" {
        old["mountingSamples"].clone()
    } else {
        json!({})
    };
    samples[if pose == "imu-roll" { "roll" } else { "pitch" }] = json!([axis.x, axis.y, axis.z]);
    if !samples["roll"].is_null() && !samples["pitch"].is_null() {
        let x = Vector3::from(vector::<3>(&samples["roll"])?);
        let mut y = Vector3::from(vector::<3>(&samples["pitch"])?);
        let dot = x.dot(&y);
        if dot.abs() > 0.17 {
            bail!("两个姿势的转轴不垂直，请重新采集侧放和低头姿势")
        };
        y = (y - x * dot).normalize();
        let z = x.cross(&y);
        let matrix = Matrix3::from_columns(&[x, y, z]);
        let q = UnitQuaternion::from_matrix(&matrix);
        let v = q.quaternion();
        result["mountingQuaternion"] = json!([v.i, v.j, v.k, v.w]);
    }
    result["mountingSamples"] = samples;
    result["mountingSamplesMethod"] = json!("gravity-v1");
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn mounting_survives_reference() {
        let state =
            json!({"imu":{"mountingQuaternion":[0,0,0,1],"mountingSamples":{"roll":[1,0,0]}}});
        let p = imu_patch(&state, "supine", &json!([0, 0, 0, 1]), "new").unwrap();
        assert_eq!(p["mountingQuaternion"], state["imu"]["mountingQuaternion"]);
        assert!(p.get("mountingSamples").is_none());
        assert_eq!(p["bootId"], "new");
    }
    #[test]
    fn heading_is_not_tilt() {
        let state = json!({"imu":{"bootId":"b","quaternion":[0,0,0,1]}});
        assert!(imu_patch(
            &state,
            "imu-roll",
            &json!([0, 0, 0.7071067811865476, 0.7071067811865476]),
            "b"
        )
        .is_err());
    }
}
