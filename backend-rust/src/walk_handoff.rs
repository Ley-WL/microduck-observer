//! Shared posture certificate and routing for every skill -> walk boundary.
use std::time::{Duration,Instant};
#[derive(Default)]
pub struct Gate { since:Option<Instant>, last:Option<Instant>, ready:bool }
impl Gate {
    pub fn standing(obs:&[f32])->bool {
        if obs.len()<34 || !obs[..34].iter().all(|v|v.is_finite()) {return false;}
        let norm=obs[3..6].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
        if !(0.5..=1.5).contains(&norm) {return false;}
        let tilt=(-(obs[5] as f64)/norm).clamp(-1.,1.).acos().to_degrees();
        let gyro=obs[..3].iter().map(|v|(*v as f64).powi(2)).sum::<f64>().sqrt();
        tilt<10. && gyro<0.5 && (0..5).chain(9..14).all(|n|obs[6+n].abs()<0.30 && obs[20+n].abs()<1.)
    }
    pub fn reset(&mut self) {*self=Self::default();}
    pub fn observe(&mut self,obs:&[f32],now:Instant) {
        if self.last.is_some_and(|last|now<last || now.duration_since(last)>Duration::from_millis(60)) {self.reset();}
        self.last=Some(now);
        if !Self::standing(obs) {self.since=None;self.ready=false;return;}
        let since=*self.since.get_or_insert(now);
        self.ready=now.duration_since(since)>=Duration::from_millis(500);
    }
    pub fn ready(&self)->bool {self.ready}
    pub fn seconds(&self)->f64 {self.since.zip(self.last).map_or(0.,|(a,b)|b.duration_since(a).as_secs_f64())}
    /// Preserve non-walk requests. Intentional seated/single-foot poses must
    /// use their own return action; an unfinished roll must not be interrupted.
    pub fn route(&self,active:&str,next:Option<String>,phase_done:bool)->Option<String> {
        let next=next?;
        if next!="xgoduck" || active=="xgoduck" {return Some(next);}
        match active {
            "sitstand_sit"=>Some("sitstand_stand".into()),
            "flamingo_hold"=>Some("flamingo_lower".into()),
            "xgoduck_roulade" if !phase_done=>None,
            _ if self.ready=>Some(next),
            "xgoduck_getup"|"sitstand_stand"=>None,
            _=>Some("xgoduck_getup".into()),
        }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn pose()->Vec<f32>{let mut x=vec![0.;61];x[5]=-1.;x}
    fn certify(g:&mut Gate,t:Instant){for n in 0..=25{g.observe(&pose(),t+Duration::from_millis(n*20));}}
    #[test] fn exact_half_second_fresh_and_no_stale_certificate(){
        let t=Instant::now();let mut g=Gate::default();
        for n in 0..25{g.observe(&pose(),t+Duration::from_millis(n*20));}assert!(!g.ready());
        g.observe(&pose(),t+Duration::from_millis(500));assert!(g.ready());
        g.observe(&pose(),t+Duration::from_millis(600));assert!(!g.ready());
        certify(&mut g,t+Duration::from_secs(1));g.reset();assert!(!g.ready());
    }
    #[test] fn invalid_motion_or_folded_leg_clears_dwell(){
        let t=Instant::now();for index in [0,3,6,15,20,29] {
            let mut g=Gate::default();certify(&mut g,t);let mut bad=pose();bad[index]=2.;
            g.observe(&bad,t+Duration::from_millis(520));assert!(!g.ready());
        }
        assert!(!Gate::standing(&[]));let mut p=pose();p[5]=0.;assert!(!Gate::standing(&p));p[5]=f32::NAN;assert!(!Gate::standing(&p));
    }
    #[test] fn every_exit_uses_one_certificate_and_intentional_returns(){
        let mut g=Gate::default();
        for kind in ["xgoduck_pick","dance10","kick_left","kick_right","flamingo_lower","xgoduck_roulade"] {
            assert_eq!(g.route(kind,Some("xgoduck".into()),true),Some("xgoduck_getup".into()));
        }
        for kind in ["xgoduck_getup","sitstand_stand"] {assert_eq!(g.route(kind,Some("xgoduck".into()),true),None);}
        certify(&mut g,Instant::now());
        for kind in ["xgoduck_pick","dance10","kick_left","kick_right","flamingo_lower","xgoduck_getup","sitstand_stand","xgoduck_roulade"] {
            assert_eq!(g.route(kind,Some("xgoduck".into()),true),Some("xgoduck".into()));
        }
        assert_eq!(g.route("sitstand_sit",Some("xgoduck".into()),false),Some("sitstand_stand".into()));
        assert_eq!(g.route("flamingo_hold",Some("xgoduck".into()),false),Some("flamingo_lower".into()));
        assert_eq!(g.route("xgoduck_roulade",Some("xgoduck".into()),false),None);
        assert_eq!(g.route("dance10",Some("sitstand_sit".into()),false),Some("sitstand_sit".into()));
    }
}
