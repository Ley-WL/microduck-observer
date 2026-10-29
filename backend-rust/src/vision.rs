//! Latest-frame perception and in-place interaction. Never owns UART or body motion.
use std::{process::Stdio,sync::{Arc,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use serde_json::{json,Value};
use tokio::{io::{AsyncBufReadExt,AsyncReadExt,AsyncWriteExt,BufReader},process::{Child,Command}};
use crate::{autonomy::Mode,telemetry::Shared};

#[derive(Clone,Copy,Debug)]
pub struct Target {pub face:bool,pub score:f64,pub bounds:[f64;4]}
impl Target {
    fn point(self)->[f64;2] {let [l,t,r,b]=self.bounds;[(l+r)*0.5,if self.face{(t+b)*0.5}else{t+(b-t)*0.22}]}
    fn related(self,other:Self)->bool {
        let a=self.point();let b=other.point();
        if self.face!=other.face {let (face,body)=if self.face{(a,other.bounds)}else{(b,self.bounds)};
            return face[0]>=body[0] && face[0]<=body[2] && face[1]>=body[1] && face[1]<=body[3];}
        (a[0]-b[0]).abs()<0.20 && (a[1]-b[1]).abs()<0.22
    }
}
// Randomized, bounded search; trajectory still passes through the sole owner's slew/limits.
#[derive(Default)]
struct Search {last:Option<Instant>,pause:Option<Instant>,goal:Option<f64>,speed:f64,velocity:f64}
impl Search {
    fn advance(&mut self,now:Instant,yaw:f64)->f64 {
        use rand::Rng;
        let dt=self.last.replace(now).map(|t|now.saturating_duration_since(t).as_secs_f64().min(0.05)).unwrap_or(0.);
        if self.pause.is_some_and(|until|now<until){return yaw;}
        if self.goal.is_none(){let mut rng=rand::thread_rng();
            let direction=if yaw.abs()<1. {if rng.gen_bool(0.5){1.}else{-1.}}else{-yaw.signum()};
            self.goal=Some(direction*rng.gen_range(12.0..=32.0));self.speed=rng.gen_range(20.0..=34.0);self.pause=None;}
        let distance=self.goal.unwrap()-yaw;
        // Brake before the endpoint; no abrupt random direction changes mid-sweep.
        let wanted=distance.signum()*self.speed.min((160.*distance.abs()).sqrt());
        self.velocity+=(wanted-self.velocity).clamp(-80.*dt,80.*dt);
        let delta=self.velocity*dt;
        if distance.abs()<0.15 || (delta.signum()==distance.signum() && delta.abs()>=distance.abs()) {
            let goal=self.goal.take().unwrap();self.velocity=0.;
            self.pause=Some(now+Duration::from_secs_f64(rand::thread_rng().gen_range(0.2..=0.65)));return goal;
        }
        (yaw+delta).clamp(-35.,35.)
    }
}
#[derive(Default)]
pub struct Interaction {
    pub error:String,pub metrics:Value,pub sound:bool,
    body:Option<Target>,stable_anchor:Option<Target>,stable_since:Option<Instant>,
    target:Option<Target>,candidate:Option<Instant>,seen:Option<Instant>,frame:Option<Instant>,
    started:Option<Instant>,locked:bool,epoch:u64,sequence:u64,
    offset:[f64;2],search:Search,last_call:Option<Instant>,greeted:bool,
}
impl Interaction {
    fn reset(&mut self,epoch:u64){self.epoch=epoch;self.search=Search::default();self.body=None;self.stable_anchor=None;self.stable_since=None;self.target=None;self.candidate=None;self.seen=None;self.frame=None;self.started=None;self.locked=false;self.offset=[0.;2];self.greeted=false;self.sequence=0;}
    pub fn suspend(&mut self,epoch:u64){self.reset(epoch);}
    pub fn ingest(&mut self,value:&Value,now:Instant,epoch:u64)->anyhow::Result<()> {
        if self.epoch!=epoch{self.reset(epoch);}
        if value["waiting"]==true{return Ok(());}
        let seq=value["sequence"].as_u64().ok_or_else(||anyhow::anyhow!("缺少视觉帧序号"))?;
        let age=value["ageMs"].as_f64().filter(|v|v.is_finite()&&*v>=0.&&*v<=500.).ok_or_else(||anyhow::anyhow!("视觉帧已过期"))?;
        let rows=value["boxes"].as_array().filter(|v|v.len()<=64).ok_or_else(||anyhow::anyhow!("检测结果格式无效"))?;
        let mut boxes=Vec::new();
        for row in rows {
            let face=match row["kind"].as_u64(){Some(0)=>false,Some(1)=>true,_=>anyhow::bail!("无效检测类别")};
            let score=row["score"].as_f64().filter(|v|v.is_finite()&&(0. ..=1.).contains(v)).ok_or_else(||anyhow::anyhow!("无效检测分数"))?;
            let coords=row["bounds"].as_array().filter(|v|v.len()==4).ok_or_else(||anyhow::anyhow!("无效检测框"))?;
            let mut bounds=[0.;4];for (i,v) in coords.iter().enumerate(){let extent=if i%2==0{480.}else{640.};
                bounds[i]=v.as_f64().filter(|v|v.is_finite()&&*v>=0.&&*v<=extent).ok_or_else(||anyhow::anyhow!("检测框越界"))?/extent;}
            if bounds[2]<=bounds[0] || bounds[3]<=bounds[1]{continue;}
            if score>=if face{0.60}else{0.45}{boxes.push(Target{face,score,bounds});}
        }
        if seq==self.sequence{return Ok(());}self.sequence=seq;
        let at=now.checked_sub(Duration::from_secs_f64(age/1000.)).unwrap_or(now);
        if self.frame.is_some_and(|t|at<=t){return Ok(());}
        let dt=self.frame.map(|t|at.saturating_duration_since(t).as_secs_f64().min(0.3)).unwrap_or(0.1);
        self.frame=Some(at);self.started.get_or_insert(now);self.error.clear();
        self.metrics=json!({"frameIntervalMs":dt*1000.,"totalMs":value["totalMs"],"cpuMs":value["cpuMs"],"rotationMs":value["rotationMs"],"resizeMs":value["resizeMs"],"faceMs":value["faceMs"],"personMs":value["personMs"],"maxRssKiB":value["maxRssKiB"]});
        if self.seen.is_some_and(|t|now.saturating_duration_since(t)>=Duration::from_secs(2)) {self.target=None;self.body=None;self.stable_since=None;self.stable_anchor=None;self.locked=false;self.candidate=None;self.greeted=false;}
        let old=self.body;
        // Body identity is the only association anchor. Faces cannot acquire a target.
        let choice=boxes.iter().copied().filter(|v|!v.face && old.is_none_or(|o|o.related(*v)))
            .min_by(|a,b|{let rank=|v:Target|{let p=v.point();(p[0]-0.5).abs()+(p[1]-0.4).abs()*0.3-v.score*0.1};rank(*a).total_cmp(&rank(*b))});
        if let Some(body)=choice {
            self.search=Search::default();
            let steady=self.stable_anchor.is_some_and(|a|a.bounds.iter().zip(body.bounds).all(|(x,y)|(x-y).abs()<0.03))
                && self.seen.is_some_and(|t|at.saturating_duration_since(t)<=Duration::from_millis(300));
            if !steady {self.stable_anchor=Some(body);self.stable_since=Some(at);}
            self.body=Some(body);
            let stable=self.stable_since.is_some_and(|t|at.saturating_duration_since(t)>=Duration::from_millis(500));
            let target=if stable {boxes.iter().copied().filter(|v|v.face && body.related(*v)
                && v.point()[1]<=body.bounds[1]+(body.bounds[3]-body.bounds[1])*0.60)
                .min_by(|a,b|{let rank=|v:Target|{let p=v.point();let q=body.point();(p[0]-q[0]).abs()+(p[1]-q[1]).abs()};rank(*a).total_cmp(&rank(*b))}).unwrap_or(body)}else{body};
            if self.seen.is_some_and(|t|at.saturating_duration_since(t)>Duration::from_millis(450)) && !self.locked{self.candidate=None;}
            self.candidate.get_or_insert(at);self.seen=Some(at);self.target=Some(target);
            self.locked|=self.candidate.is_some_and(|t|at.saturating_duration_since(t)>=Duration::from_millis(500));
            if self.locked {let p=target.point();let dead=|v:f64|if v.abs()<0.06{0.}else{v};
                self.offset[0]=(self.offset[0]+dead(p[1]-0.5)*60.*dt).clamp(-18.,18.);
                self.offset[1]=(self.offset[1]-dead(p[0]-0.5)*90.*dt).clamp(-35.,35.);}
        } else {self.stable_since=None;self.stable_anchor=None;}
        Ok(())
    }
    pub fn fresh(&self,now:Instant)->bool {self.frame.is_some_and(|t|now.saturating_duration_since(t)<Duration::from_millis(600)) && self.error.is_empty()}
    pub fn tracking(&self)->bool {self.locked && self.target.is_some()}
    fn face_tracking(&self)->bool {self.tracking() && self.fresh(Instant::now()) && self.stable_since.zip(self.seen).is_some_and(|(start,end)|end.saturating_duration_since(start)>=Duration::from_millis(500) && end.elapsed()<Duration::from_millis(300))}
    pub fn desired(&mut self,now:Instant,epoch:u64)->[f64;2] {
        if self.epoch!=epoch{self.reset(epoch);}
        if !self.fresh(now){self.search=Search::default();self.offset=[0.;2];return [0.;2];}
        if self.seen.is_some_and(|t|now.saturating_duration_since(t)<Duration::from_secs(2)) && self.target.is_some(){return self.offset;}
        self.target=None;self.body=None;self.stable_since=None;self.stable_anchor=None;self.candidate=None;self.locked=false;self.greeted=false;
        // Mechanical negative pitch looks up, relative to the standing model pose.
        self.offset=[-10.,self.search.advance(now,self.offset[1])];self.offset
    }
    pub fn tracking_pose(&mut self,now:Instant,epoch:u64)->Option<[f64;2]> {
        let pose=self.desired(now,epoch);
        (self.tracking() && self.fresh(now) && self.seen.is_some_and(|at|now.saturating_duration_since(at)<Duration::from_millis(450))).then_some(pose)
    }
    pub fn observe_leisure(&mut self,now:Instant,epoch:u64,mut head:[f64;3],obstacle:bool,busy:bool)->([f64;3],Option<&'static str>) {
        if obstacle{return (head,None);}
        if let Some(pose)=self.tracking_pose(now,epoch) {
            head[0]=crate::exploration::parameters::LOOK_UP+pose[0];head[1]=pose[1];
            return (head,self.call(now,busy));
        }
        (head,None)
    }
    pub fn call(&mut self,now:Instant,audio_busy:bool)->Option<&'static str> {
        if audio_busy || !self.locked || !self.fresh(now) || !self.seen.is_some_and(|t|now.saturating_duration_since(t)<Duration::from_millis(450)){return None;}
        let elapsed=self.last_call.map(|t|now.saturating_duration_since(t).as_secs_f64()).unwrap_or(100.);
        if (!self.greeted && elapsed>=20.) || (self.greeted && elapsed>=28.) {self.greeted=true;self.last_call=Some(now);self.sound=true;Some("single")}else{None}
    }
    pub fn status(&self,mode:Mode)->Value {
        let now=Instant::now();let state=if mode==Mode::Off{"off"}else if mode!=Mode::Standby{"paused"}else if !self.error.is_empty(){"error"}else if !self.fresh(now){"waiting"}else if self.target.is_some() && self.seen.is_some_and(|t|now.saturating_duration_since(t)>=Duration::from_millis(450)){"lost"}else if self.locked{"watching"}else{"searching"};
        json!({"state":state,"behavior":match state{"off"=>"已关闭","paused"=>"互动暂停","error"=>"视觉不可用","waiting"=>"等待摄像头","lost"=>"目标丢失","watching"=>"正在注视",_=>"寻找目标"},"error":self.error,"target":self.target.map(|t|json!({"kind":if t.face{"face"}else{"person"},"score":t.score,"bounds":t.bounds})),"frameAgeMs":self.frame.map(|t|t.elapsed().as_secs_f64()*1000.),"metrics":self.metrics,"hardware":"RGA + RKNN","headOffsetDegrees":self.offset})
    }
}
// Absolute degree offsets with 36deg/s slew at 50Hz, composed/clamped by the sole control owner.
pub fn step(offset:&mut [f64;2],desired:[f64;2],base:[f64;2],bounds:[(f64,f64);2]){
    for i in 0..2{offset[i]+=(desired[i]-offset[i]).clamp(-0.72,0.72);offset[i]=(base[i]+offset[i]).clamp(bounds[i].0,bounds[i].1)-base[i];}
}
struct Worker {child:Child,input:tokio::process::ChildStdin,output:BufReader<tokio::process::ChildStdout>,log:tokio::task::JoinHandle<()>}
impl Worker {
    fn start()->anyhow::Result<Self>{
        let exe=std::env::current_exe()?;let bin=exe.parent().unwrap();let models=bin.join("../models/vision");
        let mut child=Command::new(bin.join("microduck-vision-worker")).arg(models.join("yolov5n.rknn")).arg(models.join("RetinaFace_mobile320.rknn"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()?;
        let mut err=child.stderr.take().unwrap();
        let log=tokio::spawn(async move {let mut buf=[0;2048];loop{match err.read(&mut buf).await{Ok(0)=>break,Ok(n)=>eprint!("vision: {}",String::from_utf8_lossy(&buf[..n])),Err(e)=>{eprintln!("vision stderr: {e}");break;}}}});
        Ok(Self{input:child.stdin.take().unwrap(),output:BufReader::new(child.stdout.take().unwrap()),child,log})
    }
    async fn request(&mut self,both:bool)->anyhow::Result<Value>{
        self.input.write_all(if both{b"b"}else{b"p"}).await?;
        let mut data=Vec::new();let n=(&mut self.output).take(16385).read_until(b'\n',&mut data).await?;
        anyhow::ensure!(n>0 && n<=16384 && data.last()==Some(&b'\n'),"视觉进程输出中断或过大");
        let mut value:Value=serde_json::from_slice(&data)?;
        if value["waiting"]!=true {
            #[cfg(unix)] {let mut t:libc::timespec=unsafe{std::mem::zeroed()};
                anyhow::ensure!(unsafe{libc::clock_gettime(libc::CLOCK_MONOTONIC,&mut t)}==0,"无法读取视觉时钟");
                let now=t.tv_sec as u64*1_000_000_000+t.tv_nsec as u64;
                let captured=value["capturedNs"].as_u64().filter(|t|*t<=now).ok_or_else(||anyhow::anyhow!("视觉采集时钟无效"))?;
                value["ageMs"]=json!((now-captured) as f64/1e6);
            }
        }
        Ok(value)
    }
    async fn stop(mut self){let _=self.child.kill().await;let _=self.child.wait().await;let _=self.log.await;
        let path=std::env::var("MICRODUCK_VISION_FRAME").unwrap_or("/run/microduck-observer/vision.rgb".into());
        if let Err(e)=tokio::fs::remove_file(path).await{if e.kind()!=std::io::ErrorKind::NotFound{eprintln!("vision cleanup: {e}");}}
    }
}
pub fn init(shared:Shared,stop:Arc<AtomicBool>){tokio::spawn(async move{
    let mut worker:Option<Worker>=None;let mut last=Instant::now()-Duration::from_secs(10);let mut retry=last;
    loop {
        let (enabled,active,epoch,tracking,face_tracking)={let s=shared.read().unwrap();(s.autonomy.enabled(),s.autonomy.vision_active(),s.autonomy.epoch,s.vision.tracking(),s.vision.face_tracking())};
        if stop.load(Ordering::Acquire){if let Some(w)=worker.take(){w.stop().await;}let _=crate::camera::vision_lease(false,false).await;break;}
        if let Err(e)=crate::camera::vision_lease(active,enabled).await {shared.write().unwrap().vision.error=format!("摄像头：{e}");
            if let Some(w)=worker.take(){w.stop().await;}
            tokio::time::sleep(Duration::from_millis(200)).await;continue;
        }
        if !active {
            if let Some(w)=worker.take(){w.stop().await;}
            shared.write().unwrap().vision.suspend(epoch);
        } else if Instant::now()>=retry && last.elapsed()>=Duration::from_millis(if tracking{100}else{200}) {
            if worker.is_none(){match Worker::start(){Ok(w)=>worker=Some(w),Err(e)=>{shared.write().unwrap().vision.error=format!("视觉启动失败：{e}");retry=Instant::now()+Duration::from_secs(5);}}}
            if let Some(w)=worker.as_mut(){
                // Start-to-start cadence; do not queue catch-up requests.
                last=Instant::now();
                let cancelled=async {loop {tokio::time::sleep(Duration::from_millis(40)).await;
                    let s=shared.read().unwrap();if stop.load(Ordering::Acquire) || !s.autonomy.vision_active() || s.autonomy.epoch!=epoch{break;}}};
                let result=tokio::select!{r=tokio::time::timeout(Duration::from_secs(3),w.request(face_tracking))=>Some(r),_=cancelled=>None};
                let Some(result)=result else{if let Some(w)=worker.take(){w.stop().await;}continue;};
                let failure={let mut s=shared.write().unwrap();
                    if !s.autonomy.vision_active() || s.autonomy.epoch!=epoch{None}else{
                        match result {Ok(Ok(v))=>s.vision.ingest(&v,Instant::now(),epoch).err().map(|e|e.to_string()),Ok(Err(e))=>Some(e.to_string()),Err(_)=>Some("NPU处理超时".into())}}};
                if let Some(e)=failure{eprintln!("vision: {e}");shared.write().unwrap().vision.error=e;retry=Instant::now()+Duration::from_secs(5);if let Some(w)=worker.take(){w.stop().await;}}
            }
        }
        tokio::time::sleep(Duration::from_millis(if active{10}else{40})).await;
    }
});}

#[cfg(test)] mod tests {
    use super::*;
    fn frame(seq:u64,x:f64,face:bool)->Value {json!({"sequence":seq,"ageMs":0.,"boxes":[{"kind":0,"score":if face{0.9}else{0.8},"bounds":[x,100.,x+50.,200.]}]})}
    fn locked()->(Interaction,Instant) {let mut v=Interaction::default();let t=Instant::now();
        for i in 0..4{v.ingest(&frame(i+1,150.,true),t+Duration::from_millis(i*200),1).unwrap();}(v,t+Duration::from_millis(600))}
    #[test] fn observation_tracking_needs_fresh_person_and_discards_old_session(){
        let(mut v,t)=locked();assert!(v.tracking_pose(t,1).is_some());
        assert!(v.tracking_pose(t+Duration::from_millis(450),1).is_none());
        let(mut v,t)=locked();assert!(v.tracking_pose(t,2).is_none());
        let(mut v,t)=locked();v.suspend(1);assert!(v.tracking_pose(t,1).is_none());
    }
    #[test] fn obstacle_response_precedes_person_tracking_and_stale_vision_falls_back(){
        let(mut v,t)=locked();let head=[-7.,20.,3.];
        assert_eq!(v.observe_leisure(t,1,head,true,false),(head,None));
        let (tracking,voice)=v.observe_leisure(t,1,head,false,false);assert_ne!(tracking[1],head[1]);assert_eq!(tracking[2],head[2]);assert_eq!(voice,Some("single"));
        assert_eq!(v.observe_leisure(t+Duration::from_secs(1),1,head,false,false),(head,None));
        let(mut v,t)=locked();assert!(v.observe_leisure(t,1,head,false,true).1.is_none());
    }
    #[test] fn search_is_raised_bounded_accelerated_and_stops_on_camera_loss(){
        let mut v=Interaction::default();let t=Instant::now();let mut previous=0.;let mut max_yaw=0_f64;
        for i in 0..1500 {let now=t+Duration::from_millis(i*20);
            if i%5==0 {v.ingest(&json!({"sequence":i+1,"ageMs":0.,"boxes":[]}),now,1).unwrap();}
            let p=v.desired(now,1);assert_eq!(p[0],-10.);assert!(p[1].abs()<=32.);
            assert!((p[1]-previous).abs()<=0.72+1e-9);previous=p[1];max_yaw=max_yaw.max(p[1].abs());
            assert!((20. ..=34.).contains(&v.search.speed));
        }
        assert!(max_yaw>=11.);assert_eq!(v.desired(t+Duration::from_secs(31),1),[0.;2]);assert!(v.search.goal.is_none());
        v.suspend(2);assert_eq!(v.offset,[0.;2]);
    }
    #[test] fn face_alone_never_acquires_or_refreshes_a_person(){
        let(mut v,t)=locked();let seen=v.seen;
        let f=json!({"sequence":8,"ageMs":0.,"boxes":[{"kind":1,"score":0.99,"bounds":[155,105,180,130]}]});
        v.ingest(&f,t+Duration::from_millis(100),1).unwrap();assert_eq!(v.seen,seen);assert!(!v.face_tracking());
        let mut empty=Interaction::default();empty.ingest(&f,t,1).unwrap();assert!(empty.target.is_none());
    }
    #[test] fn search_retains_smooth_entry_from_tracking_limit_and_yields_to_body(){
        let mut v=Interaction::default();let t=Instant::now();
        v.ingest(&json!({"sequence":1,"ageMs":0.,"boxes":[]}),t,1).unwrap();v.offset=[-18.,35.];
        assert_eq!(v.desired(t,1),[-10.,35.]);
        let next=v.desired(t+Duration::from_millis(20),1);assert!((next[1]-35.).abs()<=0.72);
        v.ingest(&frame(2,150.,true),t+Duration::from_millis(100),1).unwrap();
        assert!(v.search.goal.is_none());let held=v.offset;
        assert_eq!(v.desired(t+Duration::from_millis(120),1),held);
    }
    #[test] fn stationary_body_refines_face_and_movement_or_missing_face_falls_back(){
        let(mut v,t)=locked();let mut f=frame(5,150.,true);
        f["boxes"].as_array_mut().unwrap().push(json!({"kind":1,"score":0.99,"bounds":[155,105,180,130]}));
        v.ingest(&f,t+Duration::from_millis(100),1).unwrap();assert!(v.target.unwrap().face);
        v.ingest(&frame(6,150.,true),t+Duration::from_millis(200),1).unwrap();assert!(!v.target.unwrap().face);
        f["sequence"]=json!(7);f["boxes"][0]["bounds"]=json!([170,100,220,200]);
        v.ingest(&f,t+Duration::from_millis(300),1).unwrap();assert!(!v.target.unwrap().face);assert!(!v.face_tracking());
    }
    #[test] fn close_up_partial_body_can_refine_its_face(){
        let mut v=Interaction::default();let t=Instant::now();
        for i in 0..4 {v.ingest(&json!({"sequence":i+1,"ageMs":0.,"boxes":[
            {"kind":0,"score":0.88,"bounds":[74,201,354,436]},
            {"kind":1,"score":0.99,"bounds":[170,238,274,380]}]}),t+Duration::from_millis(i*200),1).unwrap();}
        assert!(v.target.unwrap().face);
    }
    #[test] fn face_outside_upper_body_cannot_override_estimated_head(){
        let(mut v,t)=locked();let mut f=frame(5,150.,true);
        f["boxes"].as_array_mut().unwrap().extend([
            json!({"kind":1,"score":0.99,"bounds":[10,105,30,130]}),
            json!({"kind":1,"score":0.99,"bounds":[155,170,180,195]})]);
        v.ingest(&f,t+Duration::from_millis(100),1).unwrap();assert!(!v.target.unwrap().face);
        assert_eq!(v.target.unwrap().point(),v.body.unwrap().point());
    }
    #[test] fn body_lock_updates_fast_and_keeps_person_detector_enabled(){
        let(mut v,_)=locked();v.stable_since=None;v.target=Some(Target{face:false,score:0.9,bounds:[0.2,0.1,0.7,0.9]});
        assert!(v.tracking());assert!(!v.face_tracking());
        v.suspend(2);assert!(!v.tracking());assert!(!v.face_tracking());
    }
    #[test] fn moving_target_response_reverses_and_stays_bounded(){
        let(mut v,t)=locked();v.offset=[0.;2];
        for i in 1..=20 {let x=if i==1{220.}else if i==2{290.}else{326.67};v.ingest(&frame(i+4,x,true),t+Duration::from_millis(i*100),1).unwrap();}
        assert!(v.offset[1]< -30. && v.offset[1]>=-35.);let before=v.offset[1];
        for (i,x) in [266.,206.,146.,86.,50.,50.].iter().enumerate(){v.ingest(&frame(i as u64+25,*x,true),t+Duration::from_millis(2100+i as u64*100),1).unwrap();}
        assert!(v.offset[1]>before);assert!(v.offset[0].abs()<=18.);
    }
    #[test] fn acquisition_needs_half_second_of_distinct_fresh_frames(){let mut v=Interaction::default();let t=Instant::now();v.ingest(&frame(1,150.,true),t,1).unwrap();
        v.ingest(&frame(1,150.,true),t+Duration::from_secs(1),1).unwrap();assert!(!v.locked);
        v.ingest(&frame(2,150.,true),t+Duration::from_secs(2),1).unwrap();assert!(!v.locked);
        for i in 0..3{v.ingest(&frame(i+3,150.,true),t+Duration::from_millis(2200+i*200),1).unwrap();}assert!(v.locked);}
    #[test] fn retained_target_does_not_jump_to_a_different_person(){let(mut v,t)=locked();let old=v.target.unwrap().point();
        v.ingest(&frame(5,10.,true),t+Duration::from_millis(200),1).unwrap();assert_eq!(v.target.unwrap().point(),old);}
    #[test] fn lost_target_waits_two_seconds_then_raises_head_and_searches(){let(mut v,t)=locked();let held=v.offset;
        v.ingest(&json!({"sequence":5,"ageMs":0.,"boxes":[]}),t+Duration::from_millis(1800),1).unwrap();
        assert_eq!(v.desired(t+Duration::from_millis(1800),1),held);
        v.ingest(&json!({"sequence":6,"ageMs":0.,"boxes":[]}),t+Duration::from_millis(2100),1).unwrap();
        assert_eq!(v.desired(t+Duration::from_millis(2100),1),[-10.,held[1]]);assert!(!v.locked);
        v.ingest(&json!({"sequence":7,"ageMs":0.,"boxes":[]}),t+Duration::from_millis(5000),1).unwrap();
        assert_eq!(v.desired(t+Duration::from_millis(5000),1)[0],-10.);}
    #[test] fn stalled_camera_has_no_search_or_tracking_motion(){let(mut v,t)=locked();assert_eq!(v.desired(t+Duration::from_millis(601),1),[0.;2]);assert!(v.call(t+Duration::from_millis(601),false).is_none());}
    #[test] fn manual_epoch_discards_old_tracking_and_needs_reacquisition(){let(mut v,t)=locked();assert_eq!(v.desired(t,2),[0.;2]);assert!(v.target.is_none());assert!(!v.locked);}
    #[test] fn greetings_are_bounded_and_busy_audio_never_replaced(){let(mut v,t)=locked();assert!(v.call(t,true).is_none());assert_eq!(v.call(t,false),Some("single"));assert!(v.call(t,false).is_none());
        for n in 1..=140 {let now=t+Duration::from_millis(n*200);v.ingest(&frame(n+4,150.,true),now,1).unwrap();if n<140{assert!(v.call(now,false).is_none());}else{assert_eq!(v.call(now,false),Some("single"));}}
    }
    #[test] fn parser_rejects_stale_or_invalid_untrusted_results(){for bad in [json!({}),json!({"sequence":1,"ageMs":501.,"boxes":[]}),json!({"sequence":1,"ageMs":0.,"boxes":[{"kind":1,"score":0.9,"bounds":[-1,0,100,100]}]}),json!({"sequence":1,"ageMs":0.,"boxes":[{"kind":4,"score":0.9,"bounds":[0,0,100,100]}]})] {assert!(Interaction::default().ingest(&bad,Instant::now(),1).is_err());}}
    #[test] fn head_slew_and_existing_mechanical_limits_are_preserved(){let mut o=[0.;2];step(&mut o,[18.,-35.],[0.;2],[(-90.,90.),(-170.,170.)]);assert_eq!(o,[0.72,-0.72]);
        for _ in 0..300{step(&mut o,[18.,-35.],[8.,-8.],[(-10.,10.),(-10.,10.)]);}assert_eq!(o,[2.,-2.]);}
    #[test] fn body_anchor_can_handoff_to_its_face_but_not_a_bystander(){let body=Target{face:false,score:0.9,bounds:[0.2,0.1,0.7,0.9]};assert!(body.related(Target{face:true,score:0.9,bounds:[0.3,0.15,0.4,0.25]}));assert!(!body.related(Target{face:true,score:0.9,bounds:[0.8,0.15,0.9,0.25]}));}
}
