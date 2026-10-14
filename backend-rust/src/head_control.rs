//! Short-lived head offsets, composed by the existing sole UART owner.
use std::time::{Duration,Instant};
use serde_json::Value;
#[derive(Default)]
pub struct HeadIntent { input:[f64;2], updated:Option<Instant> }
pub fn ready(control:&Value)->bool {
    control["state"]=="policy" && control["activeSkill"]=="xgoduck"
        && control["runtimeProfile"]=="kissqy-sync50-v1" && control["feedbackHolding"]!=true
}
pub fn parse(body:&Value)->anyhow::Result<[f64;2]> {
    let mut input=[0.;2];
    for (i,key) in ["headPitch","headYaw"].iter().enumerate(){
        if let Some(value)=body.get(key){input[i]=value.as_f64().filter(|v|v.is_finite() && (-1. ..=1.).contains(v)).ok_or_else(||anyhow::anyhow!("头部控制量须为−1到1"))?;}
    }
    Ok(input)
}
impl HeadIntent {
    pub fn set(&mut self,input:[f64;2]){self.input=input;self.updated=Some(Instant::now());}
    pub fn clear(&mut self){self.input=[0.;2];self.updated=None;}
    pub fn current(&self,now:Instant)->[f64;2]{if self.updated.is_some_and(|at|now.saturating_duration_since(at)<Duration::from_millis(250)){self.input}else{[0.;2]}}
}
// Positive UI pitch means look up (negative mechanical pitch); yaw is positive left.
// Offset transitions run at the existing 50Hz inference cadence, 60 degrees/second.
pub fn step(offset:&mut [f64;2],input:[f64;2]) {
    for (i,target) in [-input[0]*25.,input[1]*35.].iter().enumerate(){offset[i]+=(*target-offset[i]).clamp(-1.2,1.2);}
}
#[cfg(test)] mod tests {
    use super::*;use serde_json::json;
    #[test] fn parsing_rejects_invalid_and_accepts_legacy(){
        assert_eq!(parse(&json!({})).unwrap(),[0.;2]);
        for value in [json!(2),json!(-1.1),json!("1"),Value::Null]{assert!(parse(&json!({"headYaw":value})).is_err());}
    }
    #[test] fn timeout_release_and_ramp(){
        let mut intent=HeadIntent::default();intent.set([1.,-1.]);let now=Instant::now();
        assert_eq!(intent.current(now),[1.,-1.]);assert_eq!(intent.current(now+Duration::from_millis(251)),[0.;2]);
        let mut offset=[0.;2];for _ in 0..100{step(&mut offset,[1.,-1.]);}assert_eq!(offset,[-25.,-35.]);
        step(&mut offset,[0.;2]);assert!((offset[0]+23.8).abs()<1e-8);
        for _ in 0..100{step(&mut offset,[0.;2]);}assert_eq!(offset,[0.;2]);intent.clear();assert_eq!(intent.current(now),[0.;2]);
    }
    #[test] fn only_ready_native_walk_accepts_head(){
        let mut c=json!({"state":"policy","activeSkill":"xgoduck","runtimeProfile":"kissqy-sync50-v1"});assert!(ready(&c));
        c["feedbackHolding"]=json!(true);assert!(!ready(&c));c["feedbackHolding"]=json!(false);
        for skill in ["xgoduck_getup","xgoduck_pick","xgoduck_roulade","sitstand_sit"]{c["activeSkill"]=json!(skill);assert!(!ready(&c));}
    }
}
