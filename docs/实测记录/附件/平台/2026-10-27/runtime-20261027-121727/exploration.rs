//! Bounded, head-mounted ToF exploration. Produces intents, never touches UART.
use std::time::{Duration, Instant};
use rand::Rng;
use serde_json::{json,Value};
use crate::telemetry::Telemetry;

const STOP_MM:f64=450.;
const CLEAR_MM:f64=700.;
// Match the standard (non-boosted) joystick at full deflection.
const FORWARD_SPEED:f32=0.2;
const TURN_SPEED:f32=1.0;
const LOOK:f64=25.;
const LOOK_UP:f64=-10.;
// Internal ordering value only; no-return is never reported as a measured distance.
const NO_TARGET:f64=4001.;
// User-requested exclusions: fixed near returns and the bottom two displayed rows.
// Zero-based raw sensor indices, before any display rotation. Not free-space evidence.
const EXCLUDED_ZONES:[usize;24]=[16,24,25,32,33,34,40,41,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63];
const OBSTACLE_CONFIRM:Duration=Duration::from_millis(500);
#[derive(Default)]
struct NearFilter {since:Option<Instant>,last:Option<(u64,Instant)>,confirmed:bool}
impl NearFilter {
    fn observe(&mut self,now:Instant,range:Option<Range>,threshold:f64,inclusive:bool)->bool {
        let Some(r)=range.filter(|r|r.mm.is_finite() && (0. ..=NO_TARGET).contains(&r.mm)) else {*self=Self::default();return false;};
        if self.last.is_some_and(|(_,at)|now.saturating_duration_since(at)>Duration::from_millis(200)){*self=Self::default();}
        if self.last.is_some_and(|(seq,_)|seq==r.seq){return self.confirmed;}
        self.last=Some((r.seq,now));
        if r.mm<threshold || (inclusive && r.mm==threshold) {
            let since=*self.since.get_or_insert(now);self.confirmed=now.saturating_duration_since(since)>=OBSTACLE_CONFIRM;
        }else{self.since=None;self.confirmed=false;}
        self.confirmed
    }
}
#[derive(Default,Clone,Copy,Debug,PartialEq)]
enum Phase {#[default] Observe, Forward, Stop, Left, Right, Center, Turn, Retreat, Blocked}
#[derive(Default)]
pub struct Explorer {
    pub selected:bool,
    phase:Phase, epoch:u64, since:Option<Instant>, settled:Option<Instant>,
    last_frame:u64, samples:u8, nearest:Option<f64>, left:Option<f64>, right:Option<f64>,
    direction:f32, turn_yaw:f64, attempts:u8, forward_since:Option<Instant>,
    pub head:[f64;2], pub twist:[f32;3], pub reason:String, pub distance:Option<f64>,
    pub sound:bool,
    no_target:bool,
    forward_seconds:f64, turn_degrees:f64, retreat_seconds:f64, retreat_available:bool,
    travel_yaw:f64,
    forward_near:NearFilter, turn_near:NearFilter,
    event:Option<(&'static str,Instant)>, last_call:Option<Instant>, next_chatter:Option<Instant>,
    departed:bool, pub last_voice:Option<&'static str>,
}
#[derive(Clone,Copy)]
pub struct Range {pub seq:u64,pub mm:f64}
pub struct Input {pub active:bool,pub epoch:u64,pub camera:bool,pub range:Option<Range>,pub head:[f64;2],pub still:bool,pub yaw:f64}
// Fresh transport and usable distance are separate. No-target/weak far returns
// permit bounded exploration, not a claim of verified free space. Near ambiguous
// returns still stop movement; sensor failures and malformed frames remain blocked.
pub fn range(state:&Telemetry)->Option<Range> {
    if state.tof["enabled"]!=true || state.tof["state"]!="streaming" {return None;}
    let sample=state.stamp(state.latest.get("tof")?);
    if sample["valid"]!=true || !(0. ..=200.).contains(&sample["ageMs"].as_f64()?) {return None;}
    let data=&sample["data"];
    if data["captureClockVerified"]!=true{return None;}
    let distances=data["distanceMm"].as_array()?;let statuses=data["status"].as_array()?;
    if distances.len()!=64 || statuses.len()!=64{return None;}
    let mut minimum=NO_TARGET;
    for (zone,(d,s)) in distances.iter().zip(statuses).enumerate() {
        if EXCLUDED_ZONES.contains(&zone){continue;}
        let d=d.as_f64()?;let status=s.as_u64()?;
        if !d.is_finite() || !(0. ..=65535.).contains(&d){return None;}
        match status {
            0=>{}, // No usable target; never use its distance payload.
            5|6|9=>{if d<=4000. {minimum=minimum.min(d);}},
            3|4|12|13=>{if d>0. && d<=CLEAR_MM {minimum=minimum.min(d);}},
            _=>return None, // Sensor/processing error or unknown status.
        }
    }
    Some(Range{seq:sample["seq"].as_u64()?,mm:minimum})
}
impl Explorer {
    pub fn reset(&mut self){let (selected,last_call,sound)=(self.selected,self.last_call,self.sound);*self=Self::default();self.selected=selected;self.last_call=last_call;self.sound=sound;}
    fn enter(&mut self,phase:Phase,now:Instant){
        let voice=match phase {
            Phase::Forward=>{let v=if self.departed {"explore_clear"}else{"explore_start"};self.departed=true;Some(v)},
            Phase::Stop=>Some("explore_obstacle"),Phase::Blocked=>Some("explore_blocked"),_=>None
        };
        if let Some(voice)=voice{self.event=Some((voice,now));}
        if phase==Phase::Forward {self.forward_seconds=rand::thread_rng().gen_range(3. ..=10.);}
        if phase==Phase::Turn {self.turn_degrees=rand::thread_rng().gen_range(15. ..=30.);}
        if phase==Phase::Retreat {self.retreat_seconds=rand::thread_rng().gen_range(0.4 ..=0.7);}
        self.forward_near=NearFilter::default();self.turn_near=NearFilter::default();
        self.phase=phase;self.since=Some(now);self.settled=None;self.samples=0;self.nearest=None;self.twist=[0.;3];}
    fn block(&mut self,now:Instant,reason:&str){self.enter(Phase::Blocked,now);self.reason=reason.into();self.head=[0.;2];}
    pub fn tick(&mut self,now:Instant,i:Input)->[f32;3] {
        self.twist=[0.;3];
        if !self.selected || !i.active {self.reset();return self.twist;}
        if self.epoch!=i.epoch {self.reset();self.epoch=i.epoch;}
        self.since.get_or_insert(now);
        if self.next_chatter.is_none(){self.schedule_chatter(now);}
        self.no_target=i.range.is_some_and(|r|r.mm==NO_TARGET);
        self.distance=i.range.filter(|r|r.mm!=NO_TARGET).map(|r|r.mm);
        if self.phase==Phase::Blocked {
            if self.reason=="前方和左右暂无通路，稍后重新扫描" && self.since.is_some_and(|t|now.saturating_duration_since(t)>=Duration::from_secs(3)) {
                self.enter(Phase::Observe,now);self.reason.clear();
            }else{return self.twist;}
        }
        if !i.camera || i.range.is_none_or(|r|!r.mm.is_finite() || !(0. ..=NO_TARGET).contains(&r.mm)) || !i.yaw.is_finite() || i.head.iter().any(|v|!v.is_finite()) {
            self.enter(Phase::Observe,now);self.event=None;self.head=[LOOK_UP,0.];self.forward_since=None;self.retreat_available=false;
            self.reason=if !i.camera{"等待新鲜摄像头画面"}else{"等待新鲜ToF数据或采集异常恢复"}.into();return self.twist;
        }
        let r=i.range.unwrap();
        let obstacle=self.forward_near.observe(now,Some(r),STOP_MM,true);
        let turn_obstacle=self.turn_near.observe(now,Some(r),250.,false);
        let new_frame=r.seq!=self.last_frame;self.last_frame=r.seq;
        self.head=[LOOK_UP,match self.phase {Phase::Left=>LOOK,Phase::Right=>-LOOK,_=>0.}];
        let aligned=i.head.iter().zip(self.head).all(|(a,b)|(a-b).abs()<3.);
        let elapsed=now.saturating_duration_since(self.since.unwrap()).as_secs_f64();
        let facing=i.head.iter().zip(self.head).all(|(a,b)|(a-b).abs()<12.);
        if self.phase==Phase::Retreat {
            let delta=(i.yaw-self.travel_yaw).sin().atan2((i.yaw-self.travel_yaw).cos()).abs();
            if !facing || delta>10f64.to_radians() || elapsed>=self.retreat_seconds {
                self.retreat_available=false;self.enter(Phase::Observe,now);
            }else{self.twist[0]=-0.1;}
            return self.twist;
        }
        if self.phase==Phase::Forward {
            if !facing || obstacle {
                self.retreat_available=facing && obstacle && self.forward_since.is_some_and(|t|now.saturating_duration_since(t)>=Duration::from_secs(1))
                    && (i.yaw-self.travel_yaw).sin().atan2((i.yaw-self.travel_yaw).cos()).abs()<10f64.to_radians();
                self.reason=if !facing {"行走中头部朝向偏差超过12度".into()}else{format!("前方ToF距离{:.0}mm，连续遮挡至少500ms，触发450mm停步",r.mm)};
                self.enter(Phase::Stop,now);self.forward_since=None;return self.twist;
            }
            if self.forward_since.is_none(){self.forward_since=Some(now);self.travel_yaw=i.yaw;}
            if self.forward_since.is_some_and(|t|now.saturating_duration_since(t).as_secs_f64()>=self.forward_seconds) {
                self.attempts=0;self.forward_since=None;self.enter(Phase::Observe,now);
                self.reason="本段随机前进结束，抬头重新扫描".into();return self.twist;
            }
            self.twist[0]=FORWARD_SPEED;
            self.reason.clear();return self.twist;
        }
        if self.phase==Phase::Turn {
            if !facing {self.block(now,"转向时头部朝向偏差超过12度，等待人工接管");return self.twist;}
            if turn_obstacle {self.block(now,"转向距离小于250mm持续500ms，等待人工接管");return self.twist;}
            let delta=(i.yaw-self.turn_yaw).sin().atan2((i.yaw-self.turn_yaw).cos());
            if delta.abs()>45f64.to_radians(){self.block(now,"转向偏离目标，等待人工接管");}
            else if delta*self.direction as f64>=self.turn_degrees.to_radians(){self.enter(Phase::Observe,now);}
            else if elapsed>=3. {self.block(now,"转向超时，等待人工接管");}
            else {self.twist[2]=self.direction*TURN_SPEED;}
            return self.twist;
        }
        if elapsed>6. {self.block(now,"未能确认头部到位或站稳，等待人工接管");return self.twist;}
        if !aligned || !i.still {self.settled=None;self.samples=0;self.nearest=None;return self.twist;}
        let settled=*self.settled.get_or_insert(now);
        // Let pre-settle ToF frames age out; count distinct frames only.
        if now.saturating_duration_since(settled)<Duration::from_millis(350) || !new_frame{return self.twist;}
        self.samples=self.samples.saturating_add(1);self.nearest=Some(self.nearest.unwrap_or(r.mm).min(r.mm));
        if self.samples<3{return self.twist;}
        let distance=self.nearest.unwrap();
        match self.phase {
            Phase::Observe=>{self.reason="抬头扫描左右通路".into();self.left=None;self.right=None;self.enter(Phase::Left,now);},
            Phase::Stop=>{if self.retreat_available && elapsed<2. && rand::thread_rng().gen_bool(0.5) {
                self.retreat_available=false;self.reason="沿刚走过的方向短退，再重新扫描".into();self.enter(Phase::Retreat,now);
            }else if self.attempts>=3{self.block(now,"连续三次未找到通路，等待人工接管");}else{self.left=None;self.right=None;self.enter(Phase::Left,now);}},
            Phase::Left=>{self.left=Some(distance);self.enter(Phase::Right,now);},
            Phase::Right=>{self.right=Some(distance);self.direction=match (self.left.unwrap_or(0.)>=CLEAR_MM,distance>=CLEAR_MM){(true,true)=>if rand::thread_rng().gen_bool(0.5){1.}else{-1.},(true,false)=>1.,_=>-1.};self.enter(Phase::Center,now);},
            Phase::Center=>{
                if distance>=CLEAR_MM {self.reason.clear();self.forward_since=None;self.enter(Phase::Forward,now);}
                else if self.left.unwrap_or(0.).max(self.right.unwrap_or(0.))>=CLEAR_MM {
                    self.turn_yaw=i.yaw;self.attempts+=1;self.enter(Phase::Turn,now);
                }else{self.block(now,"前方和左右暂无通路，稍后重新扫描");}
            },
            _=>{}
        }
        self.twist
    }
    pub fn permits_motion(&mut self,now:Instant,epoch:u64,range:Option<Range>,camera:bool,command:[f32;3])->bool{
        if !self.selected || self.epoch!=epoch{return false;}
        if !camera || range.is_none_or(|r|!r.mm.is_finite() || !(0. ..=NO_TARGET).contains(&r.mm)) {
            self.forward_near=NearFilter::default();self.turn_near=NearFilter::default();
            return command==[0.;3];
        }
        let forward=self.forward_near.observe(now,range,STOP_MM,true);
        let turn=self.turn_near.observe(now,range,250.,false);
        if command==[0.;3]{return true;}
        if command[0]<0. && self.phase!=Phase::Retreat{return false;}
        (command[0]<=0. || !forward) && (command[2]==0. || !turn)
    }
    fn schedule_chatter(&mut self,now:Instant){use rand::Rng;self.next_chatter=Some(now+Duration::from_secs(rand::thread_rng().gen_range(15..=30)));}
    /// One bounded event slot; never delay movement or queue a backlog of calls.
    pub fn call(&mut self,now:Instant,busy:bool)->Option<&'static str>{
        if !self.selected || self.since.is_none(){self.event=None;return None;}
        if self.event.is_some_and(|(_,at)|now.saturating_duration_since(at)>Duration::from_secs(2)){self.event=None;}
        if busy{return None;}
        let event=self.event.map(|(v,_)|v);
        let cooldown=if event==Some("explore_blocked"){2}else{4};
        if self.last_call.is_some_and(|at|now.saturating_duration_since(at)<Duration::from_secs(cooldown)){return None;}
        let voice=if let Some(v)=event{self.event=None;v}
            else if self.phase==Phase::Forward && self.next_chatter.is_some_and(|t|now>=t){"explore_idle"}
            else{return None;};
        self.last_call=Some(now);self.last_voice=Some(voice);self.sound=true;self.schedule_chatter(now);Some(voice)
    }
    pub fn status(&self)->Value {json!({"excludedTofZones":EXCLUDED_ZONES,"requiredTofZones":64-EXCLUDED_ZONES.len(),"lastVoice":self.last_voice,"selected":self.selected,"behavior":match self.phase{Phase::Observe=>"抬头观察",Phase::Forward=>"缓慢前进",Phase::Stop=>"障碍停步",Phase::Left=>"向左测距",Phase::Right=>"向右测距",Phase::Center=>"头部回正",Phase::Turn=>"随机转向",Phase::Retreat=>"短退后重新扫描",Phase::Blocked=>"等待人工接管"},"reason":self.reason,"distanceMm":self.distance,"rangeAssessment":if self.no_target{"no-target"}else if self.distance.is_some(){"target"}else{"unavailable"},"leftMm":self.left.filter(|v|*v!=NO_TARGET),"rightMm":self.right.filter(|v|*v!=NO_TARGET),"leftNoTarget":self.left==Some(NO_TARGET),"rightNoTarget":self.right==Some(NO_TARGET),"forwardBurstSeconds":self.forward_seconds,"forwardBurstRangeSeconds":[3,10],"turnDegrees":self.turn_degrees,"retreatSeconds":self.retreat_seconds,"headDegrees":self.head,"commandTwist":self.twist,"attempts":self.attempts,"obstacleConfirmMs":500,"obstacleConfirmed":self.forward_near.confirmed,"turnObstacleConfirmed":self.turn_near.confirmed,"stopMm":STOP_MM,"clearMm":CLEAR_MM})}
}

fn path()->std::path::PathBuf {std::path::PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE").unwrap_or("./state/calibration.json".into())).with_file_name("autonomy.json")}
fn selected(v:&Value)->anyhow::Result<bool>{anyhow::ensure!(v.as_object().is_some_and(|o|o.len()==1),"仅接受 mode");match v["mode"].as_str(){Some("interaction")=>Ok(false),Some("explore")=>Ok(true),_=>anyhow::bail!("无效自主模式")}}
pub fn init(shared:&crate::telemetry::Shared){match std::fs::read(path()){Ok(b)=>match serde_json::from_slice(&b).map_err(anyhow::Error::from).and_then(|v|selected(&v)){Ok(v)=>shared.write().unwrap().autonomy.explorer.selected=v,Err(e)=>shared.write().unwrap().log("ERROR","autonomy",&format!("自主偏好读取失败：{e}"))},Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>shared.write().unwrap().log("ERROR","autonomy",&format!("自主偏好读取失败：{e}"))}}
pub fn router()->axum::Router<crate::App>{axum::Router::new().route("/api/v1/autonomy/settings",axum::routing::get(get).post(set))}
async fn get(axum::extract::State(a):axum::extract::State<crate::App>)->axum::Json<Value>{axum::Json(json!({"mode":if a.telemetry.read().unwrap().autonomy.explorer.selected{"explore"}else{"interaction"}}))}
async fn set(axum::extract::State(a):axum::extract::State<crate::App>,headers:axum::http::HeaderMap,body:axum::body::Bytes)->axum::response::Response {
    use axum::{http::StatusCode,response::IntoResponse};
    let v=match crate::check_write(&headers,&body,256){Ok(v)=>v,Err(e)=>return e};
    let choice=match selected(&v){Ok(v)=>v,Err(e)=>return crate::error(StatusCode::UNPROCESSABLE_ENTITY,&e.to_string())};
    let mut s=a.telemetry.write().unwrap();
    if s.autonomy.enabled(){return crate::error(StatusCode::CONFLICT,"请先关闭自主模式再选择行为");}
    let save=||->anyhow::Result<()> {use std::io::Write;let p=path();std::fs::create_dir_all(p.parent().unwrap())?;let temp=p.with_extension("tmp");let mut f=std::fs::File::create(&temp)?;write!(f,"{v}")?;f.sync_all()?;drop(f);std::fs::rename(temp,p)?;Ok(())};
    if let Err(e)=save(){return crate::error(StatusCode::INTERNAL_SERVER_ERROR,&format!("保存失败：{e}"));}
    s.autonomy.explorer.reset();s.autonomy.explorer.selected=choice;
    axum::Json(v).into_response()
}

#[cfg(test)] mod tests {
    use super::*;
    fn input(seq:u64,mm:f64)->Input {Input{active:true,epoch:1,camera:true,range:Some(Range{seq,mm}),head:[LOOK_UP,0.],still:true,yaw:0.}}
    fn run(e:&mut Explorer,t:Instant,ms:u64,seq:u64,mm:f64)->[f32;3]{let mut i=input(seq,mm);i.head=e.head;e.tick(t+Duration::from_millis(ms),i)}
    fn explorer()->Explorer{Explorer{selected:true,epoch:1,..Default::default()}}
    fn settle(e:&mut Explorer,t:Instant,mm:f64){let phase=e.phase;for n in 0..12{run(e,t,n*100,n+100,mm);if e.phase!=phase{break;}}}
    #[test] fn unknown_stale_partial_or_near_range_never_means_clear(){
        let mut s=Telemetry::new(false);s.started=Instant::now()-Duration::from_secs(1);s.tof=json!({"enabled":true,"state":"streaming"});
        let data=json!({"distanceMm":vec![1000;64],"status":vec![5;64],"captureClockVerified":true});
        s.sample("tof",data.clone(),true,Instant::now());assert_eq!(range(&s).unwrap().mm,1000.);
        let mut invalid=data.clone();invalid["status"][17]=json!(255);s.sample("tof",invalid,true,Instant::now());assert!(range(&s).is_none());
        s.sample("tof",data.clone(),true,Instant::now()-Duration::from_millis(250));assert!(range(&s).is_none());
        let mut d=data.clone();d["captureClockVerified"]=json!(false);s.sample("tof",d,true,Instant::now());assert!(range(&s).is_none());
        s.sample("tof",data,false,Instant::now());assert!(range(&s).is_none());
    }
    #[test] fn explicit_mask_ignores_only_fixed_returns_and_bottom_two_rows(){
        let mut s=Telemetry::new(false);s.tof=json!({"enabled":true,"state":"streaming"});
        let mut data=json!({"distanceMm":vec![1000;64],"status":vec![5;64],"captureClockVerified":true});
        for i in EXCLUDED_ZONES{data["distanceMm"][i]=json!(3);data["status"][i]=json!(0);}
        s.sample("tof",data.clone(),true,Instant::now());assert_eq!(range(&s).unwrap().mm,1000.);
        for i in 0..64{if EXCLUDED_ZONES.contains(&i){continue;}
            let mut bad=data.clone();bad["status"][i]=json!(255);s.sample("tof",bad,true,Instant::now());assert!(range(&s).is_none(),"zone {i} must remain required");
            let mut near=data.clone();near["distanceMm"][i]=json!(300);s.sample("tof",near,true,Instant::now());assert_eq!(range(&s).unwrap().mm,300.);
        }
        assert_eq!(Explorer::default().status()["requiredTofZones"],40);
        assert_eq!(&EXCLUDED_ZONES[8..], &(48usize..64).collect::<Vec<_>>()[..]);
    }
    #[test] fn repeated_frames_cannot_authorize_start(){let mut e=explorer();let t=Instant::now();for n in 0..30{assert_eq!(run(&mut e,t,n*100,1,1000.),[0.;3]);}assert_eq!(e.phase,Phase::Observe);}
    #[test] fn standard_stick_forward_does_not_shrink_before_stop_boundary(){
        let mut e=explorer();let t=Instant::now();e.enter(Phase::Forward,t);e.head=[LOOK_UP,0.];
        assert_eq!(e.phase,Phase::Forward);
        for (n,mm) in [1000.,700.,600.,451.].into_iter().enumerate(){
            assert_eq!(run(&mut e,t,1300+n as u64*100,200+n as u64,mm),[0.2,0.,0.]);
        }
        for n in 0..5{assert_eq!(run(&mut e,t,1800+n*100,210+n,450.),[0.2,0.,0.]);}
        assert_eq!(run(&mut e,t,2300,215,450.),[0.;3]);assert_eq!(e.phase,Phase::Stop);
    }
    #[test] fn standard_stick_turn_both_directions_stops_at_measured_angle(){
        let t=Instant::now();
        for direction in [-1.,1.]{
            let mut e=explorer();e.enter(Phase::Turn,t);e.turn_degrees=20.;e.direction=direction;
            assert_eq!(e.tick(t,input(1,1000.)),[0.,0.,direction]);
            let mut i=input(2,1000.);i.yaw=direction as f64*21f64.to_radians();
            assert_eq!(e.tick(t+Duration::from_millis(400),i),[0.;3]);assert_eq!(e.phase,Phase::Observe);
        }
    }
    #[test] fn camera_loss_sensor_loss_takeover_and_epoch_clear_motion(){
        let t=Instant::now();for fault in 0..4{let mut e=explorer();e.enter(Phase::Forward,t);let mut i=input(200,1000.);match fault{0=>i.camera=false,1=>i.range=None,2=>i.active=false,_=>i.epoch=2};assert_eq!(e.tick(t+Duration::from_secs(2),i),[0.;3]);assert_eq!(e.phase,Phase::Observe);}
    }
    #[test] fn scan_requires_measured_head_arrival_and_returns_center_before_turn(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Left,t);
        for n in 0..10{assert_eq!(e.tick(t+Duration::from_millis(n*100),input(n+1,1000.)),[0.;3]);}assert_eq!(e.phase,Phase::Left);
        settle(&mut e,t+Duration::from_secs(1),1000.);assert_eq!(e.phase,Phase::Right);
        settle(&mut e,t+Duration::from_secs(3),600.);assert_eq!(e.phase,Phase::Center);
        settle(&mut e,t+Duration::from_secs(5),400.);assert_eq!(e.phase,Phase::Turn);
        let mut i=input(999,400.);i.head=[LOOK_UP,0.];let out=e.tick(t+Duration::from_millis(6200),i);assert_eq!(out,[0.,0.,1.]);
        let mut i=input(1000,800.);i.yaw=31f64.to_radians();assert_eq!(e.tick(t+Duration::from_millis(6300),i),[0.;3]);assert_eq!(e.phase,Phase::Observe);
    }
    #[test] fn blocked_is_latched_and_announcement_cannot_restart(){let mut e=explorer();let t=Instant::now();e.block(t,"blocked");let mut i=input(1,1000.);i.camera=false;assert_eq!(e.tick(t,i),[0.;3]);assert_eq!(e.phase,Phase::Blocked);let mut i=input(2,1000.);i.active=false;e.tick(t,i);assert_eq!(e.phase,Phase::Observe);}
    #[test] fn turn_timeout_obstacle_and_attempt_budget_stop(){let t=Instant::now();for case in 0..3{let mut e=explorer();e.enter(Phase::Turn,t);e.direction=1.;let mut i=input(1,1000.);if case==1{i.range=Some(Range{seq:1,mm:200.});}if case==2{i.yaw=-50f64.to_radians();}assert_eq!(e.tick(t+Duration::from_secs(4),i),[0.;3]);assert_eq!(e.phase,Phase::Blocked);}let mut e=explorer();e.attempts=3;e.enter(Phase::Stop,t);settle(&mut e,t,400.);assert_eq!(e.phase,Phase::Blocked);}
    #[test] fn context_calls_are_bounded_and_do_not_change_motion(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);
        assert_eq!(e.call(t,false),Some("explore_start"));
        assert!(e.sound);assert_eq!(e.call(t,false),None);
        e.enter(Phase::Stop,t+Duration::from_secs(5));
        assert_eq!(e.call(t+Duration::from_secs(5),true),None);
        assert_eq!(e.call(t+Duration::from_secs(6),false),Some("explore_obstacle"));
        assert_eq!(e.twist,[0.;3]);
        e.enter(Phase::Forward,t+Duration::from_secs(11));
        assert_eq!(e.call(t+Duration::from_secs(11),false),Some("explore_clear"));
        e.next_chatter=Some(t+Duration::from_secs(30));
        assert_eq!(e.call(t+Duration::from_secs(29),false),None);
        assert_eq!(e.call(t+Duration::from_secs(30),false),Some("explore_idle"));
        e.block(t+Duration::from_secs(35),"blocked");
        assert_eq!(e.call(t+Duration::from_secs(35),false),Some("explore_blocked"));
        assert_eq!(e.call(t+Duration::from_secs(100),false),None);
    }
    #[test] fn old_calls_do_not_play_after_takeover_or_busy_audio(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);
        assert_eq!(e.call(t,true),None);assert_eq!(e.call(t+Duration::from_secs(3),false),None);
        e.enter(Phase::Stop,t);e.reset();assert_eq!(e.call(t,false),None);
        let mut s=Telemetry::new(false);s.autonomy.explorer.sound=true;s.pending_quack=true;s.quack_running=true;
        s.cancel_vision_sound();assert!(s.quack_cancel);assert!(!s.pending_quack && !s.quack_running && !s.autonomy.explorer.sound);
    }
    #[test] fn obstacle_arriving_during_inference_invalidates_old_forward_output(){
        let mut e=explorer();let clear=Some(Range{seq:1,mm:1000.});let close=Some(Range{seq:2,mm:400.});
        assert!(e.permits_motion(Instant::now(),1,clear,true,[0.2,0.,0.]));
        assert!(e.permits_motion(Instant::now(),1,close,true,[0.2,0.,0.]));
        assert!(!e.permits_motion(Instant::now(),2,clear,true,[0.2,0.,0.]));
        assert!(!e.permits_motion(Instant::now(),1,None,true,[0.,0.,1.]));
        assert!(!e.permits_motion(Instant::now(),1,clear,false,[0.2,0.,0.]));
        assert!(e.permits_motion(Instant::now(),1,None,false,[0.;3]));
    }
    #[test] fn no_target_frame_is_distinct_from_stale_or_broken_capture(){
        let mut s=Telemetry::new(false);s.started=Instant::now()-Duration::from_secs(1);s.tof=json!({"enabled":true,"state":"streaming"});
        let data=json!({"distanceMm":vec![8;64],"status":vec![0;64],"captureClockVerified":true});
        s.sample("tof",data.clone(),true,Instant::now());assert_eq!(range(&s).unwrap().mm,NO_TARGET);
        let mut near=data.clone();near["status"][17]=json!(5);near["distanceMm"][17]=json!(300);
        s.sample("tof",near.clone(),true,Instant::now());assert_eq!(range(&s).unwrap().mm,300.);
        near["status"][17]=json!(4);s.sample("tof",near,true,Instant::now());assert_eq!(range(&s).unwrap().mm,300.);
        let mut bad=data.clone();bad["status"][17]=json!(1);s.sample("tof",bad,true,Instant::now());assert!(range(&s).is_none());
        s.sample("tof",data.clone(),true,Instant::now()-Duration::from_millis(250));assert!(range(&s).is_none());
        s.sample("tof",data,true,Instant::now());s.tof["enabled"]=json!(false);assert!(range(&s).is_none());
    }
    #[test] fn empty_scene_scans_both_sides_before_bounded_forward_burst(){
        let mut e=explorer();let t=Instant::now();
        settle(&mut e,t,NO_TARGET);assert_eq!(e.phase,Phase::Left);assert_eq!(e.twist,[0.;3]);
        settle(&mut e,t+Duration::from_secs(2),NO_TARGET);assert_eq!(e.phase,Phase::Right);
        settle(&mut e,t+Duration::from_secs(4),NO_TARGET);assert_eq!(e.phase,Phase::Center);
        settle(&mut e,t+Duration::from_secs(6),NO_TARGET);assert_eq!(e.phase,Phase::Forward);
        assert_eq!(run(&mut e,t,8000,1000,NO_TARGET),[0.2,0.,0.]);
        assert_eq!(e.head,[LOOK_UP,0.]);assert!(e.status()["distanceMm"].is_null());
        assert_eq!(e.status()["rangeAssessment"],"no-target");
        assert_eq!(run(&mut e,t,18001,1001,NO_TARGET),[0.;3]);assert_eq!(e.phase,Phase::Observe);
        assert!(!e.permits_motion(Instant::now(),1,None,true,[0.2,0.,0.]));
        assert!(e.permits_motion(Instant::now(),1,Some(Range{seq:1002,mm:300.}),true,[0.2,0.,0.]));
    }
    #[test] fn blocked_scene_retries_scan_but_turn_fault_stays_latched(){
        let mut e=explorer();let t=Instant::now();e.left=Some(300.);e.right=Some(300.);e.enter(Phase::Center,t);
        settle(&mut e,t,300.);assert_eq!(e.phase,Phase::Blocked);
        assert_eq!(run(&mut e,t,5000,300,NO_TARGET),[0.;3]);assert_eq!(e.phase,Phase::Observe);
    }
    #[test] fn random_bursts_last_three_to_ten_seconds_and_still_stop_for_obstacles(){
        let t=Instant::now();let mut durations=std::collections::BTreeSet::new();
        for _ in 0..50 {
            let mut e=explorer();e.enter(Phase::Forward,t);e.head=[LOOK_UP,0.];durations.insert((e.forward_seconds*1000.) as u64);
            assert!((3. ..=10.).contains(&e.forward_seconds));e.forward_seconds=10.;
            assert_eq!(run(&mut e,t,0,1,1000.),[0.2,0.,0.]);
            let mut i=input(2,1000.);i.head=[LOOK_UP+5.,4.];
            assert_eq!(e.tick(t+Duration::from_millis(2900),i),[0.2,0.,0.]);
            for n in 0..5 {assert_eq!(run(&mut e,t,2950+n*100,100+n,400.),[0.2,0.,0.]);}
            assert_eq!(run(&mut e,t,3450,105,400.),[0.;3]);assert_eq!(e.phase,Phase::Stop);assert!(e.retreat_available);
        }
        assert!(durations.len()>1);
    }
    #[test] fn retreat_is_short_and_sensor_loss_or_heading_change_stops_it(){
        let t=Instant::now();
        for fault in 0..3 {
            let mut e=explorer();e.enter(Phase::Retreat,t);e.head=[LOOK_UP,0.];
            assert!((0.4 ..=0.7).contains(&e.retreat_seconds));
            assert_eq!(run(&mut e,t,0,1,300.),[-0.1,0.,0.]);
            let mut i=input(2,300.);i.head=e.head;
            match fault {0=>i.range=None,1=>i.yaw=12f64.to_radians(),_=>{}}
            assert_eq!(e.tick(t+Duration::from_millis(if fault==2 {701}else{100}),i),[0.;3]);
            assert_eq!(e.phase,Phase::Observe);
        }
        let mut e=explorer();e.enter(Phase::Forward,t);e.head=[LOOK_UP,0.];run(&mut e,t,0,1,1000.);run(&mut e,t,100,2,300.);assert!(!e.retreat_available);
    }
    #[test] fn obstacle_filter_requires_half_second_of_fresh_continuous_returns(){
        let t=Instant::now();let mut f=NearFilter::default();
        for n in 0..5 {assert!(!f.observe(t+Duration::from_millis(n*100),Some(Range{seq:n,mm:5.}),450.,true));}
        assert!(!f.observe(t+Duration::from_millis(499),Some(Range{seq:5,mm:5.}),450.,true));
        assert!(f.observe(t+Duration::from_millis(500),Some(Range{seq:6,mm:5.}),450.,true));
        assert!(!f.observe(t+Duration::from_millis(510),Some(Range{seq:7,mm:1000.}),450.,true));
        assert!(!f.observe(t+Duration::from_millis(520),Some(Range{seq:8,mm:5.}),450.,true));
        assert!(!f.observe(t+Duration::from_millis(1100),Some(Range{seq:9,mm:5.}),450.,true));
        assert!(!f.observe(t+Duration::from_millis(1200),None,450.,true));
        let mut f=NearFilter::default();for n in 0..20 {assert!(!f.observe(t+Duration::from_millis(n*50),Some(Range{seq:1,mm:5.}),450.,true));}
    }
    #[test] fn prewrite_check_shares_filter_and_sensor_faults_still_stop_immediately(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);e.head=[LOOK_UP,0.];
        for n in 0..5 {assert_eq!(run(&mut e,t,n*100,n+1,5.),[0.2,0.,0.]);
            assert!(e.permits_motion(t+Duration::from_millis(n*100+1),1,Some(Range{seq:n+1,mm:5.}),true,[0.2,0.,0.]));}
        assert!(!e.permits_motion(t+Duration::from_millis(500),1,Some(Range{seq:6,mm:5.}),true,[0.2,0.,0.]));
        assert_eq!(run(&mut e,t,501,6,5.),[0.;3]);assert_eq!(e.phase,Phase::Stop);
        assert!(!e.permits_motion(t+Duration::from_millis(502),1,None,true,[0.2,0.,0.]));
        let mut f=NearFilter::default();for n in 0..5 {assert!(!f.observe(t+Duration::from_millis(n*100),Some(Range{seq:n,mm:249.}),250.,false));}
        assert!(f.observe(t+Duration::from_millis(500),Some(Range{seq:5,mm:249.}),250.,false));
        assert!(!f.observe(t+Duration::from_millis(600),Some(Range{seq:6,mm:250.}),250.,false));
    }
    #[test] fn selection_is_explicit(){assert!(selected(&json!({"mode":"explore"})).unwrap());assert!(!selected(&json!({"mode":"interaction"})).unwrap());for bad in [json!({"mode":"walk"}),json!({"mode":"explore","speed":1}),json!({})]{assert!(selected(&bad).is_err());}}
}
