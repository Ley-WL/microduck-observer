//! Head-only leisure interaction, independent of actor weights and navigation scans.
use std::time::Instant;
use super::parameters::{BehaviorParameters,LOOK_UP};
#[derive(Default)]
pub struct Gaze {last_response:Option<Instant>, response:Option<Instant>}
impl Gaze {
    pub fn tick(&mut self,now:Instant,age:f64,obstacle:bool,busy:bool,p:&BehaviorParameters)->([f64;3],Option<&'static str>){
        let mut voice=None;
        if obstacle && !busy && self.last_response.is_none_or(|at|now.saturating_duration_since(at).as_secs_f64()>=p.response_period){
            self.last_response=Some(now);self.response=Some(now);voice=Some("explore_obstacle");
        }
        if !obstacle {self.response=None;}
        let mut head=[LOOK_UP,p.look_yaw*(std::f64::consts::TAU*age/p.look_period).sin(),0.];
        if let Some(at)=self.response {
            let t=now.saturating_duration_since(at).as_secs_f64()/p.response_period;
            if t<1. {
                let wave=(std::f64::consts::TAU*t).sin();
                head[0]+=p.response_degrees[0]*wave;
                head[1]+=p.response_degrees[1]*wave;
                head[2]+=p.response_degrees[2]*(2.*std::f64::consts::TAU*t).sin();
            }else{self.response=None;}
        }
        (head,voice)
    }
}

#[cfg(test)] mod tests {
    use super::*;use std::time::Duration;
    #[test] fn near_obstacle_calls_every_two_seconds_without_audio_backlog(){
        let p=&super::super::parameters::BEHAVIOR;let mut g=Gaze::default();let t=Instant::now();
        assert!(g.tick(t,0.,false,false,p).1.is_none());
        assert_eq!(g.tick(t,0.,true,false,p).1,Some("explore_obstacle"));
        let (head,voice)=g.tick(t+Duration::from_millis(250),0.25,true,false,p);assert!(voice.is_none());assert!(head[0].abs()>0. && head[2].abs()>0.);
        assert!(g.tick(t+Duration::from_millis(1999),1.999,true,false,p).1.is_none());
        assert!(g.tick(t+Duration::from_secs(2),2.,true,true,p).1.is_none());
        assert_eq!(g.tick(t+Duration::from_secs(3),3.,true,false,p).1,Some("explore_obstacle"));
        assert!(g.tick(t+Duration::from_millis(3020),3.02,true,false,p).1.is_none());
        let (head,voice)=g.tick(t+Duration::from_secs(4),4.,false,false,p);assert!(voice.is_none());assert_eq!(head[0],LOOK_UP);assert_eq!(head[2],0.);
    }
    #[test] fn head_envelope_is_bounded_and_no_obstacle_only_scans_yaw(){
        let p=&super::super::parameters::BEHAVIOR;let t=Instant::now();let mut g=Gaze::default();
        for n in 0..1500 {let age=n as f64*0.02;let (head,_)=g.tick(t+Duration::from_secs_f64(age),age,true,false,p);
            assert!((head[0]-LOOK_UP).abs()<=6. && head[1].abs()<=57. && head[2].abs()<=5.);
        }
        let (head,_)=g.tick(t+Duration::from_secs(31),31.,false,false,p);assert_eq!(head[0],LOOK_UP);assert_eq!(head[2],0.);
    }
}
