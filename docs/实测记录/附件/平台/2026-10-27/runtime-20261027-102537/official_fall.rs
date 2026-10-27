//! Fall reflex from robotd 942e58e3; default phases use the saved HD1910 P/D.
use anyhow::{ensure, Result};
use serde_json::Value;
use crate::gains::Gains;

#[derive(Clone,Copy,Debug,PartialEq)]
pub enum Output { Gait, Limp, Pose([f32;14]), Reset }
#[derive(Clone,Copy,Debug)]
enum Phase { Idle, Limp{since:f64,still:f64}, Pose{since:f64,from:[f32;14]} }
pub struct Reflex { phase:Phase, falling:f64, fallen:f64 }
impl Default for Reflex {fn default()->Self {Self{phase:Phase::Idle,falling:0.,fallen:0.}}}
impl Reflex {
    pub fn blind(&mut self) {
        self.falling=0.;self.fallen=0.;
        if let Phase::Limp{still,..}=&mut self.phase {*still=0.;}
    }
    pub fn active(&self)->bool {!matches!(self.phase,Phase::Idle)}
    /// `q` is the current measured position, never the preceding target.
    pub fn tick(&mut self, now:f64, fresh:bool, eligible:bool, gyro:[f32;3], g:[f32;3], q:[f32;14], home:[f32;14])->Output {
        if fresh {self.fallen=if g[2]>-0.5 {self.fallen+0.02}else{0.};}else{self.blind();}
        match self.phase {
            Phase::Idle=>{
                let rate=-(gyro[0] as f64*g[1] as f64-gyro[1] as f64*g[0] as f64);
                let falling=fresh && eligible && self.fallen<0.2-1e-9 && g[2]>-0.9 && rate>0. && g[2] as f64+rate*0.3 > -0.5;
                self.falling=if falling {self.falling+0.02}else{0.};
                if self.falling>=0.06-1e-9 {self.phase=Phase::Limp{since:now,still:0.};Output::Limp}else{Output::Gait}
            }
            Phase::Limp{since,mut still}=>{
                let rate=gyro.iter().map(|x|(*x as f64).powi(2)).sum::<f64>().sqrt();
                still=if fresh && rate<1. {still+0.02}else{0.};
                if still>=0.2-1e-9 || now-since>=1.5-1e-9 {
                    self.phase=Phase::Pose{since:now,from:q};Output::Pose(q)
                }else{self.phase=Phase::Limp{since,still};Output::Limp}
            }
            Phase::Pose{since,from}=>{
                if now-since>=0.6-1e-9 {self.phase=Phase::Idle;self.falling=0.;Output::Reset}
                else{let t=((now-since)/0.6).clamp(0.,1.) as f32;Output::Pose(std::array::from_fn(|i|from[i]+t*(home[i]-from[i])))}
            }
        }
    }
}

/// A timed HOME interpolation alone is not evidence that the body is upright.
pub fn can_resume(fresh:bool,tilt:f64,gyro:&[f32])->bool {
    fresh && tilt.is_finite() && tilt<20. && gyro.len()==3 && gyro.iter().all(|v|v.is_finite() && v.abs()<0.5)
}

pub struct Calibration {pub limp:Gains,pub pose:Gains,pub measured:bool}
impl Calibration {
    pub fn defaults(base:Gains)->Self {Self{limp:base,pose:base,measured:false}}
    pub fn status(&self)->Value {serde_json::json!({"enabled":true,"source":if self.measured {"measured"}else{"default-model-gains"},"calibrated":self.measured,"limp":{"p":self.limp.p,"d":self.limp.d},"pose":{"p":self.pose.p,"d":self.pose.d}})}
    pub fn parse(v:&Value,base:Gains,sha:&str)->Result<Self> {
        ensure!(v["schema"]=="hd1910-official-fall-v1" && v["calibrated"]==true,"跌倒增益尚未完成实机标定");
        ensure!(v["gaitPolicySha256"]==sha && crate::gains::parse(&v["baseGains"])?==base,"跌倒标定的行走模型或运行增益已改变");
        ensure!(v["evidence"].as_str().is_some_and(|s|!s.trim().is_empty()),"跌倒标定缺少实测记录");
        let limp=crate::gains::parse(&v["limp"])?;let pose=crate::gains::parse(&v["pose"])?;
        ensure!(limp.p<pose.p && pose.p<=base.p,"跌倒标定必须满足低增益P < 回姿P ≤ 行走P");
        Ok(Self{limp,pose,measured:true})
    }
    pub fn load(base:Gains,sha:&str)->Result<Option<Self>> {
        let path=std::path::PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE").unwrap_or("./state/calibration.json".into())).with_file_name("official-fall-gains.json");
        Self::from_path(&path,base,sha)
    }
    fn from_path(path:&std::path::Path,base:Gains,sha:&str)->Result<Option<Self>> {
        if !path.exists(){return Ok(Some(Self::defaults(base)));}
        Ok(Some(Self::parse(&serde_json::from_slice(&std::fs::read(path)?)?,base,sha)?))
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn fallen_or_moving_body_cannot_resume_gait_after_timed_pose(){
        assert!(can_resume(true,5.,&[0.;3]));
        for tilt in [20.,60.,84.,91.,f64::NAN]{assert!(!can_resume(true,tilt,&[0.;3]));}
        assert!(!can_resume(false,5.,&[0.;3]));assert!(!can_resume(true,5.,&[0.,0.6,0.]));
    }
    #[test] fn missing_calibration_enables_both_phases_with_saved_default_gains() {
        let temp=tempfile::tempdir().unwrap();let base=Gains{p:5,d:0};
        let config=Calibration::from_path(&temp.path().join("absent.json"),base,"policy").unwrap().unwrap();
        assert_eq!(config.limp,base);assert_eq!(config.pose,base);assert!(!config.measured);
        assert_eq!(config.status()["enabled"],true);
        assert_eq!(config.status()["source"],"default-model-gains");
    }
    fn fall(r:&mut Reflex,t:f64)->Output {r.tick(t,true,true,[0.,3.,0.],[0.565,0.,-0.825],[0.2;14],[0.;14])}
    #[test] fn predictor_debounces_and_excludes_skill_static_tilt_and_already_fallen() {
        let mut r=Reflex::default();assert_eq!(fall(&mut r,0.),Output::Gait);assert_eq!(fall(&mut r,0.02),Output::Gait);assert_eq!(fall(&mut r,0.04),Output::Limp);
        for (eligible,gyro) in [(false,[0.,3.,0.]),(true,[0.;3]),(true,[0.,-3.,0.])] {
            let mut r=Reflex::default();for n in 0..50 {assert_eq!(r.tick(n as f64*0.02,true,eligible,gyro,[0.565,0.,-0.825],[0.;14],[0.;14]),Output::Gait);}
        }
        let mut r=Reflex::default();for n in 0..10 {r.tick(n as f64*0.02,true,false,[0.;3],[1.,0.,0.],[0.;14],[0.;14]);}
        assert_eq!(r.tick(0.2,true,true,[0.,4.,0.],[1.,0.,0.],[0.;14],[0.;14]),Output::Gait);
    }
    #[test] fn landing_requires_continuous_fresh_stillness_and_pose_starts_from_measurement() {
        let mut r=Reflex::default();for n in 0..3 {fall(&mut r,n as f64*0.02);}
        for n in 1..10 {assert_eq!(r.tick(0.04+n as f64*0.02,true,true,[0.;3],[0.,0.,-1.],[0.7;14],[0.;14]),Output::Limp);}
        r.blind();
        for n in 1..10 {assert_eq!(r.tick(0.24+n as f64*0.02,true,true,[0.;3],[0.,0.,-1.],[0.7;14],[0.;14]),Output::Limp);}
        assert_eq!(r.tick(0.44,true,true,[0.;3],[0.,0.,-1.],[0.7;14],[0.;14]),Output::Pose([0.7;14]));
        if let Output::Pose(q)=r.tick(0.74,true,true,[0.;3],[0.,0.,-1.],[9.;14],[0.;14]) {assert!((q[0]-0.35).abs()<1e-6);}else{panic!("expected linear pose");}
        assert_eq!(r.tick(1.04,true,true,[0.;3],[0.,0.,-1.],[0.;14],[0.;14]),Output::Reset);assert!(!r.active());
    }
    #[test] fn moving_landing_times_out_at_1500ms_not_one_quiet_sample() {
        let mut r=Reflex::default();for n in 0..3 {fall(&mut r,n as f64*0.02);}
        assert_eq!(fall(&mut r,1.52),Output::Limp);
        assert_eq!(fall(&mut r,1.54),Output::Pose([0.2;14]));
    }
    #[test] fn gain_numbers_require_recorded_matching_calibration() {
        let base=Gains{p:5,d:0};
        let mut v=serde_json::json!({"schema":"hd1910-official-fall-v1","calibrated":true,"gaitPolicySha256":"test","baseGains":{"p":5,"d":0},"limp":{"p":1,"d":0},"pose":{"p":4,"d":0},"evidence":"test fixture only, not calibration"});
        assert!(Calibration::parse(&v,base,"test").is_ok());
        assert!(Calibration::parse(&v,base,"changed").is_err());
        v["calibrated"]=serde_json::json!(false);assert!(Calibration::parse(&v,base,"test").is_err());
    }
}
