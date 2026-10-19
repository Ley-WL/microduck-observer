//! External rear-hook clock. FK is a posture proxy, not a contact sensor.
use nalgebra::{Matrix3, Quaternion, UnitQuaternion, Vector3};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Stage { #[default] Idle, Enter, Hold, Exit, Settle }
#[derive(Clone, Default)]
pub struct Clock {
    pub stage: Stage, pub progress:f32, age:u32, pub held:u32, good:u32,
    pub good_max:u32, settle:u32, pub stand_run:u32, pub aborted:bool, pub cancelled:bool,
    transfer_required:bool, transfer_run:u32, transfer_passed:bool,
}
impl Clock {
    pub fn new(transfer_required:bool)->Self {Self {transfer_required,..Self::default()}}
    pub fn trigger(&mut self) {let progress=self.progress;let required=self.transfer_required;*self=Self::new(required);self.progress=progress;self.stage=Stage::Enter;}
    pub fn cancel(&mut self) {if self.stage!=Stage::Idle {self.stage=Stage::Exit;self.cancelled=true;}}
    pub fn pause(&mut self) {self.good=0;self.stand_run=0;self.transfer_run=0;}
    pub fn command(&self)->[f32;3] {[if matches!(self.stage,Stage::Enter|Stage::Hold) {1.} else {0.},self.progress,0.035]}
    pub fn done(&self)->bool {self.stage==Stage::Settle && self.settle>=100}
    pub fn step(&mut self,pose:bool,standing:bool) {
        self.step_with_transfer(pose,standing,false);
    }
    pub fn step_with_transfer(&mut self,pose:bool,standing:bool,transfer:bool) {
        let old=self.stage;
        if old==Stage::Enter && self.transfer_required && !self.transfer_passed && self.progress>=0.5-1e-5 {
            self.transfer_run=if transfer {self.transfer_run+1} else {0};
            self.transfer_passed=self.transfer_run>=15;
        }
        if old==Stage::Enter {self.age+=1;}
        let enabled=matches!(old,Stage::Enter|Stage::Hold);
        self.progress+=(f32::from(enabled)-self.progress).clamp(-0.02/1.5,0.02/1.5);
        if old==Stage::Enter && self.transfer_required && !self.transfer_passed {self.progress=self.progress.min(0.5);}
        if self.progress<1e-5 {self.progress=0.;} else if self.progress>1.-1e-5 {self.progress=1.;}
        match old {
            Stage::Enter if self.progress==1. && pose=>self.stage=Stage::Hold,
            Stage::Enter if self.age>=150=>{self.stage=Stage::Exit;self.aborted=true;},
            Stage::Hold=>{self.held+=1;self.good=if pose {self.good+1} else {0};self.good_max=self.good_max.max(self.good);if self.held>=150 {self.stage=Stage::Exit;}},
            Stage::Exit if self.progress==0.=>self.stage=Stage::Settle,
            Stage::Settle=>{self.settle+=1;self.stand_run=if standing {self.stand_run+1} else {0};},
            _=>{},
        }
    }
    pub fn status(&self)->Value {json!({"stage":format!("{:?}",self.stage),"progress":self.progress,"holdSeconds":self.held as f64*0.02,"longestPoseProxySeconds":self.good_max as f64*0.02,"standingProxySeconds":self.stand_run as f64*0.02,"transferSettleRequired":self.transfer_required,"transferQuietTicks":self.transfer_run,"transferReady":self.transfer_passed,"entryTimedOut":self.aborted,"cancelled":self.cancelled,"physicalAccepted":false,"measurement":"imu-joint-fk-proxy-v1; no contact/root velocity sensor"})}
}
fn vec(v:&Value)->Vector3<f64> {Vector3::new(v[0].as_f64().unwrap(),v[1].as_f64().unwrap(),v[2].as_f64().unwrap())}
fn rot(v:&Value)->Matrix3<f64> {UnitQuaternion::new_normalize(Quaternion::new(v[0].as_f64().unwrap(),v[1].as_f64().unwrap(),v[2].as_f64().unwrap(),v[3].as_f64().unwrap())).to_rotation_matrix().into_inner()}
struct Link {shift:Vector3<f64>, fixed:Matrix3<f64>, axis:Vector3<f64>, anchor:Vector3<f64>, index:usize}
struct Foot {chain:Vec<Link>,shift:Vector3<f64>,fixed:Matrix3<f64>,vertices:Vec<Vector3<f64>>,center:Vector3<f64>}
pub struct Geometry {geometry:[Foot;2]}
impl Default for Geometry {
    fn default()->Self {
        let data:Value=serde_json::from_str(include_str!("rear_hook_geometry.json")).unwrap();
        Self {geometry:std::array::from_fn(|side| {
            let g=&data[side];let vertices:Vec<_>=g["vertices"].as_array().unwrap().iter().map(vec).collect();
            let center=vertices.iter().copied().sum::<Vector3<f64>>()/vertices.len() as f64;
            Foot {chain:g["chain"].as_array().unwrap().iter().map(|b|Link {shift:vec(&b["pos"]),fixed:rot(&b["quat"]),axis:vec(&b["axis"]),anchor:vec(&b["anchor"]),index:b["index"].as_u64().unwrap() as usize}).collect(),shift:vec(&g["pos"]),fixed:rot(&g["quat"]),vertices,center}
        })}
    }
}
impl Geometry {
    fn transforms(&self,q:&[f64;14])->[(Vector3<f64>,Matrix3<f64>);2] {
        std::array::from_fn(|side| {
            let g=&self.geometry[side];let mut r=Matrix3::identity();let mut p=Vector3::zeros();
            for b in &g.chain {
                p+=r*b.shift;r*=b.fixed;
                let turn=UnitQuaternion::from_scaled_axis(b.axis*q[b.index]).to_rotation_matrix().into_inner();
                p+=r*(b.anchor-turn*b.anchor);r*=turn;
            }
            p+=r*g.shift;r*=g.fixed;(p,r)
        })
    }
    pub fn feet(&self,q:&[f64;14])->[Vec<Vector3<f64>>;2] {
        let transforms=self.transforms(q);
        std::array::from_fn(|side| {let (p,r)=transforms[side];self.geometry[side].vertices.iter().map(|v|p+r*v).collect()})
    }
    pub fn sample(&self,obs:&[f32],home:&[f32;14])->(bool,bool,f64,f64) {
        let (pose,standing,clearance,rear,_)=self.sample_with_transfer(obs,home);(pose,standing,clearance,rear)
    }
    pub fn sample_with_transfer(&self,obs:&[f32],home:&[f32;14])->(bool,bool,f64,f64,bool) {
        let q=std::array::from_fn(|n|(obs[6+n]+home[n]) as f64);
        let next=std::array::from_fn(|n|q[n]+obs[20+n] as f64*0.001);
        let transforms=self.transforms(&q);let moved=self.transforms(&next);
        let up=-Vector3::new(obs[3] as f64,obs[4] as f64,obs[5] as f64).normalize();
        let mut lowest=[0.;2];let mut speed2=0f64;
        for side in 0..2 {
            let (p,r)=transforms[side];let (pnext,rnext)=moved[side];
            let local_up=r.transpose()*up;let dr=rnext-r;let dp=pnext-p;
            let mut low=f64::INFINITY;
            for v in &self.geometry[side].vertices {
                low=low.min(v.dot(&local_up));speed2=speed2.max((dr*v+dp).norm_squared());
            }
            lowest[side]=p.dot(&up)+low;
        }
        let clearance=lowest[1]-lowest[0];let rear=-(transforms[1].0+transforms[1].1*self.geometry[1].center).x;
        let speed=speed2.sqrt()/0.001;
        let angular=obs[..3].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
        let tilt=up.z.clamp(-1.,1.).acos();let head=(5..9).map(|n|obs[6+n].abs()).fold(0f32,f32::max);
        let finite=obs.iter().all(|v|v.is_finite());
        (finite && clearance>0.020 && rear>0.045 && tilt<std::f64::consts::PI/6. && angular<0.4 && speed<0.02,
         finite && clearance.abs()<0.010 && tilt<std::f64::consts::PI/18. && angular<0.3 && speed<0.02 && head<std::f32::consts::PI/12.,clearance,rear,
         finite && clearance.abs()<0.010 && tilt<std::f64::consts::PI/6. && angular<0.3 && speed<0.02)
    }
    #[cfg(test)]
    fn sample_reference(&self,obs:&[f32],home:&[f32;14])->(bool,bool,f64,f64) {
        let q=std::array::from_fn(|n|(obs[6+n]+home[n]) as f64);
        let feet=self.feet(&q);let up=-Vector3::new(obs[3] as f64,obs[4] as f64,obs[5] as f64).normalize();
        let lowest=|s:usize|feet[s].iter().map(|v|v.dot(&up)).fold(f64::INFINITY,f64::min);
        let clearance=lowest(1)-lowest(0);
        let rear=-feet[1].iter().map(|v|v.x).sum::<f64>()/feet[1].len() as f64;
        let angular=obs[..3].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
        let tilt=up.z.clamp(-1.,1.).acos();
        // Finite-difference Jacobian at measured q, using measured joint rates.
        // Relative sole speed omits unobserved root translation/contact/slip.
        let next=std::array::from_fn(|n|q[n]+obs[20+n] as f64*0.001);
        let moved=self.feet(&next);
        let speed=(0..2).flat_map(|s|feet[s].iter().zip(&moved[s]).map(|(a,b)|(b-a).norm()/0.001)).fold(0f64,f64::max);
        let head=(5..9).map(|n|obs[6+n].abs()).fold(0f32,f32::max);
        let finite=obs.iter().all(|v|v.is_finite());
        (finite && clearance>0.020 && rear>0.045 && tilt<std::f64::consts::PI/6. && angular<0.4 && speed<0.02,
         finite && clearance.abs()<0.010 && tilt<std::f64::consts::PI/18. && angular<0.3 && speed<0.02 && head<std::f32::consts::PI/12.,clearance,rear)
    }
}
pub fn benchmark_fixture(path:&std::path::Path)->anyhow::Result<Value> {
    let data:Value=serde_json::from_slice(&std::fs::read(path)?)?;
    let home=crate::orientation::vector::<14>(&data["home"])?.map(|v|v as f32);
    let observations=data["observations"].as_array().ok_or_else(||anyhow::anyhow!("Missing observations"))?
        .iter().map(|v|crate::orientation::vector::<61>(v).map(|a|a.map(|x|x as f32))).collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(!observations.is_empty(),"No observations");
    let g=Geometry::default();let started=std::time::Instant::now();let mut checksum=0.;
    for _ in 0..16 {for obs in &observations {let (_,_,h,r)=g.sample(std::hint::black_box(obs),&home);checksum+=h+r;}}
    let count=observations.len()*16;let elapsed=started.elapsed().as_secs_f64()*1000.;
    Ok(json!({"hardwareAccess":false,"allMeshVerticesRetained":true,"samples":count,"totalMs":elapsed,"meanMs":elapsed/count as f64,"checksum":checksum}))
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn gated_entry_waits_for_fifteen_quiet_samples_before_lifting() {
        let mut c=Clock::new(true);c.trigger();
        for _ in 0..60 {c.step_with_transfer(false,false,false);}
        assert_eq!(c.progress,0.5);assert!(!c.transfer_passed);
        for _ in 0..14 {c.step_with_transfer(false,false,true);}
        c.pause();assert_eq!(c.transfer_run,0);
        for _ in 0..14 {c.step_with_transfer(false,false,true);}
        assert_eq!(c.progress,0.5);c.step_with_transfer(false,false,true);assert!(c.transfer_passed);
        for _ in 0..40 {c.step_with_transfer(true,false,true);}
        assert_eq!(c.stage,Stage::Hold);
        for _ in 0..150 {c.step_with_transfer(true,false,true);}
        assert_eq!(c.good_max,150);assert_eq!(c.stage,Stage::Exit);
        c.trigger();assert!(c.transfer_required);assert!(!c.transfer_passed);
    }
    #[test] fn gated_timeout_and_cancel_return_continuously() {
        let mut c=Clock::new(true);c.trigger();for _ in 0..70 {c.step(false,false);}
        c.cancel();c.step(false,false);assert!(c.progress<0.5);
        c=Clock::new(true);c.trigger();for _ in 0..150 {c.step(false,false);}
        assert!(c.aborted);assert_eq!(c.stage,Stage::Exit);assert_eq!(c.progress,0.5);
    }
    #[test] fn cached_probe_matches_full_mesh_on_actual_failed_trial() {
        let data:Value=serde_json::from_str(include_str!("../tests/fixtures/rear-native-failed-observations.json")).unwrap();
        let home=std::array::from_fn(|n|data["home"][n].as_f64().unwrap() as f32);let g=Geometry::default();
        for x in data["observations"].as_array().unwrap() {
            let obs:Vec<f32>=x.as_array().unwrap().iter().map(|v|v.as_f64().unwrap() as f32).collect();
            let a=g.sample(&obs,&home);let b=g.sample_reference(&obs,&home);
            assert_eq!((a.0,a.1),(b.0,b.1));assert!((a.2-b.2).abs()<1e-12);assert!((a.3-b.3).abs()<1e-12);
        }
    }

    #[test] fn exact_fk_matches_mujoco_recorded_poses() {
        let g=Geometry::default();let cases:Value=serde_json::from_str(include_str!("../tests/fixtures/rear-hook-fk.json")).unwrap();
        for case in cases.as_array().unwrap() {let q=std::array::from_fn(|n|case["q"][n].as_f64().unwrap());let feet=g.feet(&q);
            for s in 0..2 {for (i,p) in feet[s].iter().enumerate() {assert!((p-vec(&case["feet"][s][i])).norm()<1e-8);}}}
    }
    #[test] fn pose_gate_150_hold_samples_then_return_and_settle() {
        let mut c=Clock::default();c.trigger();for _ in 0..75 {c.step(false,false);}assert_eq!(c.stage,Stage::Enter);
        c.step(true,false);assert_eq!(c.stage,Stage::Hold);
        for _ in 0..149 {c.step(true,false);}assert_eq!(c.stage,Stage::Hold);assert_eq!(c.held,149);
        c.step(true,false);assert_eq!(c.stage,Stage::Exit);assert_eq!(c.good_max,150);
        for _ in 0..75 {c.step(false,false);}assert_eq!(c.stage,Stage::Settle);
        for _ in 0..100 {c.step(false,true);}assert!(c.done());assert_eq!(c.stand_run,100);
    }
    #[test] fn timeout_cancel_and_pause_do_not_jump_or_fabricate_stability() {
        let mut c=Clock::default();c.trigger();for _ in 0..150 {c.step(false,false);}assert!(c.aborted);assert_eq!(c.stage,Stage::Exit);
        c=Clock::default();c.trigger();for _ in 0..20 {c.step(false,false);}let p=c.progress;c.cancel();assert_eq!(c.progress,p);c.step(false,false);assert!(c.progress<p);
        c.trigger();assert!(c.progress>0.);for _ in 0..80 {c.step(true,false);}for _ in 0..40 {c.step(true,false);}let held=c.held;let p=c.progress;c.pause();assert_eq!(c.held,held);assert_eq!(c.progress,p);assert_eq!(c.good,0);
    }
}
