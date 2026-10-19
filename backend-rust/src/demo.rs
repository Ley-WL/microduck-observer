//! A bounded demonstration sequencer; only the policy UART owner executes outputs.
use std::time::{Duration,Instant};
use serde_json::{json,Value};
#[derive(Default,Clone,Debug)]
pub struct Output { pub twist:[f32;3], pub head:[f64;2], pub skill:Option<String>, pub voice:Option<&'static str>, pub stopped:bool }
const NAMES:[&str;9]=["开场招呼与转头","坐下","站起并站稳","低速前进","低速后退","拾取","拾取回应","跳舞","转头与长叫收尾"];
#[derive(Default)]
pub struct Demo {
    pub active:bool, pub session:String, pub output:Output,
    ended:Option<Instant>, started:Option<Instant>, step_at:Option<Instant>, heartbeat:Option<Instant>,
    stable_since:Option<Instant>, observed:Option<Instant>, pub ready:bool,
    step:usize, issued:bool, seen:bool, cues:u8, return_pending:bool,
    pub reason:String,
    completed:Vec<Value>,
}
impl Demo {
    pub fn start(&mut self,session:&str,now:Instant)->anyhow::Result<()> {
        anyhow::ensure!(!self.active && self.ready && self.observed.is_some_and(|at|now.saturating_duration_since(at)<Duration::from_millis(150)),"请先保持稳定站立、摇杆回中");
        self.completed.clear();self.ended=None;self.active=true;self.session=session.into();self.started=Some(now);self.step_at=Some(now);self.heartbeat=Some(now);
        self.step=0;self.issued=false;self.seen=false;self.cues=0;self.return_pending=false;self.reason.clear();self.output=Output::default();Ok(())
    }
    pub fn renew(&mut self,session:&str,now:Instant) {if self.active && self.session==session {self.heartbeat=Some(now);}}
    pub fn cancel(&mut self,reason:&str,return_to_stand:bool) {
        if !return_to_stand {self.return_pending=false;}
        if self.active {self.ended=Some(Instant::now());self.reason=reason.into();self.active=false;self.return_pending=return_to_stand;self.output=Output::default();}
    }
    pub fn status(&self)->Value {json!({"active":self.active,"ready":self.ready,"step":self.step+1,"steps":NAMES.len(),"name":NAMES[self.step],"elapsedSeconds":self.started.map(|t|self.ended.unwrap_or_else(Instant::now).saturating_duration_since(t).as_secs_f64()),"expectedSeconds":30,"completedStages":self.completed,"timeoutSeconds":55,"reason":self.reason})}
    pub fn tick(&mut self,now:Instant,kind:&str,progress:f64,sit_settle:f64,stable:bool,audio_busy:bool)->Output {
        self.observed=Some(now);
        if stable && kind=="xgoduck" {self.stable_since.get_or_insert(now);} else {self.stable_since=None;}
        self.ready=self.stable_since.is_some_and(|at|now.saturating_duration_since(at)>=Duration::from_millis(500));
        let mut out=Output::default();
        if self.active && (self.started.is_some_and(|at|now.saturating_duration_since(at)>=Duration::from_secs(55)) || self.heartbeat.is_none_or(|at|now.saturating_duration_since(at)>=Duration::from_millis(500))) {
            self.cancel("演示超时或控制连接中断",true);out.stopped=true;
        }
        if !self.active {
            if std::mem::take(&mut self.return_pending) && kind!="xgoduck" {out.skill=Some(if kind=="sitstand_sit" {"sitstand_stand"}else{"xgoduck"}.into());}
            self.output=out.clone();return out;
        }
        let age=now.saturating_duration_since(self.step_at.unwrap()).as_secs_f64();
        let target=match self.step {1=>Some("sitstand_sit"),2=>Some("sitstand_stand"),5=>Some("xgoduck_pick"),7=>Some("dance10"),_=>None};
        if let Some(target)=target {
            if !self.issued {out.skill=Some(target.into());self.issued=true;}
            if kind==target {self.seen=true;}
        }
        let max=match self.step {1|2=>10.,5=>9.,7=>12.,_=>9.};
        if age>=max {
            self.cancel("当前演示动作未按时完成",true);out=Output{stopped:true,..Default::default()};
            if std::mem::take(&mut self.return_pending) && kind!="xgoduck" {out.skill=Some(if kind=="sitstand_sit" {"sitstand_stand"}else{"xgoduck"}.into());}
            self.output=out.clone();return out;
        }
        let voice=match self.step {0 if age>=0.2=>Some((1,"single")),1 if self.seen && progress>=sit_settle=>Some((1,"single")),2 if self.seen && self.ready=>Some((1,"single")),3|4 if age>=0.4=>Some((1,"single")),6 if age>=0.1=>Some((1,"double")),7 if age>=4.5=>Some((2,"double")),7 if age>=2.=>Some((1,"single")),8 if age>=2.2=>Some((1,"long")),_=>None};
        if let Some((cue,voice))=voice {if self.cues<cue && !audio_busy {out.voice=Some(voice);self.cues=cue;}}
        match self.step {
            0=>out.head=[0.,if age<0.8 {0.18}else if age<1.6 {-0.18}else{0.}],
            3 if age<1.5=>out.twist=[0.15,0.,0.],
            4 if age<1.5=>out.twist=[-0.15,0.,0.],
            8=>out.head=if age<0.8 {[0.,0.18]}else if age<1.6 {[0.,-0.18]}else if age<2.3 {[0.12,0.]}else if age<3.0 {[-0.10,0.]}else{[0.;2]},
            _=>{}
        }
        let done=match self.step {
            0=>age>=2.8 && self.ready,
            1=>self.seen && kind=="sitstand_sit" && progress>=sit_settle+0.25,
            2=>self.seen && self.ready,
            3|4=>age>=2. && self.ready,
            5=>self.seen && self.ready,
            6=>age>=0.2 && !audio_busy && out.voice.is_none(),
            7=>self.seen && self.ready,
            8=>age>=4.8 && self.ready && !audio_busy && out.voice.is_none(),_=>false
        };
        if done {
            self.completed.push(json!({"name":NAMES[self.step],"seconds":age}));
            if self.step==8 {self.cancel("演示完成",false);out=Output{stopped:true,..Default::default()};}
            else {self.step+=1;self.step_at=Some(now);self.issued=false;self.seen=false;self.cues=0;}
        }
        self.output=out.clone();out
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn ready()->(Demo,Instant) {let mut d=Demo::default();let now=Instant::now();d.tick(now,"xgoduck",0.,4.,true,false);let now=now+Duration::from_millis(600);d.tick(now,"xgoduck",0.,4.,true,false);(d,now)}
    #[test] fn rejects_unstable_and_stale_start(){let (mut d,t)=ready();assert!(d.start("s",t+Duration::from_millis(151)).is_err());d.tick(t,"sitstand_sit",5.,4.,true,false);assert!(d.start("s",t).is_err());}
    #[test] fn timeout_and_disconnect_clear_motion_and_voice(){let(mut d,t)=ready();d.start("s",t).unwrap();d.step=3;d.renew("s",t);assert_eq!(d.tick(t,"xgoduck",0.,4.,true,false).twist[0],0.15);let o=d.tick(t+Duration::from_millis(501),"xgoduck",0.,4.,true,false);assert!(!d.active);assert_eq!(o.twist,[0.;3]);assert!(o.voice.is_none());let(mut d,t)=ready();d.start("s",t).unwrap();let end=t+Duration::from_secs(55);d.renew("s",end);assert!(d.tick(end,"dance10",2.,4.,false,false).stopped);assert!(!d.active);}
    #[test] fn skill_must_be_observed_before_advancing(){let(mut d,t)=ready();d.start("s",t).unwrap();d.step=5;for n in 0..80 {let now=t+Duration::from_millis(n*100);d.renew("s",now);d.tick(now,"xgoduck",0.,4.,true,false);}assert_eq!(d.step,5);assert!(!d.seen);}
    #[test] fn cancel_returns_from_sitting_and_cannot_queue_more_sounds(){let(mut d,t)=ready();d.start("s",t).unwrap();d.cancel("manual",true);let o=d.tick(t,"sitstand_sit",5.,4.,false,false);assert_eq!(o.skill.as_deref(),Some("sitstand_stand"));assert!(o.voice.is_none());assert!(d.tick(t,"sitstand_stand",0.,4.,false,false).skill.is_none());}

    #[test] fn complete_sequence_waits_for_skills_and_finishes_with_bounded_motion(){
        let(mut d,t)=ready();d.start("s",t).unwrap();
        let mut kind="xgoduck".to_string();let mut skill_at=t;let mut audio_until=t;
        let mut skills=Vec::new();let mut voices=Vec::new();let mut forward=0;let mut reverse=0;
        let mut elapsed=0.;
        for n in 0..2750 {
            let now=t+Duration::from_millis(n*20);elapsed=n as f64*0.02;
            let progress=now.saturating_duration_since(skill_at).as_secs_f64();
            if (kind=="sitstand_stand" && progress>=4.) || (kind=="xgoduck_pick" && progress>=4.) || (kind=="dance10" && progress>=8.) {kind="xgoduck".into();}
            d.renew("s",now);
            let o=d.tick(now,&kind,progress,2.,true,now<audio_until);
            if o.twist[0]>0. {forward+=1;}if o.twist[0]<0. {reverse+=1;}
            assert!(o.twist[0].abs()<=0.15);
            if let Some(voice)=o.voice {assert_ne!(kind,"xgoduck_pick");voices.push(voice);audio_until=now+Duration::from_millis(if voice=="long" {1800}else{1000});}
            if let Some(next)=o.skill {skills.push(next.clone());kind=next;skill_at=now;}
            if !d.active {break;}
        }
        assert_eq!(d.completed.len(),9);assert_eq!(d.reason,"演示完成");assert!((28.0..35.0).contains(&elapsed),"{elapsed}");
        assert_eq!(skills,["sitstand_sit","sitstand_stand","xgoduck_pick","dance10"]);
        assert!((74..=76).contains(&forward));assert!((74..=76).contains(&reverse));
        assert!(voices.contains(&"double") && voices.contains(&"long"));assert_eq!(d.output.twist,[0.;3]);
    }
    #[test] fn wrong_session_cannot_extend_watchdog(){let(mut d,t)=ready();d.start("owner",t).unwrap();d.renew("other",t+Duration::from_millis(490));assert!(d.tick(t+Duration::from_millis(501),"xgoduck",0.,2.,true,false).stopped);}
    #[test] fn stop_discards_a_previously_requested_return_to_stand(){let(mut d,t)=ready();d.start("s",t).unwrap();d.cancel("manual",true);d.cancel("feedback lost",false);assert!(d.tick(t,"sitstand_sit",5.,2.,false,false).skill.is_none());}
}
