//! Explicit donor execution profile; only the selected walking model opts in.
use anyhow::{ensure, Result};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub struct WalkProfile { pub scale: f32, pub head: f32, pub legs: f32 }
impl WalkProfile {
    pub fn parse(kind: &str, meta: &Value) -> Result<Option<Self>> {
        let v=&meta["walkExecution"];
        if v.is_null() { return Ok(None); }
        ensure!(kind=="xgoduck" && v["schema"]=="kissqy-r17-v1" && v["policySha256"]==meta["policySha256"], "行走执行合同或哈希不匹配");
        let number=|key:&str| -> Result<f32> { let x=v[key].as_f64().unwrap_or(f64::NAN);ensure!(x.is_finite() && x>0. && x<=1.,"行走执行系数无效");Ok(x as f32) };
        let _archived_head_weight=number("headNewWeight")?;
        Ok(Some(Self {scale:number("actionScale")?,head:1.,legs:number("legsNewWeight")?}))
    }
    /// Preserve leg filtering; #30–#33 always use this frame, including legacy profiles.
    pub fn filter(&self, previous:&[f32;14], raw:&[f32;14], initialized:bool)->[f32;14] {
        std::array::from_fn(|n| {let target=self.scale*raw[n];if (5..9).contains(&n) || !initialized {target}else{(1.-self.legs)*previous[n]+self.legs*target}})
    }
}
/// Donor robotd cmd_alpha=0.2 at 50 Hz; stale intent stops immediately.
pub fn smooth_twist(previous: &mut [f32;3], target:[f32;3], fresh:bool)->[f32;3] {
    if !fresh {*previous=[0.;3];return *previous;}
    for (value,next) in previous.iter_mut().zip(target) {*value+=0.2*(next-*value);}
    *previous
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn donor_target_sequence_preserves_raw_history_and_home_offset() {
        let p=WalkProfile{scale:0.9,head:0.5,legs:0.7};
        let first=p.filter(&[0.;14],&[1.;14],false);assert_eq!(first,[0.9;14]);
        let next=p.filter(&first,&[-1.;14],true);
        for i in 0..14 {assert!((next[i]-if (5..9).contains(&i){-0.9}else{-0.36}).abs()<1e-6);}
    }
    #[test] fn reverse_and_turn_slew_match_donor_and_stale_intent_stops() {
        let mut command=[0.;3];
        let first=smooth_twist(&mut command,[-0.4,0.,0.5],true);
        assert!((first[0]+0.08).abs()<1e-7 && (first[2]-0.1).abs()<1e-7);
        for _ in 0..60 {smooth_twist(&mut command,[-0.4,0.,0.5],true);}
        assert!((command[0]+0.4).abs()<1e-6 && (command[2]-0.5).abs()<1e-6);
        assert_eq!(smooth_twist(&mut command,[-0.4,0.,0.5],false),[0.;3]);
    }
    #[test] fn contract_cannot_leak_to_other_skills_or_other_weights() {
        let mut m=serde_json::json!({"policySha256":"a","walkExecution":{"schema":"kissqy-r17-v1","policySha256":"a","actionScale":0.9,"headNewWeight":0.5,"legsNewWeight":0.7,"gains":{"p":3,"d":0}}});
        assert_eq!(WalkProfile::parse("xgoduck",&m).unwrap().unwrap().head,1.);
        assert!(WalkProfile::parse("sitstand_sit",&m).is_err());m["policySha256"]=serde_json::json!("b");assert!(WalkProfile::parse("xgoduck",&m).is_err());
        assert!(WalkProfile::parse("xgoduck",&serde_json::json!({})).unwrap().is_none());
    }
}
