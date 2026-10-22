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
