//! Round-based leisure scheduler. Outputs intents; policy.rs remains the UART owner.
use std::time::Instant;
use rand::Rng;
use serde_json::{json, Value};
use super::interaction::Gaze;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Activity { Sit, Roll, Dance, Look, Explore }
impl Activity {
    pub fn name(self)->&'static str {match self {Self::Sit=>"坐下张望",Self::Roll=>"翻滚",Self::Dance=>"跳舞",Self::Look=>"站立张望",Self::Explore=>"避障探索"}}
    fn skill(self)->Option<&'static str> {match self {Self::Sit=>Some("sitstand_sit"),Self::Roll=>Some("xgoduck_roulade"),Self::Dance=>Some("dance10"),_=>None}}
}
#[derive(Default, Debug, PartialEq)]
enum Phase { #[default] Choose, Starting, Holding, Returning }
#[derive(Default)]
pub struct Scheduler {
    phase:Phase, pub activity:Option<Activity>, since:Option<Instant>, hold:f64,
    seen:bool, gaze:Gaze, pub head:Option<[f64;3]>, pub reason:String,
}
pub struct Input<'a> {
    pub now:Instant, pub active:bool, pub kind:&'a str, pub progress:f64, pub sit_settle:f64,
    pub ready:bool, pub busy:bool, pub obstacle:bool, pub round_done:bool,
    // Sit requires both directions. Missing skills are reported and skipped, never queued.
    pub available:[bool;3],
}
#[derive(Default)]
pub struct Output {pub skill:Option<&'static str>, pub voice:Option<&'static str>}
impl Scheduler {
    pub fn reset(&mut self){*self=Self::default();}
    pub fn pause(&mut self){self.head=None;self.gaze=Gaze::default();}
    pub fn observing(&self)->bool {self.phase==Phase::Holding && matches!(self.activity,Some(Activity::Sit|Activity::Look)) && self.head.is_some()}
    pub fn exploring(&self)->bool {self.activity==Some(Activity::Explore) && self.phase==Phase::Holding}
    pub fn status(&self)->Value {json!({"observing":self.observing(),"activity":self.activity.map(Activity::name),"phase":format!("{:?}",self.phase),"holdSeconds":self.hold,"headDegrees":self.head,"reason":self.reason,"parameters":super::parameters::BEHAVIOR.status()})}
    pub fn tick(&mut self,i:Input<'_>)->anyhow::Result<Output>{
        let mut rng=rand::thread_rng();
        self.step(i,rng.gen_range(0..100),rng.gen_range(0. ..=1.))
    }
    fn step(&mut self,i:Input<'_>,draw:u8,fraction:f64)->anyhow::Result<Output>{
        let p=&super::parameters::BEHAVIOR;
        let mut out=Output::default();self.head=None;
        if !i.active {self.reset();return Ok(out);}
        if self.phase==Phase::Choose {
            if !i.ready || i.busy || i.kind!="xgoduck" {return Ok(out);}
            let activity=p.choose(draw);
            let available=match activity {Activity::Sit=>i.available[0],Activity::Roll=>i.available[1],Activity::Dance=>i.available[2],_=>true};
            if !available {self.reason=format!("{}模型不可用，本轮改为站立张望",activity.name());}else{self.reason.clear();}
            let activity=if available {activity}else{Activity::Look};
            self.activity=Some(activity);self.since=Some(i.now);self.seen=false;self.gaze=Gaze::default();
            let range=if activity==Activity::Sit {p.sit_seconds}else{p.look_seconds};
            self.hold=range[0]+fraction*(range[1]-range[0]);
            if let Some(skill)=activity.skill(){out.skill=Some(skill);self.phase=Phase::Starting;}
            else {self.phase=Phase::Holding;}
            return Ok(out);
        }
        let activity=self.activity.expect("active round has an activity");
        let age=i.now.saturating_duration_since(self.since.unwrap()).as_secs_f64();
        if matches!(self.phase,Phase::Starting|Phase::Returning) {
            anyhow::ensure!(age<p.transition_timeout_seconds,"自主{}动作衔接超时",activity.name());
        }
        if self.phase==Phase::Starting {
            if activity.skill()==Some(i.kind) {self.seen=true;}
            if activity==Activity::Sit && self.seen && i.kind=="sitstand_sit" && i.progress>=i.sit_settle {
                self.phase=Phase::Holding;self.since=Some(i.now);self.gaze=Gaze::default();
            }else if activity!=Activity::Sit && self.seen && i.kind=="xgoduck" && i.ready {self.finish();}
            return Ok(out);
        }
        if self.phase==Phase::Returning {
            if i.kind=="sitstand_stand" {self.seen=true;}
            if self.seen && i.kind=="xgoduck" && i.ready {self.finish();}
            return Ok(out);
        }
        if activity==Activity::Explore {
            if i.round_done {self.finish();}
            return Ok(out);
        }
        let seated=activity==Activity::Sit;
        if (seated && (i.kind!="sitstand_sit" || i.progress<i.sit_settle)) || (!seated && i.kind!="xgoduck") {return Ok(out);}
        if age>=self.hold {
            if seated {out.skill=Some("sitstand_stand");self.phase=Phase::Returning;self.seen=false;self.since=Some(i.now);}
            else {self.finish();}
            return Ok(out);
        }
        let (head,voice)=self.gaze.tick(i.now,age,i.obstacle,i.busy,p);
        self.head=Some(head);out.voice=voice;Ok(out)
    }
    fn finish(&mut self){self.phase=Phase::Choose;self.activity=None;self.head=None;self.gaze=Gaze::default();}
}

#[cfg(test)] mod tests {
    use super::*;
    use std::time::Duration;
    fn input(t:Instant)->Input<'static>{Input{now:t,active:true,kind:"xgoduck",progress:0.,sit_settle:2.,ready:true,busy:false,obstacle:false,round_done:false,available:[true;3]}}
    #[test] fn camera_window_only_covers_standing_or_settled_seated_observation(){
        let t=Instant::now();
        for draw in [0,10,20,30,60] {
            let mut s=Scheduler::default();s.step(input(t),draw,0.).unwrap();assert!(!s.observing());
            let mut i=input(t+Duration::from_millis(20));if draw==0 {i.kind="sitstand_sit";i.progress=1.;}
            s.step(i,99,0.).unwrap();assert_eq!(s.observing(),draw==30);
            if draw==0 {
                let mut i=input(t+Duration::from_secs(2));i.kind="sitstand_sit";i.progress=2.;s.step(i,99,0.).unwrap();assert!(!s.observing());
                let mut i=input(t+Duration::from_millis(2020));i.kind="sitstand_sit";i.progress=2.02;s.step(i,99,0.).unwrap();assert!(s.observing());
                let mut i=input(t+Duration::from_secs(7));i.kind="sitstand_sit";i.progress=7.;s.step(i,99,0.).unwrap();assert!(!s.observing());
            }
            s.pause();assert!(!s.observing());s.reset();assert!(!s.observing());
        }
    }
    #[test] fn probabilities_cover_exactly_one_hundred_slots(){
        let p=&super::super::parameters::BEHAVIOR;assert_eq!(p.weights.iter().map(|n|*n as u16).sum::<u16>(),100);
        for (activity,count) in [(Activity::Sit,10),(Activity::Roll,10),(Activity::Dance,10),(Activity::Look,30),(Activity::Explore,40)] {
            assert_eq!((0..100).filter(|n|p.choose(*n)==activity).count(),count);
        }
    }
    #[test] fn sitting_timer_starts_after_settle_then_stands_once(){
        for fraction in [0.,1.] {
            let mut s=Scheduler::default();let t=Instant::now();
            assert_eq!(s.step(input(t),0,fraction).unwrap().skill,Some("sitstand_sit"));
            let mut i=input(t+Duration::from_secs(4));i.kind="sitstand_sit";i.ready=false;i.progress=1.;
            assert!(s.step(i,99,0.).unwrap().skill.is_none());assert_eq!(s.phase,Phase::Starting);
            let settled=t+Duration::from_secs(5);let mut i=input(settled);i.kind="sitstand_sit";i.progress=2.;i.ready=false;
            s.step(i,99,0.).unwrap();assert_eq!(s.phase,Phase::Holding);
            let duration=5.+fraction*5.;
            let mut i=input(settled+Duration::from_secs_f64(duration-0.01));i.kind="sitstand_sit";i.progress=10.;i.ready=false;i.obstacle=true;
            assert_eq!(s.step(i,99,0.).unwrap().voice,Some("explore_obstacle"));assert!(s.head.is_some());
            let end=settled+Duration::from_secs_f64(duration);let mut i=input(end);i.kind="sitstand_sit";i.progress=12.;i.ready=false;
            assert_eq!(s.step(i,99,0.).unwrap().skill,Some("sitstand_stand"));assert!(s.head.is_none());
            s.step(input(end),99,0.).unwrap();assert_eq!(s.phase,Phase::Returning);
            let mut i=input(end+Duration::from_millis(20));i.kind="sitstand_stand";i.ready=false;
            assert!(s.step(i,99,0.).unwrap().skill.is_none());
            s.step(input(end+Duration::from_secs(2)),99,0.).unwrap();assert_eq!(s.phase,Phase::Choose);
        }
    }
    #[test] fn skills_issue_once_and_must_be_seen_before_completion(){
        for (draw,kind) in [(10,"xgoduck_roulade"),(20,"dance10")] {
            let t=Instant::now();let mut s=Scheduler::default();assert_eq!(s.step(input(t),draw,0.).unwrap().skill,Some(kind));
            for n in 1..20 {assert!(s.step(input(t+Duration::from_millis(n*20)),99,0.).unwrap().skill.is_none());}
            assert_eq!(s.phase,Phase::Starting);
            let mut i=input(t+Duration::from_secs(1));i.kind=kind;i.ready=false;s.step(i,99,0.).unwrap();
            let mut i=input(t+Duration::from_secs(10));i.ready=false;s.step(i,99,0.).unwrap();assert_eq!(s.phase,Phase::Starting);
            s.step(input(t+Duration::from_secs(11)),99,0.).unwrap();assert_eq!(s.phase,Phase::Choose);
        }
    }
    #[test] fn look_duration_bounds_and_explore_round_completion(){
        for fraction in [0.,1.] {
            let t=Instant::now();let mut s=Scheduler::default();s.step(input(t),30,fraction).unwrap();
            let seconds=15.+fraction*15.;s.step(input(t+Duration::from_secs_f64(seconds-0.01)),99,0.).unwrap();assert!(s.head.is_some());
            s.step(input(t+Duration::from_secs_f64(seconds)),99,0.).unwrap();assert!(s.head.is_none());assert_eq!(s.phase,Phase::Choose);
        }
        let t=Instant::now();let mut s=Scheduler::default();s.step(input(t),60,0.).unwrap();
        let mut i=input(t+Duration::from_secs(90));i.obstacle=true;assert!(s.step(i,0,0.).unwrap().voice.is_none());assert!(s.exploring());assert!(s.head.is_none());
        let mut i=input(t+Duration::from_secs(91));i.round_done=true;s.step(i,0,0.).unwrap();assert!(!s.exploring());
    }
    #[test] fn no_selection_until_ready_and_missing_skill_falls_back_visibly(){
        let t=Instant::now();let mut s=Scheduler::default();let mut i=input(t);i.ready=false;s.step(i,0,0.).unwrap();assert!(s.activity.is_none());
        let mut i=input(t);i.busy=true;s.step(i,0,0.).unwrap();assert!(s.activity.is_none());
        let mut i=input(t);i.available=[false;3];assert!(s.step(i,0,0.).unwrap().skill.is_none());assert_eq!(s.activity,Some(Activity::Look));assert!(!s.reason.is_empty());
    }
    #[test] fn cancellation_clears_every_phase_without_return_requests(){
        let t=Instant::now();
        for draw in [0,10,20,30,60] {
            let mut s=Scheduler::default();s.step(input(t),draw,0.).unwrap();
            let mut i=input(t+Duration::from_secs(1));i.active=false;i.obstacle=true;
            let out=s.step(i,0,0.).unwrap();assert!(out.skill.is_none() && out.voice.is_none());assert!(s.activity.is_none() && s.head.is_none());
        }
    }
    #[test] fn feedback_pause_clears_expression_without_reissuing_skill_and_timeout_is_bounded(){
        let t=Instant::now();let mut s=Scheduler::default();s.step(input(t),0,0.).unwrap();s.pause();
        assert!(s.step(input(t+Duration::from_secs(1)),0,0.).unwrap().skill.is_none());
        assert!(s.step(input(t+Duration::from_secs(20)),0,0.).is_err());
    }
}
