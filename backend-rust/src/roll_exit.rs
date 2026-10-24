//! Local completion guard for the donor roll, not a change to its actor contract.
//! Gravity winding observes rotation without integrating IMU bias. No contact
//! sensor is available: this is an exit guard, not proof of a supported roll.
#[derive(Default)]
pub struct RollExit {
    previous: Option<f64>,
    pub rotation_degrees: f64,
    pub inverted: bool,
    stable: f64,
}
impl RollExit {
    pub const TIMEOUT: f64 = 2.0;
    pub fn reset(&mut self) { *self=Self::default(); }
    pub fn sample(&mut self, obs:&[f32]) {
        let g=[obs[3] as f64,obs[4] as f64,obs[5] as f64];
        let norm=g.iter().map(|v|v*v).sum::<f64>().sqrt();
        // A lateral roll or a discontinuous sensor sample cannot certify a
        // forward roll. Do not bridge missing samples or an ambiguous pole.
        if !norm.is_finite() || norm<0.5 || g[1].abs()/norm>35f64.to_radians().sin() {
            self.reset();return;
        }
        let phase=g[0].atan2(-g[2]);
        if let Some(previous)=self.previous {
            let delta=(phase-previous+std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)-std::f64::consts::PI;
            if delta.abs()>90f64.to_radians() {self.reset();self.previous=Some(phase);return;}
            self.rotation_degrees+=delta.to_degrees();
        }
        self.previous=Some(phase);
        self.inverted|=g[2]/norm>0.5;
        let tilt=(-g[2]/norm).clamp(-1.,1.).acos().to_degrees();
        let gyro=obs[..3].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
        // Positive gravity winding matches the donor's sagittal forward roll.
        if self.inverted && self.rotation_degrees>=330. && tilt<15. && gyro<1.5 {
            self.stable+=0.02;
        } else {self.stable=0.;}
    }
    pub fn complete(&self,seconds:f64)->bool {seconds>=1.-1e-9 && self.stable>=0.2-1e-9}
}
#[cfg(test)]
mod tests {
    use super::*;
    fn obs(degrees:f64,gyro:f32)->Vec<f32>{let mut o=vec![0.;61];o[1]=gyro;o[3]=degrees.to_radians().sin() as f32;o[5]=-degrees.to_radians().cos() as f32;o}
    fn turn(r:&mut RollExit){for n in 0..=72 {r.sample(&obs(n as f64*5.,1.));}}
    #[test] fn recorded_landing_exits_before_five_second_timeout() {
        let frames:Vec<Vec<f64>>=serde_json::from_str(include_str!("../tests/fixtures/roll-landing-23.json")).unwrap();
        let mut guard=RollExit::default();let mut first=None;
        for frame in frames {
            let observation:Vec<f32>=frame[1..].iter().map(|v|*v as f32).collect();
            guard.sample(&observation);
            if guard.complete(frame[0]) {first=Some(frame[0]);break;}
        }
        assert!((first.expect("recorded landing must complete")-3.74).abs()<0.001);
    }
    #[test] fn mid_roll_and_simple_recovery_do_not_complete(){
        let mut r=RollExit::default();for n in 0..=18 {r.sample(&obs(n as f64*5.,1.));}assert!(!r.complete(1.));
        for n in (0..18).rev(){r.sample(&obs(n as f64*5.,1.));}for _ in 0..30{r.sample(&obs(0.,0.));}assert!(!r.complete(1.8));
    }
    #[test] fn completed_turn_needs_stable_landing_and_minimum_window(){let mut r=RollExit::default();turn(&mut r);assert!(!r.complete(1.5));for _ in 0..9{r.sample(&obs(360.,0.));}assert!(r.complete(1.7));assert!(!r.complete(0.9));}
    #[test] fn fast_upright_landing_is_not_stable(){let mut r=RollExit::default();turn(&mut r);for _ in 0..30{r.sample(&obs(360.,4.));}assert!(!r.complete(1.8));}
    #[test] fn backward_turn_does_not_certify_forward_roll(){let mut r=RollExit::default();for n in 0..=72{r.sample(&obs(-5.*n as f64,1.));}for _ in 0..30{r.sample(&obs(-360.,0.));}assert!(!r.complete(1.8));}
    #[test] fn pause_new_episode_and_lateral_motion_clear_winding(){let mut r=RollExit::default();turn(&mut r);r.reset();for _ in 0..30{r.sample(&obs(0.,0.));}assert!(!r.complete(2.));turn(&mut r);let mut o=obs(0.,0.);o[4]=1.;r.sample(&o);assert!(!r.inverted);assert_eq!(r.rotation_degrees,0.);}
}

/// First back-down attitude after leaving upright; extend for 0.5s, then getup.
/// IMU attitude is not a contact measurement.
#[derive(Default)]
pub struct RollToGetup {
    upright_seen: bool,
    back_samples: u8,
    last: Option<[f32;14]>,
    bridge: Option<(f64,[f32;14])>,
}
impl RollToGetup {
    pub const MIN_SECONDS:f64=0.2;
    pub const EXTEND_SECONDS:f64=0.5;
    pub const TIMEOUT:f64=2.;
    pub fn capture(&mut self,_seconds:f64,action:[f32;14]) {
        if self.bridge.is_none() && action.iter().all(|v|v.is_finite()) {self.last=Some(action);}
    }
    pub fn phase(&self,_seconds:f64)->&'static str {
        if self.bridge.is_none() {"rolling"}else{"extending"}
    }
    pub fn just_started(&self,seconds:f64)->bool {self.bridge.is_some_and(|(start,_)|(seconds-start).abs()<1e-9)}
    pub fn target(&self,seconds:f64)->Option<[f32;14]> {
        let (start,mut target)=self.bridge?;
        let t=((seconds-start+0.02)/Self::EXTEND_SECONDS).clamp(0.,1.) as f32;
        // Twenty-five 50Hz commands reach HOME before getup.
        // Last sent target, not the early roll action; head stays continuous.
        for i in [0,1,2,3,4,9,10,11,12,13] {target[i]*=1.-t;}
        let head_step=(60f64.to_radians()*(seconds-start+0.02).max(0.)) as f32;
        for i in [5,6,7] {target[i]+=(-target[i]).clamp(-head_step,head_step);}
        Some(target)
    }
    pub fn ready(&mut self,seconds:f64,obs:&[f32])->Result<bool,&'static str> {
        if !seconds.is_finite() || obs.len()<34 || !obs[..34].iter().all(|v|v.is_finite()) {return Err("翻滚起身观测无效");}
        let norm=obs[3..6].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
        if !(0.5..=1.5).contains(&norm) {return Err("翻滚起身重力观测无效");}
        if let Some((start,_))=self.bridge {return Ok(seconds-start>=Self::EXTEND_SECONDS-1e-9);}
        // Model body axes: supine is gravity along -X (pitch -90 degrees),
        // unlike prone (+X), inverted (+Z), or sideways (+/-Y).
        self.upright_seen|=-(obs[5] as f64)/norm>30f64.to_radians().cos();
        let back=self.upright_seen && seconds>=Self::MIN_SECONDS-1e-9 &&
            -(obs[3] as f64)/norm>=25f64.to_radians().cos();
        self.back_samples=if back {self.back_samples.saturating_add(1)}else{0};
        if self.back_samples>=2 {
            self.bridge=Some((seconds,self.last.ok_or("翻滚展开目标缺失，停止并卸力")?));
        } else if seconds>=Self::TIMEOUT-1e-9 {return Err("翻滚2秒内未确认背朝下姿态，停止并卸力");}
        Ok(false)
    }
}
/// Limit head/neck target speed across getup and the following walk handoff.
/// The actor's raw history is unaffected; returning the head never delays getup.
pub fn slew_head(action:&mut [f32;14],home:&[f32;14],from:[f32;3])->bool {
    let step=1.2f32.to_radians();let mut caught=true;
    for (j,i) in [5,6,7].into_iter().enumerate() {
        let delta=home[i]+action[i]-from[j];caught&=delta.abs()<=step;
        action[i]=from[j]+delta.clamp(-step,step)-home[i];
    }
    caught
}
#[cfg(test)] mod extension_tests {
    use super::*;
    fn obs()->Vec<f32>{let mut o=vec![0.;61];o[5]=-1.;o}
    fn back()->Vec<f32>{let mut o=obs();o[3]=-1.;o[5]=0.;o[1]=7.;o}
    #[test] fn back_attitude_triggers_early_then_half_second_extension() {
        let mut r=RollToGetup::default();r.capture(0.1,[4.;14]);r.ready(0.,&obs()).unwrap();r.capture(0.7,[-0.8;14]);
        assert!(!r.ready(0.76,&back()).unwrap());assert!(r.target(0.76).is_none());
        assert!(!r.ready(0.78,&back()).unwrap());assert!(r.just_started(0.78));
        assert!((r.target(0.78).unwrap()[3]+0.768).abs()<1e-5);assert!((r.target(0.78).unwrap()[5]+0.8-1.2f32.to_radians()).abs()<1e-6);
        assert!(!r.ready(1.26,&back()).unwrap());assert!(r.target(1.26).unwrap()[3].abs()<1e-5);
        assert!(r.ready(1.28,&back()).unwrap());
    }
    #[test] fn prone_sideways_inverted_and_initial_back_do_not_trigger() {
        for g in [[1.,0.,0.],[0.,1.,0.],[0.,0.,1.],[0.,0.,-1.]] {
            let mut r=RollToGetup::default();r.capture(0.,[0.;14]);r.ready(0.,&obs()).unwrap();let mut o=obs();o[3..6].copy_from_slice(&g);
            for t in [0.7,0.72] {r.ready(t,&o).unwrap();assert!(r.target(t).is_none());}
            assert!(r.ready(2.,&o).is_err());
        }
        let mut r=RollToGetup::default();r.capture(0.,[0.;14]);r.ready(0.7,&back()).unwrap();r.ready(0.72,&back()).unwrap();assert!(r.target(0.72).is_none());
        r.ready(0.74,&obs()).unwrap();r.ready(0.76,&back()).unwrap();r.ready(0.78,&obs()).unwrap();r.ready(0.8,&back()).unwrap();assert!(r.target(0.8).is_none());
        assert!(r.ready(0.8,&[]).is_err());let mut o=obs();o[0]=f32::NAN;assert!(r.ready(0.8,&o).is_err());
    }
    #[test] fn recorded_back_attitude_is_detected_before_full_roll() {
        let frames:Vec<Vec<f32>>=serde_json::from_str(include_str!("../tests/fixtures/roll-back-gravity-135.json")).unwrap();
        let mut r=RollToGetup::default();r.capture(0.,[0.;14]);let mut trigger=None;
        for f in frames {let mut o=vec![0.;61];o[..6].copy_from_slice(&f[1..7]);let t=f[0] as f64;r.ready(t,&o).unwrap();if r.target(t).is_some(){trigger=Some(t);break;}}
        assert!((0.65..0.95).contains(&trigger.expect("back attitude in recording")));
    }
    #[test] fn head_returns_at_bounded_speed_without_delaying_getup() {
        let mut r=RollToGetup::default();r.capture(0.,[-2.;14]);r.ready(0.,&obs()).unwrap();r.ready(0.7,&back()).unwrap();r.ready(0.72,&back()).unwrap();
        let end=r.target(1.2).unwrap();assert!((end[5]-(-2.+30f32.to_radians())).abs()<1e-5);
        assert!(r.ready(1.22,&back()).unwrap()); // head is still far from HOME
        let home=[0.2;14];let mut from=[-1.57,-0.6,0.1];
        for tick in 0..150 {
            let mut a=[if tick<50 {1.}else{-0.2};14];let legs=a[3];let before=from;
            slew_head(&mut a,&home,from);
            for (j,i) in [5,6,7].into_iter().enumerate(){from[j]=a[i]+home[i];assert!((from[j]-before[j]).abs()<=1.2f32.to_radians()+1e-6);}
            assert_eq!(a[3],legs);
        }
        for x in from {assert!(x.abs()<1e-5);}
    }
}
