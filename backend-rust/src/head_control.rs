//! Short-lived head offsets, composed by the existing sole UART owner.
use std::time::{Duration,Instant};
use serde_json::Value;
#[derive(Default)]
pub struct HeadIntent { input:[f64;2], updated:Option<Instant> }
pub fn ready(control:&Value)->bool {
    control["state"]=="policy" && (control["activeSkill"]=="xgoduck" ||
        (control["activeSkill"]=="sitstand_sit" && control["skillProgressSeconds"].as_f64().zip(control["sitSettleSeconds"].as_f64()).is_some_and(|(p,s)|p>=s)))
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
pub fn step(offset:&mut [f64;2],input:[f64;2],base:[f64;2],bounds:[(f64,f64);2]) {
    for (i,value) in [-input[0],input[1]].iter().enumerate(){
        let (lo,hi)=bounds[i];
        let target=if *value<0. {value.abs()*(lo-base[i])} else {value*(hi-base[i])};
        offset[i]+=(target-offset[i]).clamp(-1.2,1.2);
        offset[i]=(base[i]+offset[i]).clamp(lo,hi)-base[i];
    }
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
        let mut offset=[0.;2];for _ in 0..100{step(&mut offset,[1.,-1.],[0.;2],[(-90.,90.),(-170.,170.)]);}assert_eq!(offset[0],-90.);
        for _ in 0..100{step(&mut offset,[1.,-1.],[0.;2],[(-90.,90.),(-170.,170.)]);}assert_eq!(offset,[-90.,-170.]);
        step(&mut offset,[0.;2],[0.;2],[(-90.,90.),(-170.,170.)]);assert!((offset[0]+88.8).abs()<1e-8);
        for _ in 0..200{step(&mut offset,[0.;2],[0.;2],[(-90.,90.),(-170.,170.)]);}assert_eq!(offset,[0.;2]);intent.clear();assert_eq!(intent.current(now),[0.;2]);
    }
    #[test] fn full_stick_reaches_limits_from_nonzero_model_pose(){
        let base=[12.,-18.];
        for input in [[1.,-1.],[-1.,1.]] {
            let mut offset=[0.;2];
            for _ in 0..400 {step(&mut offset,input,base,[(-90.,90.),(-170.,170.)]);}
            assert!((base[0]+offset[0]+input[0]*90.).abs()<1e-8);
            assert!((base[1]+offset[1]-input[1]*170.).abs()<1e-8);
        }
    }
    #[test] fn reported_board_limits_remain_encodable_through_full_stick_and_model_motion(){
        for direction in [-1.,1.] {
            for hardware in [(0,4095),(1000,3800)] {
                let cal=json!({"joints":{"references":{"31":2998,"32":2300},"directions":{"31":direction,"32":direction}}});
                let bounds=[crate::control::effective_angle_limits(31,&cal,hardware).unwrap(),crate::control::effective_angle_limits(32,&cal,hardware).unwrap()];
                for input in [[1.,-1.],[-1.,1.],[0.,0.]] {
                    let mut offset=[0.;2];
                    for n in 0..500 {
                        let base=[20.+(n as f64*0.1).sin()*8.,-38.+(n as f64*0.07).sin()*8.];
                        step(&mut offset,input,base,bounds);
                        for (axis,id) in [31,32].into_iter().enumerate(){
                            let angle=base[axis]+offset[axis];
                            assert!(angle>=bounds[axis].0-1e-9 && angle<=bounds[axis].1+1e-9);
                            let goal=crate::control::target(id,0,&cal,hardware.0,hardware.1,angle.to_radians(),false).unwrap();
                            assert!((hardware.0..=hardware.1).contains(&goal));
                        }
                    }
                }
            }
        }
    }
    #[test] fn only_ready_native_walk_accepts_head(){
        let mut c=json!({"state":"policy","activeSkill":"xgoduck","runtimeProfile":"kissqy-sync50-v1"});assert!(ready(&c));
        c["feedbackHolding"]=json!(true);assert!(!ready(&c));c["feedbackHolding"]=json!(false);
        for skill in ["xgoduck_getup","xgoduck_pick","xgoduck_roulade","sitstand_sit"]{c["activeSkill"]=json!(skill);assert!(!ready(&c));}
    }
}
