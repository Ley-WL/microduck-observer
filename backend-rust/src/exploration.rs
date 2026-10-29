//! Bounded, head-mounted ToF exploration. Produces intents, never touches UART.
use std::time::{Duration, Instant};
use rand::Rng;
use serde_json::{json,Value};
use crate::telemetry::Telemetry;

pub mod behavior;
pub mod interaction;
pub mod parameters;
use parameters::*;
// Internal ordering value only; no-return is never reported as a measured distance.
const NO_TARGET:f64=4001.;
// User-requested exclusions: fixed near returns and the bottom two displayed rows.
// Zero-based raw sensor indices, before any display rotation. Not free-space evidence.
const EXCLUDED_ZONES:[usize;24]=[16,24,25,32,33,34,40,41,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63];
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
    pub round_done:bool,
    pub deferred:bool,
    range_wait_since:Option<Instant>, scan_retries:u8,
    range_missing_since:Option<Instant>, range_seen:bool,
    phase:Phase, epoch:u64, since:Option<Instant>, settled:Option<Instant>,
    last_frame:u64, samples:u8, nearest:Option<f64>, left:Option<f64>, right:Option<f64>,
    wide_scan:bool, scan_index:usize, scan_ranges:[Option<f64>;6], left_angle:f64, right_angle:f64,
    direction:f32, turn_yaw:f64, turn_last_yaw:Option<f64>, turn_progress:f64, attempts:u8, forward_since:Option<Instant>,
    pub head:[f64;2], pub twist:[f32;3], pub reason:String, pub distance:Option<f64>,
    pub sound:bool,
    no_target:bool,
    forward_seconds:f64, turn_degrees:f64, retreat_seconds:f64, retreat_available:bool,
    travel_yaw:f64,
    forward_near:NearFilter,
    event:Option<(&'static str,Instant)>, last_call:Option<Instant>, next_chatter:Option<Instant>,
    departed:bool, pub last_voice:Option<&'static str>,
}
#[derive(Clone,Copy)]
pub struct Range {pub seq:u64,pub mm:f64}
pub struct Input {pub active:bool,pub epoch:u64,pub range:Option<Range>,pub head:[f64;2],pub still:bool,pub yaw:f64}
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
    /// A feedback interruption is not a manual acknowledgement of a blocked state.
    pub fn pause_feedback(&mut self){
        if self.phase==Phase::Blocked {
            self.twist=[0.;3];self.event=None;self.distance=None;self.no_target=false;
            self.forward_near=NearFilter::default();
        }else{
            let (epoch,attempts)=(self.epoch,self.attempts);
            self.reset();self.epoch=epoch;self.attempts=attempts;
        }
    }
    fn enter(&mut self,phase:Phase,now:Instant){
        let voice=match phase {
            Phase::Forward=>{let v=if self.departed {"explore_clear"}else{"explore_start"};self.departed=true;Some(v)},
            Phase::Stop=>Some("explore_obstacle"),Phase::Blocked=>Some("explore_blocked"),_=>None
        };
        if let Some(voice)=voice{self.event=Some((voice,now));}
        if matches!(phase,Phase::Forward|Phase::Turn|Phase::Retreat) {self.range_wait_since=None;self.scan_retries=0;}
        if phase==Phase::Forward {self.forward_seconds=rand::thread_rng().gen_range(FORWARD_SECONDS[0]..=FORWARD_SECONDS[1]);}
        if phase==Phase::Turn {self.turn_last_yaw=None;self.turn_progress=0.;self.turn_degrees=rand::thread_rng().gen_range(TURN_MIN_DEGREES..=TURN_MAX_DEGREES);}
        if phase==Phase::Retreat {self.retreat_seconds=rand::thread_rng().gen_range(RETREAT_SECONDS[0]..=RETREAT_SECONDS[1]);}
        self.forward_near=NearFilter::default();
        self.phase=phase;self.head=self.scan_head();self.since=Some(now);self.settled=None;self.samples=0;self.nearest=None;self.twist=[0.;3];}
    fn scan_head(&self)->[f64;2] {
        [LOOK_UP,match self.phase {Phase::Left=>SCAN_ANGLES[self.scan_index],Phase::Right=>-SCAN_ANGLES[self.scan_index],_=>0.}]
    }
    fn start_scan(&mut self,now:Instant,wide:bool) {
        self.wide_scan=wide;
        self.left=None;self.right=None;self.left_angle=SCAN_ANGLES[0];self.right_angle=SCAN_ANGLES[0];
        self.scan_index=0;self.scan_ranges=[None;6];self.enter(Phase::Left,now);
    }
    fn scanned(&mut self,distance:f64,now:Instant) {
        let left=self.phase==Phase::Left;
        self.scan_ranges[if left {self.scan_index}else{3+self.scan_index}]=Some(distance);
        let (best,angle)=if left {(&mut self.left,&mut self.left_angle)}else{(&mut self.right,&mut self.right_angle)};
        if best.is_none_or(|old|distance>old) {*best=Some(distance);*angle=SCAN_ANGLES[self.scan_index];}
        if self.scan_index+1<if self.wide_scan {SCAN_ANGLES.len()}else{1} {self.scan_index+=1;self.enter(self.phase,now);}
        else if left {self.scan_index=0;self.enter(Phase::Right,now);}
        else {
            self.direction=match (self.left.unwrap_or(0.)>=CLEAR_MM,self.right.unwrap_or(0.)>=CLEAR_MM) {
                (true,true)=>if self.left>self.right {1.}else if self.right>self.left {-1.}else if rand::thread_rng().gen_bool(0.5){1.}else{-1.},
                (true,false)=>1.,_=>-1.,
            };
            self.enter(Phase::Center,now);
        }
    }
    fn block(&mut self,now:Instant,reason:&str){self.enter(Phase::Blocked,now);self.reason=reason.into();self.head=[0.;2];}
    fn defer(&mut self,now:Instant,reason:&str) {
        self.enter(Phase::Observe,now);self.head=[LOOK_UP,0.];self.deferred=true;
        self.left=None;self.right=None;self.scan_ranges=[None;6];self.event=None;
        self.forward_since=None;self.retreat_available=false;self.reason=reason.into();
    }
    fn range_available(&mut self,now:Instant,range:Option<Range>)->bool {
        if range.is_some_and(|r|r.mm.is_finite() && (0. ..=NO_TARGET).contains(&r.mm)) {
            self.range_missing_since=None;self.range_seen=true;true
        }else{self.range_missing_since.get_or_insert(now);false}
    }
    fn brief_range_dropout(&self,now:Instant)->bool {
        self.range_seen && self.range_missing_since.is_some_and(|t|now.saturating_duration_since(t)<RANGE_DROPOUT_GRACE)
    }
    fn forward_command(&mut self,now:Instant,yaw:f64) {
        if self.forward_since.is_none(){self.forward_since=Some(now);self.travel_yaw=yaw;}
        let elapsed=now.saturating_duration_since(self.forward_since.unwrap()).as_secs_f64();
        if elapsed>=self.forward_seconds {
            self.attempts=0;self.forward_since=None;self.enter(Phase::Observe,now);self.round_done=true;
            self.reason="本段随机前进结束，抬头重新扫描".into();
        }else{self.twist[0]=FORWARD_SPEED*(elapsed/FORWARD_RAMP_SECONDS).clamp(0.,1.) as f32;self.reason.clear();}
    }
    pub fn tick(&mut self,now:Instant,i:Input)->[f32;3] {
        self.twist=[0.;3];
        if !self.selected || !i.active {self.reset();return self.twist;}
        if self.epoch!=i.epoch {self.reset();self.epoch=i.epoch;}
        self.since.get_or_insert(now);
        if self.next_chatter.is_none(){self.schedule_chatter(now);}
        self.no_target=i.range.is_some_and(|r|r.mm==NO_TARGET);
        self.distance=i.range.filter(|r|r.mm!=NO_TARGET).map(|r|r.mm);
        if self.phase==Phase::Blocked || self.deferred {return self.twist;}
        if self.range_wait_since.is_some_and(|t|now.saturating_duration_since(t).as_secs_f64()>=RANGE_RECOVERY_SECONDS) {
            self.defer(now,"ToF测距反复异常，本轮暂停，转入原地观察后重试");return self.twist;
        }
        let range_available=self.range_available(now,i.range);
        let pose_valid=i.yaw.is_finite() && i.head.iter().all(|v|v.is_finite());
        if !range_available || !pose_valid {
            let facing=pose_valid && i.head.iter().zip(self.scan_head()).all(|(a,b)|(a-b).abs()<MOTION_ALIGNMENT_DEGREES);
            if !range_available && self.brief_range_dropout(now) && pose_valid {
                if self.phase==Phase::Forward && self.forward_since.is_some() && facing && !self.forward_near.confirmed {
                    self.forward_command(now,i.yaw);
                    if self.phase==Phase::Forward {self.reason="ToF短暂异常，容忍窗口内继续直行".into();}
                    return self.twist;
                }
                if matches!(self.phase,Phase::Observe|Phase::Stop|Phase::Left|Phase::Right|Phase::Center) {
                    self.range_wait_since.get_or_insert(now);
                    self.settled=None;self.samples=0;self.nearest=None;
                    self.reason="ToF短暂异常，保持当前角度等待重新采样".into();return self.twist;
                }
            }
            self.range_wait_since.get_or_insert(now);
            // Repeated invalid frames clear samples, not the recovery deadline.
            if self.phase!=Phase::Observe {self.enter(Phase::Observe,now);}
            self.settled=None;self.samples=0;self.nearest=None;self.forward_near=NearFilter::default();
            self.left=None;self.right=None;self.scan_ranges=[None;6];
            self.event=None;self.head=[LOOK_UP,0.];self.forward_since=None;self.retreat_available=false;
            self.reason="ToF测距不可用，已暂停移动，等待恢复".into();return self.twist;
        }
        let r=i.range.unwrap();
        let obstacle=self.forward_near.observe(now,Some(r),STOP_MM,true);
        let new_frame=r.seq!=self.last_frame;self.last_frame=r.seq;
        self.head=self.scan_head();
        let aligned=i.head.iter().zip(self.head).all(|(a,b)|(a-b).abs()<SCAN_ALIGNMENT_DEGREES);
        let elapsed=now.saturating_duration_since(self.since.unwrap()).as_secs_f64();
        let facing=i.head.iter().zip(self.head).all(|(a,b)|(a-b).abs()<MOTION_ALIGNMENT_DEGREES);
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
            self.forward_command(now,i.yaw);return self.twist;
        }
        if self.phase==Phase::Turn {
            if !facing {self.block(now,"转向时头部朝向偏差超过12度，等待人工接管");return self.twist;}
            // Integrate wrapped increments: start-relative wrapping flips sign at 180 degrees.
            let previous=self.turn_last_yaw.replace(i.yaw).unwrap_or(self.turn_yaw);
            self.turn_progress+=(i.yaw-previous).sin().atan2((i.yaw-previous).cos());
            let delta=self.turn_progress;
            if delta*(self.direction as f64) < -45f64.to_radians() || delta*(self.direction as f64) > (self.turn_degrees+15.).to_radians(){self.block(now,"转向偏离目标，等待人工接管");}
            else if delta*(self.direction as f64)>=self.turn_degrees.to_radians(){self.enter(Phase::Observe,now);}
            else if elapsed>=TURN_TIMEOUT_SECONDS {self.block(now,"转向超时，等待人工接管");}
            else {self.twist[2]=self.direction*TURN_SPEED;}
            return self.twist;
        }
        if elapsed>SCAN_TIMEOUT_SECONDS {
            self.scan_retries+=1;
            if self.scan_retries>=MAX_SCAN_RETRIES {
                self.defer(now,"扫描连续超时，本轮暂停，转入原地观察后重试");return self.twist;
            }
            // Re-center before taking a new front reading. Keep the turn-attempt
            // budget, but never reuse partial side measurements from a failed scan.
            self.left=None;self.right=None;self.scan_ranges=[None;6];
            self.left_angle=SCAN_ANGLES[0];self.right_angle=SCAN_ANGLES[0];
            self.scan_index=0;self.wide_scan=false;self.forward_since=None;
            self.retreat_available=false;self.round_done=false;self.event=None;
            self.enter(Phase::Observe,now);
            self.reason="扫描等待超时，头部回正后重新观察".into();return self.twist;
        }
        if !aligned || !i.still {
            self.settled=None;self.samples=0;self.nearest=None;
            self.reason=if !aligned {"等待头部到达测距角度"}else{"等待身体站稳后测距"}.into();return self.twist;
        }
        let settled=*self.settled.get_or_insert(now);
        // Let pre-settle ToF frames age out; count distinct frames only.
        if now.saturating_duration_since(settled)<SCAN_SETTLE || !new_frame{self.reason="测距可用，等待稳定采样".into();return self.twist;}
        self.samples=self.samples.saturating_add(1);self.nearest=Some(self.nearest.unwrap_or(r.mm).min(r.mm));
        if self.samples<SCAN_SAMPLES{self.reason="正在收集不同ToF帧".into();return self.twist;}
        // Partial scan samples do not establish navigation recovery.
        let distance=self.nearest.unwrap();
        match self.phase {
            Phase::Observe=>{self.reason="抬头扫描左右通路".into();self.start_scan(now,distance<CLEAR_MM);},
            Phase::Stop=>{if self.retreat_available && elapsed<2. && rand::thread_rng().gen_bool(0.5) {
                self.retreat_available=false;self.reason="沿刚走过的方向短退，再重新扫描".into();self.enter(Phase::Retreat,now);
            }else if self.attempts>=MAX_TURN_ATTEMPTS{self.block(now,"连续三次未找到通路，等待人工接管");}else{self.start_scan(now,distance<CLEAR_MM);}},
            Phase::Left|Phase::Right=>self.scanned(distance,now),
            Phase::Center=>{
                if distance>=CLEAR_MM {self.reason.clear();self.forward_since=None;self.enter(Phase::Forward,now);}
                else if self.attempts>=MAX_TURN_ATTEMPTS {self.block(now,"连续三次未找到通路，等待人工接管");}
                else if !self.wide_scan {self.reason="前方出现遮挡，扩大扫描角度".into();self.start_scan(now,true);}
                else if self.left.unwrap_or(0.).max(self.right.unwrap_or(0.))>=CLEAR_MM {
                    self.turn_yaw=i.yaw;self.attempts+=1;self.enter(Phase::Turn,now);
                    self.turn_degrees=if self.direction>0. {self.left_angle}else{self.right_angle}.clamp(TURN_MIN_DEGREES,TURN_MAX_DEGREES);
                }else{
                    self.turn_yaw=i.yaw;self.attempts+=1;self.enter(Phase::Turn,now);
                    self.direction=if self.left>self.right {1.}else if self.right>self.left {-1.}else if rand::thread_rng().gen_bool(0.5){1.}else{-1.};
                    self.turn_degrees=U_TURN_DEGREES;self.reason="大角度扫描仍无通路，原地掉头后重新观察".into();
                }
            },
            _=>{}
        }
        self.twist
    }
    pub fn permits_motion(&mut self,now:Instant,epoch:u64,range:Option<Range>,command:[f32;3])->bool{
        if !self.selected || self.epoch!=epoch{return false;}
        if self.deferred || self.phase==Phase::Blocked {return command==[0.;3];}
        if !self.range_available(now,range) {
            return command==[0.;3] || (self.phase==Phase::Forward && self.forward_since.is_some()
                && self.brief_range_dropout(now) && !self.forward_near.confirmed
                && command[0]>0. && command[0]<=FORWARD_SPEED && command[1]==0. && command[2]==0.);
        }
        let forward=self.forward_near.observe(now,range,STOP_MM,true);
        if command==[0.;3]{return true;}
        if command[0]<0. && self.phase!=Phase::Retreat{return false;}
        command[0]<=0. || !forward
    }
    fn schedule_chatter(&mut self,now:Instant){use rand::Rng;self.next_chatter=Some(now+Duration::from_secs(rand::thread_rng().gen_range(CHATTER_SECONDS[0]..=CHATTER_SECONDS[1])));}
    /// One bounded event slot; never delay movement or queue a backlog of calls.
    pub fn call(&mut self,now:Instant,busy:bool)->Option<&'static str>{
        if !self.selected || self.since.is_none(){self.event=None;return None;}
        if self.event.is_some_and(|(_,at)|now.saturating_duration_since(at)>Duration::from_secs(2)){self.event=None;}
        if busy{return None;}
        let event=self.event.map(|(v,_)|v);
        let cooldown=if event==Some("explore_blocked"){2}else{4};
        if self.last_call.is_some_and(|at|now.saturating_duration_since(at)<Duration::from_secs(cooldown)){return None;}
        let voice=if let Some(v)=event{self.event=None;v}
            else if self.phase==Phase::Blocked && self.last_call.is_some_and(|at|now.saturating_duration_since(at).as_secs_f64()>=BLOCKED_REMINDER_SECONDS){"explore_blocked"}
            else if self.phase==Phase::Forward && self.next_chatter.is_some_and(|t|now>=t){"explore_idle"}
            else{return None;};
        self.last_call=Some(now);self.last_voice=Some(voice);self.sound=true;self.schedule_chatter(now);Some(voice)
    }
    pub fn status(&self)->Value {json!({"rangeDropoutGraceMs":RANGE_DROPOUT_GRACE.as_millis(),"deferred":self.deferred,"rangeRecoverySeconds":RANGE_RECOVERY_SECONDS,"scanRetryLimit":MAX_SCAN_RETRIES,"excludedTofZones":EXCLUDED_ZONES,"requiredTofZones":64-EXCLUDED_ZONES.len(),"lastVoice":self.last_voice,"selected":self.selected,"behavior":match self.phase{Phase::Observe=>"抬头观察",Phase::Forward=>"缓慢前进",Phase::Stop=>"障碍停步",Phase::Left=>"向左测距",Phase::Right=>"向右测距",Phase::Center=>"头部回正",Phase::Turn=>if self.turn_degrees==U_TURN_DEGREES {"原地掉头"}else{"朝通路转向"},Phase::Retreat=>"短退后重新扫描",Phase::Blocked=>"等待人工接管"},"reason":self.reason,"distanceMm":self.distance,"rangeAssessment":if self.no_target{"no-target"}else if self.distance.is_some(){"target"}else{"unavailable"},"leftMm":self.left.filter(|v|*v!=NO_TARGET),"rightMm":self.right.filter(|v|*v!=NO_TARGET),"leftNoTarget":self.left==Some(NO_TARGET),"rightNoTarget":self.right==Some(NO_TARGET),"forwardBurstSeconds":self.forward_seconds,"forwardBurstRangeSeconds":FORWARD_SECONDS,"turnDegrees":self.turn_degrees,"turnProgressDegrees":self.turn_progress.to_degrees(),"uTurnDegrees":U_TURN_DEGREES,"wideScan":self.wide_scan,"normalScanAnglesDegrees":[SCAN_ANGLES[0],-SCAN_ANGLES[0]],"forwardRampSeconds":FORWARD_RAMP_SECONDS,"scanAnglesDegrees":[30,60,90,-30,-60,-90],"scanDistancesMm":self.scan_ranges.map(|v|v.filter(|d|*d!=NO_TARGET)),"scanNoTarget":self.scan_ranges.map(|v|v==Some(NO_TARGET)),"turnRangeDegrees":[TURN_MIN_DEGREES,TURN_MAX_DEGREES],"turnTimeoutSeconds":TURN_TIMEOUT_SECONDS,"retreatSeconds":self.retreat_seconds,"headDegrees":self.head,"commandTwist":self.twist,"attempts":self.attempts,"obstacleConfirmMs":OBSTACLE_CONFIRM.as_millis(),"obstacleConfirmed":self.forward_near.confirmed,"turnObstacleConfirmed":false,"chatterRangeSeconds":CHATTER_SECONDS,"blockedReminderSeconds":BLOCKED_REMINDER_SECONDS,"stopMm":STOP_MM,"clearMm":CLEAR_MM})}
}

fn path()->std::path::PathBuf {std::path::PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE").unwrap_or("./state/calibration.json".into())).with_file_name("autonomy.json")}
fn selected(v:&Value)->anyhow::Result<bool>{anyhow::ensure!(v.as_object().is_some_and(|o|o.len()==1),"仅接受 mode");match v["mode"].as_str(){Some("interaction"|"explore")=>Ok(true),_=>anyhow::bail!("无效自主模式")}}
pub fn init(shared:&crate::telemetry::Shared){shared.write().unwrap().autonomy.explorer.selected=true;match std::fs::read(path()){Ok(b)=>match serde_json::from_slice(&b).map_err(anyhow::Error::from).and_then(|v|selected(&v)){Ok(v)=>shared.write().unwrap().autonomy.explorer.selected=v,Err(e)=>shared.write().unwrap().log("ERROR","autonomy",&format!("自主偏好读取失败：{e}"))},Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>shared.write().unwrap().log("ERROR","autonomy",&format!("自主偏好读取失败：{e}"))}}
pub fn router()->axum::Router<crate::App>{axum::Router::new().route("/api/v1/autonomy/settings",axum::routing::get(get).post(set))}
async fn get(axum::extract::State(a):axum::extract::State<crate::App>)->axum::Json<Value>{axum::Json(json!({"mode":if a.telemetry.read().unwrap().autonomy.explorer.selected{"explore"}else{"interaction"}}))}
async fn set(axum::extract::State(a):axum::extract::State<crate::App>,headers:axum::http::HeaderMap,body:axum::body::Bytes)->axum::response::Response {
    use axum::{http::StatusCode,response::IntoResponse};
    let v=match crate::check_write(&headers,&body,256){Ok(v)=>v,Err(e)=>return e};
    let choice=match selected(&v){Ok(v)=>v,Err(e)=>return crate::error(StatusCode::UNPROCESSABLE_ENTITY,&e.to_string())};
    let v=json!({"mode":"explore"}); // Normalize legacy in-place preferences to the merged mode.
    let mut s=a.telemetry.write().unwrap();
    if s.autonomy.enabled(){return crate::error(StatusCode::CONFLICT,"请先关闭自主模式再选择行为");}
    let save=||->anyhow::Result<()> {use std::io::Write;let p=path();std::fs::create_dir_all(p.parent().unwrap())?;let temp=p.with_extension("tmp");let mut f=std::fs::File::create(&temp)?;write!(f,"{v}")?;f.sync_all()?;drop(f);std::fs::rename(temp,p)?;Ok(())};
    if let Err(e)=save(){return crate::error(StatusCode::INTERNAL_SERVER_ERROR,&format!("保存失败：{e}"));}
    s.autonomy.explorer.reset();s.autonomy.explorer.selected=choice;
    axum::Json(v).into_response()
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn transient_forward_dropout_continues_but_shared_deadline_stops_at_500ms(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);e.forward_seconds=12.;
        run(&mut e,t,0,1,1000.);assert_eq!(run(&mut e,t,600,2,1000.),[0.2,0.,0.]);
        // The post-inference check sees the loss first; tick must share its deadline.
        assert!(e.permits_motion(t+Duration::from_millis(700),1,None,[0.2,0.,0.]));
        for ms in [720,1000,1199] {
            let mut i=input(ms,1000.);i.range=None;
            assert_eq!(e.tick(t+Duration::from_millis(ms),i),[0.2,0.,0.]);
            assert!(e.permits_motion(t+Duration::from_millis(ms),1,None,[0.2,0.,0.]));
            assert_eq!(e.forward_since,Some(t));
        }
        assert!(!e.permits_motion(t+Duration::from_millis(1200),1,None,[0.2,0.,0.]));
        let mut i=input(1200,1000.);i.range=None;assert_eq!(e.tick(t+Duration::from_millis(1200),i),[0.;3]);
        assert_eq!(e.phase,Phase::Observe);assert!(e.forward_since.is_none());
    }
    #[test] fn recovered_range_resets_dropout_window_without_restarting_forward_duration(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);e.forward_seconds=6.;
        run(&mut e,t,0,1,1000.);run(&mut e,t,500,2,1000.);
        for ms in [1000,1400] {let mut i=input(ms,1000.);i.range=None;assert_eq!(e.tick(t+Duration::from_millis(ms),i),[0.2,0.,0.]);}
        run(&mut e,t,1450,3,1000.);assert!(e.range_missing_since.is_none());
        let mut i=input(1600,1000.);i.range=None;assert_eq!(e.tick(t+Duration::from_millis(1600),i),[0.2,0.,0.]);
        run(&mut e,t,1700,4,1000.);run(&mut e,t,5900,5,1000.);
        let mut i=input(5950,1000.);i.range=None;e.tick(t+Duration::from_millis(5950),i);
        let mut i=input(6000,1000.);i.range=None;assert_eq!(e.tick(t+Duration::from_millis(6000),i),[0.;3]);
        assert!(e.round_done);assert_eq!(e.phase,Phase::Observe);
    }
    #[test] fn brief_scan_dropout_retains_angle_but_discards_unfinished_samples(){
        let t=Instant::now();let mut e=explorer();settle(&mut e,t,1000.);let start=t+Duration::from_secs(1);
        let since=e.since;let mut i=input(100,1000.);i.range=None;i.head=e.head;
        e.tick(start,i);assert_eq!(e.phase,Phase::Left);assert_eq!(e.head,[LOOK_UP,30.]);assert_eq!(e.since,since);
        assert_eq!(e.samples,0);assert!(e.settled.is_none());
        let mut i=input(101,1000.);i.range=None;i.head=e.head;e.tick(start+Duration::from_millis(500),i);
        assert_eq!(e.phase,Phase::Observe);assert_eq!(e.head,[LOOK_UP,0.]);
    }
    #[test] fn dropout_grace_never_starts_motion_or_bypasses_pose_and_ownership(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);
        let mut i=input(1,1000.);i.range=None;assert_eq!(e.tick(t,i),[0.;3]);
        assert!(!e.permits_motion(t,1,None,[0.2,0.,0.]));
        for fault in 0..3 {
            let mut e=explorer();e.enter(Phase::Forward,t);run(&mut e,t,0,1,1000.);run(&mut e,t,500,2,1000.);
            let mut i=input(3,1000.);i.range=None;
            match fault {0=>i.head=[LOOK_UP,20.],1=>i.yaw=f64::NAN,_=>i.active=false}
            assert_eq!(e.tick(t+Duration::from_millis(600),i),[0.;3]);
            assert!(!e.permits_motion(t+Duration::from_millis(601),1,None,[0.2,0.,0.]));
        }
        let mut e=explorer();e.enter(Phase::Forward,t);run(&mut e,t,0,1,1000.);
        assert!(!e.permits_motion(t,2,None,[0.2,0.,0.]));
        assert!(!e.permits_motion(t,1,None,[0.,0.,1.]));
    }
    #[test] fn clear_front_scans_only_thirty_and_new_front_obstacle_expands(){
        let t=Instant::now();let mut e=explorer();
        settle(&mut e,t,1000.);assert_eq!(e.phase,Phase::Left);assert!(!e.wide_scan);
        settle(&mut e,t+Duration::from_secs(1),1000.);assert_eq!(e.phase,Phase::Right);assert_eq!(e.head,[LOOK_UP,-30.]);
        settle(&mut e,t+Duration::from_secs(2),1000.);assert_eq!(e.phase,Phase::Center);
        assert!(e.scan_ranges[1..3].iter().all(Option::is_none));assert!(e.scan_ranges[4..6].iter().all(Option::is_none));
        settle(&mut e,t+Duration::from_secs(3),400.);assert_eq!(e.phase,Phase::Left);assert!(e.wide_scan);assert_eq!(e.twist,[0.;3]);
        let mut e=explorer();settle(&mut e,t,400.);assert!(e.wide_scan);assert_eq!(e.phase,Phase::Left);
    }
    #[test] fn forward_ramps_in_half_second_and_sensor_stop_resets_start(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);e.forward_seconds=10.;
        for (ms,speed) in [(0,0.),(100,0.04),(250,0.1),(500,0.2),(700,0.2)] {
            let twist=run(&mut e,t,ms,ms+1,1000.);assert!((twist[0]-speed).abs()<1e-6,"{ms}: {twist:?}");
        }
        let mut i=input(800,1000.);i.range=None;assert_eq!(e.tick(t+Duration::from_millis(800),i),[0.2,0.,0.]);
        let mut i=input(1300,1000.);i.range=None;assert_eq!(e.tick(t+Duration::from_millis(1300),i),[0.;3]);
        e.enter(Phase::Forward,t+Duration::from_secs(2));assert_eq!(run(&mut e,t,2000,2001,1000.),[0.;3]);
        for n in 0..5 {run(&mut e,t,2100+n*100,2101+n,400.);}assert_eq!(run(&mut e,t,2600,2601,400.),[0.;3]);assert_eq!(e.phase,Phase::Stop);
    }
    #[test] fn walking_chatter_can_happen_within_a_normal_forward_burst(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);e.forward_seconds=10.;e.forward_since=Some(t);
        assert_eq!(e.call(t,false),Some("explore_start"));
        let mut calls=0;
        for n in 1..10 {
            assert_eq!(run(&mut e,t,n*1000,n,1000.),[FORWARD_SPEED,0.,0.]);
            if e.call(t+Duration::from_secs(n),false)==Some("explore_idle"){calls+=1;}
        }
        assert!((1..=2).contains(&calls));assert_eq!(e.head,[LOOK_UP,0.]);
    }
    #[test] fn only_normal_navigation_rounds_finish_not_latched_faults(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);e.forward_seconds=3.;e.forward_since=Some(t);
        run(&mut e,t,3000,1,1000.);assert!(e.round_done);assert_eq!(e.twist,[0.;3]);
        e.reset();e.epoch=1;e.block(t,"转向超时，等待人工接管");run(&mut e,t,60000,4,1000.);assert!(!e.round_done);assert_eq!(e.twist,[0.;3]);
    }
    fn input(seq:u64,mm:f64)->Input {Input{active:true,epoch:1,range:Some(Range{seq,mm}),head:[LOOK_UP,0.],still:true,yaw:0.}}
    fn run(e:&mut Explorer,t:Instant,ms:u64,seq:u64,mm:f64)->[f32;3]{let mut i=input(seq,mm);i.head=e.head;e.tick(t+Duration::from_millis(ms),i)}
    fn explorer()->Explorer{Explorer{selected:true,epoch:1,..Default::default()}}
    fn settle(e:&mut Explorer,t:Instant,mm:f64){let phase=e.phase;for n in 0..150{run(e,t,n*20,n+100,mm);if e.phase!=phase{break;}}}
    #[test] fn six_angles_find_wide_open_path_and_center_before_turn() {
        let t=Instant::now();
        for (ranges,direction,angle) in [([300.,400.,1000.,200.,300.,350.],1.,90.),([300.,400.,500.,200.,1000.,350.],-1.,60.)] {
        let mut e=explorer();e.start_scan(t,true);
        for (step,mm) in ranges.into_iter().enumerate() {
            let yaw=SCAN_ANGLES[step%3]*if step<3 {1.}else{-1.};
            assert_eq!(e.head,[LOOK_UP,yaw]);
            let phase=e.phase;let index=e.scan_index;
            for n in 0..10 {
                let at=t+Duration::from_millis(step as u64*1000+n*100);
                let mut i=input(step as u64*100+n+1,mm);i.head=[LOOK_UP,yaw];
                assert_eq!(e.tick(at,i),[0.;3]);
                if e.phase!=phase || e.scan_index!=index {break;}
            }
        }
        assert_eq!(e.phase,Phase::Center);assert_eq!(e.head,[LOOK_UP,0.]);
        assert_eq!(e.scan_ranges,ranges.map(Some));
        // A head still looking right cannot authorize a body turn.
        let mut i=input(900,400.);i.head=[LOOK_UP,-90.];
        assert_eq!(e.tick(t+Duration::from_secs(6),i),[0.;3]);assert_eq!(e.phase,Phase::Center);
        settle(&mut e,t+Duration::from_secs(7),400.);
        assert_eq!(e.phase,Phase::Turn);assert_eq!(e.direction,direction);assert_eq!(e.turn_degrees,angle);
        }
    }
    #[test] fn each_scan_angle_requires_arrival_and_distinct_frames() {
        let t=Instant::now();let mut e=explorer();e.start_scan(t,true);
        for n in 0..10 {e.tick(t+Duration::from_millis(n*100),input(n+1,1000.));}
        assert_eq!(e.scan_index,0);assert_eq!(e.scan_ranges,[None;6]);
        for n in 10..20 {let mut i=input(100,1000.);i.head=e.head;e.tick(t+Duration::from_millis(n*100),i);}
        assert_eq!(e.scan_index,0);assert_eq!(e.scan_ranges,[None;6]);
        let mut i=input(101,1000.);i.range=None;e.tick(t+Duration::from_secs(2),i);
        assert_eq!(e.phase,Phase::Left);assert_eq!(e.samples,0);assert_eq!(e.twist,[0.;3]);
        let mut i=input(102,1000.);i.range=None;e.tick(t+Duration::from_millis(2500),i);
        assert_eq!(e.phase,Phase::Observe);assert_eq!(e.twist,[0.;3]);
        settle(&mut e,t+Duration::from_secs(3),1000.);
        assert_eq!(e.phase,Phase::Left);assert_eq!(e.scan_ranges,[None;6]);
    }
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
        assert_eq!(e.phase,Phase::Forward);e.forward_since=Some(t);
        for (n,mm) in [1000.,700.,600.,451.].into_iter().enumerate(){
            assert_eq!(run(&mut e,t,1300+n as u64*100,200+n as u64,mm),[0.2,0.,0.]);
        }
        for n in 0..5{assert_eq!(run(&mut e,t,1800+n*100,210+n,450.),[0.2,0.,0.]);}
        assert_eq!(run(&mut e,t,2300,215,450.),[0.;3]);assert_eq!(e.phase,Phase::Stop);
    }
    #[test] fn standard_stick_turn_both_directions_stops_at_measured_angle(){
        let t=Instant::now();
        for direction in [-1.,1.]{
            let mut e=explorer();e.enter(Phase::Turn,t);e.turn_degrees=90.;e.direction=direction;
            assert_eq!(e.tick(t,input(1,1000.)),[0.,0.,direction]);
            let mut i=input(2,1000.);i.yaw=direction as f64*91f64.to_radians();
            assert_eq!(e.tick(t+Duration::from_millis(400),i),[0.;3]);assert_eq!(e.phase,Phase::Observe);
        }
    }
    #[test] fn turns_allow_large_targets_until_ten_seconds_and_keep_deviation_guard(){
        let t=Instant::now();
        for direction in [-1.,1.] {
            let mut e=explorer();e.enter(Phase::Turn,t);
            assert!((30. ..=90.).contains(&e.turn_degrees));
            e.turn_degrees=90.;e.direction=direction;
            // Cross the yaw wrap while advancing past the former 45-degree cap.
            e.turn_yaw=direction as f64*170f64.to_radians();
            let mut i=input(1,1000.);i.yaw=-direction as f64*130f64.to_radians();
            assert_eq!(e.tick(t+Duration::from_millis(9999),i),[0.,0.,direction]);
            let mut i=input(2,1000.);i.yaw=-direction as f64*130f64.to_radians();
            assert_eq!(e.tick(t+Duration::from_secs(10),i),[0.;3]);assert_eq!(e.phase,Phase::Blocked);assert!(e.reason.contains("超时"));
            for delta in [-46f64,106.] {
                let mut e=explorer();e.enter(Phase::Turn,t);e.turn_degrees=90.;e.direction=direction;
                let mut i=input(1,1000.);i.yaw=direction as f64*delta.to_radians();
                assert_eq!(e.tick(t,i),[0.;3]);assert_eq!(e.phase,Phase::Blocked);assert!(e.reason.contains("偏离"));
            }
        }
    }
    #[test] fn sensor_loss_takeover_and_epoch_clear_motion(){
        let t=Instant::now();for fault in 0..3{let mut e=explorer();e.enter(Phase::Forward,t);let mut i=input(200,1000.);match fault{0=>i.range=None,1=>i.active=false,_=>i.epoch=2};assert_eq!(e.tick(t+Duration::from_secs(2),i),[0.;3]);assert_eq!(e.phase,Phase::Observe);}
    }
    #[test] fn scan_requires_measured_head_arrival_and_returns_center_before_turn(){
        let t=Instant::now();let mut e=explorer();e.start_scan(t,true);
        for n in 0..10{assert_eq!(e.tick(t+Duration::from_millis(n*100),input(n+1,1000.)),[0.;3]);}assert_eq!(e.phase,Phase::Left);
        settle(&mut e,t+Duration::from_secs(1),1000.);assert_eq!(e.phase,Phase::Right);
        settle(&mut e,t+Duration::from_secs(3),600.);assert_eq!(e.phase,Phase::Center);
        settle(&mut e,t+Duration::from_secs(5),400.);assert_eq!(e.phase,Phase::Turn);
        let mut i=input(999,400.);i.head=[LOOK_UP,0.];let out=e.tick(t+Duration::from_millis(6200),i);assert_eq!(out,[0.,0.,1.]);
        let mut i=input(1000,800.);i.yaw=(e.turn_degrees+1.).to_radians();assert_eq!(e.tick(t+Duration::from_millis(6300),i),[0.;3]);assert_eq!(e.phase,Phase::Observe);
    }
    #[test] fn scan_timeout_recenters_discards_partial_ranges_and_retries_after_settling(){
        let t=Instant::now();
        for phase in [Phase::Observe,Phase::Stop,Phase::Left,Phase::Right,Phase::Center] {
            for unsteady in [false,true] {
                let mut e=explorer();e.enter(phase,t);e.attempts=2;
                e.left=Some(1500.);e.right=Some(1200.);e.scan_ranges=[Some(1500.);6];
                e.wide_scan=true;e.scan_index=2;
                let mut i=input(1,1000.);i.still=!unsteady;
                if !unsteady {i.head=[40.,120.];}
                let retry=t+Duration::from_millis(6001);
                assert_eq!(e.tick(retry,i),[0.;3]);assert_eq!(e.phase,Phase::Observe);
                assert_eq!(e.head,[-15.,0.]);assert_eq!(e.attempts,2);
                assert_eq!(e.left,None);assert_eq!(e.right,None);assert_eq!(e.scan_ranges,[None;6]);
                assert_eq!(e.samples,0);assert!(e.settled.is_none());assert!(!e.round_done);
                assert!(e.event.is_none());assert!(e.reason.contains("重新观察"));
                let mut i=input(2,1000.);i.head=[-15.,30.];
                assert_eq!(e.tick(retry+Duration::from_millis(20),i),[0.;3]);
                assert_eq!(e.phase,Phase::Observe);assert_eq!(e.samples,0);
                settle(&mut e,retry+Duration::from_millis(40),1000.);
                assert_eq!(e.phase,Phase::Left);assert_eq!(e.head,[-15.,30.]);
                assert_eq!(e.attempts,2);assert!(!e.wide_scan);
            }
        }
    }
    #[test] fn repeated_unsteady_scan_timeouts_stay_stationary_without_latching(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Observe,t);
        for n in 1..=4 {
            let mut i=input(n,1000.);i.still=false;
            assert_eq!(e.tick(t+Duration::from_millis(n*6001),i),[0.;3]);
            assert_eq!(e.phase,Phase::Observe);assert_eq!(e.head,[-15.,0.]);
        }
    }
    #[test] fn blocked_is_latched_and_announcement_cannot_restart(){let mut e=explorer();let t=Instant::now();e.block(t,"blocked");let mut i=input(1,1000.);i.range=None;assert_eq!(e.tick(t,i),[0.;3]);assert_eq!(e.phase,Phase::Blocked);let mut i=input(2,1000.);i.active=false;e.tick(t,i);assert_eq!(e.phase,Phase::Observe);}
    #[test] fn close_returns_allow_turn_and_uturn_until_target_in_both_directions(){
        let t=Instant::now();
        for direction in [-1.,1.] {for degrees in [30.,180.] {
            let mut e=explorer();e.enter(Phase::Turn,t);e.direction=direction;e.turn_degrees=degrees;
            for n in 0..20 {
                let now=t+Duration::from_millis(n*100);
                let command=run(&mut e,t,n*100,n+1,100.);
                assert_eq!(command,[0.,0.,direction]);assert_eq!(e.phase,Phase::Turn);
                assert!(e.permits_motion(now,1,Some(Range{seq:n+1,mm:100.}),command));
                assert_eq!(e.head,[-15.,0.]);
            }
            let mut i=input(21,100.);i.yaw=(degrees*direction as f64).to_radians();
            assert_eq!(e.tick(t+Duration::from_secs(2),i),[0.;3]);assert_eq!(e.phase,Phase::Observe);
            assert!(!e.permits_motion(t+Duration::from_secs(2),1,None,[0.,0.,direction]));
        }}
    }
    #[test] fn turn_timeout_deviation_and_attempt_budget_stop(){
        let t=Instant::now();
        let mut e=explorer();e.enter(Phase::Turn,t);e.direction=1.;
        assert_eq!(e.tick(t+Duration::from_secs(10),input(1,1000.)),[0.;3]);assert_eq!(e.phase,Phase::Blocked);assert!(e.reason.contains("超时"));
        let mut e=explorer();e.enter(Phase::Turn,t);e.direction=1.;let mut i=input(1,1000.);i.yaw=-50f64.to_radians();
        assert_eq!(e.tick(t,i),[0.;3]);assert_eq!(e.phase,Phase::Blocked);
        let mut e=explorer();e.attempts=3;e.enter(Phase::Stop,t);settle(&mut e,t,400.);assert_eq!(e.phase,Phase::Blocked);
    }
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
        assert_eq!(e.call(t+Duration::from_secs(44),false),None);
        assert_eq!(e.call(t+Duration::from_secs(45),false),Some("explore_blocked"));
        assert_eq!(e.call(t+Duration::from_secs(46),false),None);
        assert_eq!(e.twist,[0.;3]);
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
        assert!(e.permits_motion(Instant::now(),1,clear,[0.2,0.,0.]));
        assert!(e.permits_motion(Instant::now(),1,close,[0.2,0.,0.]));
        assert!(!e.permits_motion(Instant::now(),2,clear,[0.2,0.,0.]));
        assert!(!e.permits_motion(Instant::now(),1,None,[0.,0.,1.]));
        assert!(e.permits_motion(Instant::now(),1,None,[0.;3]));
    }
    #[test] fn real_intermittent_tof_frames_cannot_restart_recovery_deadline_forever(){
        let frames:Vec<Value>=serde_json::from_str(include_str!("../tests/fixtures/exploration-tof-intermittent.json")).unwrap();
        let mut telemetry=Telemetry::new(false);telemetry.started=Instant::now()-Duration::from_secs(1);
        telemetry.tof=json!({"enabled":true,"state":"streaming"});
        let t=Instant::now();let mut e=explorer();let mut rejected=0;let mut progressed=false;
        for (n,data) in frames.iter().cycle().take(120).enumerate() {
            telemetry.sample("tof",data.clone(),true,Instant::now());let r=range(&telemetry);if r.is_none(){rejected+=1;}
            let mut i=input(n as u64+1,1000.);i.range=r;i.head=e.head;
            let now=t+Duration::from_millis(n as u64*300);let command=e.tick(now,i);
            if command[0]>0. {progressed=true;assert!(r.is_some() || e.brief_range_dropout(now));}
        }
        assert_eq!(rejected,36);assert!(e.deferred || progressed);
        if e.deferred {assert_eq!(e.scan_ranges,[None;6]);assert!(!e.permits_motion(t,1,None,[0.2,0.,0.]));}
    }
    #[test] fn temporary_range_failure_recovers_only_after_fresh_settled_samples(){
        let t=Instant::now();let mut e=explorer();let mut i=input(1,1000.);i.range=None;
        e.tick(t,i);assert_eq!(e.range_wait_since,Some(t));
        let mut i=input(2,1000.);i.head=[20.,0.];e.tick(t+Duration::from_millis(200),i);
        assert!(e.reason.contains("头部"));assert_eq!(e.range_wait_since,Some(t));
        settle(&mut e,t+Duration::from_secs(1),1000.);
        assert_eq!(e.phase,Phase::Left);assert_eq!(e.range_wait_since,Some(t));assert!(!e.deferred);
        settle(&mut e,t+Duration::from_secs(3),1000.);
        settle(&mut e,t+Duration::from_secs(5),1000.);
        settle(&mut e,t+Duration::from_secs(7),1000.);
        assert_eq!(e.phase,Phase::Forward);assert!(e.range_wait_since.is_none());
    }
    #[test] fn permanent_range_loss_and_repeated_unsteady_scans_defer_without_releasing_hard_block(){
        let t=Instant::now();let mut e=explorer();
        for n in 0..=1500 {let mut i=input(n+1,1000.);i.range=None;assert_eq!(e.tick(t+Duration::from_millis(n*20),i),[0.;3]);}
        assert!(e.deferred);
        e.reset();e.epoch=1;e.enter(Phase::Observe,t);
        for n in 1..=2 {let mut i=input(n,1000.);i.still=false;e.tick(t+Duration::from_millis(n*6001),i);}
        assert!(e.deferred);assert_eq!(e.phase,Phase::Observe);
        e.reset();e.epoch=1;e.block(t,"转向超时");let mut i=input(1,1000.);i.range=None;
        e.tick(t+Duration::from_secs(60),i);assert_eq!(e.phase,Phase::Blocked);assert!(!e.deferred);
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
        assert_eq!(run(&mut e,t,8000,1000,NO_TARGET),[0.;3]);
        assert_eq!(run(&mut e,t,8500,1001,NO_TARGET),[0.2,0.,0.]);
        assert_eq!(e.head,[LOOK_UP,0.]);assert!(e.status()["distanceMm"].is_null());
        assert_eq!(e.status()["rangeAssessment"],"no-target");
        assert_eq!(run(&mut e,t,20001,1001,NO_TARGET),[0.;3]);assert_eq!(e.phase,Phase::Observe);
        assert!(!e.permits_motion(Instant::now(),1,None,[0.2,0.,0.]));
        assert!(e.permits_motion(Instant::now(),1,Some(Range{seq:1002,mm:300.}),[0.2,0.,0.]));
    }
    #[test] fn all_wide_directions_blocked_start_uturn_toward_wider_side(){
        let t=Instant::now();
        for (left,right,direction) in [(300.,600.,-1.),(600.,300.,1.)] {
            let mut e=explorer();
            for (n,mm) in [400.,left,right,400.].into_iter().enumerate(){settle(&mut e,t+Duration::from_secs(n as u64*2),mm);}
            assert_eq!(e.phase,Phase::Turn);assert_eq!(e.turn_degrees,180.);assert_eq!(e.direction,direction);assert_eq!(e.attempts,1);
            assert_eq!(e.head,[LOOK_UP,0.]);assert_eq!(e.twist,[0.;3]);assert!(!e.round_done);
        }
    }
    #[test] fn uturn_tracks_both_directions_through_yaw_wrap_and_rescans(){
        let t=Instant::now();
        for direction in [-1.,1.] {
            let mut e=explorer();e.turn_yaw=direction as f64*170f64.to_radians();e.enter(Phase::Turn,t);e.turn_degrees=180.;e.direction=direction;
            for (n,degrees) in [0f64,60.,120.,179.,181.].into_iter().enumerate(){
                let yaw=e.turn_yaw+direction as f64*degrees.to_radians();let mut i=input(n as u64+1,400.);i.yaw=yaw.sin().atan2(yaw.cos());
                let out=e.tick(t+Duration::from_secs(n as u64),i);
                if degrees<180. {assert_eq!(out,[0.,0.,direction]);assert_eq!(e.phase,Phase::Turn);}
                else {assert_eq!(out,[0.;3]);assert_eq!(e.phase,Phase::Observe);}
            }
        }
    }
    #[test] fn uturn_keeps_time_limit_even_with_close_returns(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Turn,t);e.turn_degrees=180.;e.direction=-1.;
        assert_eq!(run(&mut e,t,9999,1,100.),[0.,0.,-1.]);assert_eq!(run(&mut e,t,10000,2,100.),[0.;3]);assert_eq!(e.phase,Phase::Blocked);
        assert!(e.reason.contains("超时"));
    }
    #[test] fn random_bursts_last_six_to_twelve_seconds_and_still_stop_for_obstacles(){
        let t=Instant::now();let mut durations=std::collections::BTreeSet::new();
        for _ in 0..50 {
            let mut e=explorer();e.enter(Phase::Forward,t);e.head=[LOOK_UP,0.];durations.insert((e.forward_seconds*1000.) as u64);
            assert!((6. ..=12.).contains(&e.forward_seconds));e.forward_seconds=10.;
            assert_eq!(run(&mut e,t,0,1,1000.),[0.;3]);
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
    #[test] fn prewrite_check_shares_obstacle_filter_and_does_not_restart_stopped_motion(){
        let t=Instant::now();let mut e=explorer();e.enter(Phase::Forward,t);e.head=[LOOK_UP,0.];
        for n in 0..5 {assert!((run(&mut e,t,n*100,n+1,5.)[0]-0.04*n as f32).abs()<1e-6);
            assert!(e.permits_motion(t+Duration::from_millis(n*100+1),1,Some(Range{seq:n+1,mm:5.}),[0.2,0.,0.]));}
        assert!(!e.permits_motion(t+Duration::from_millis(500),1,Some(Range{seq:6,mm:5.}),[0.2,0.,0.]));
        assert_eq!(run(&mut e,t,501,6,5.),[0.;3]);assert_eq!(e.phase,Phase::Stop);
        assert!(!e.permits_motion(t+Duration::from_millis(502),1,None,[0.2,0.,0.]));
    }
    #[test] fn selection_is_explicit(){assert!(selected(&json!({"mode":"explore"})).unwrap());assert!(selected(&json!({"mode":"interaction"})).unwrap());for bad in [json!({"mode":"walk"}),json!({"mode":"explore","speed":1}),json!({})]{assert!(selected(&bad).is_err());}}
    #[test] fn feedback_dropout_cannot_acknowledge_a_manual_required_block(){
        let mut a=crate::autonomy::Autonomy::default();
        a.mode=crate::autonomy::Mode::Standby;a.epoch=1;a.explorer=explorer();
        let t=Instant::now();let reason="转向超时，等待人工接管";
        a.explorer.block(t,reason);
        for n in 0..5 {
            a.invalid_feedback();
            settle(&mut a.explorer,t+Duration::from_secs(n*2),1000.);
            assert_eq!(a.explorer.phase,Phase::Blocked);
            assert_eq!(a.explorer.reason,reason);assert_eq!(a.explorer.twist,[0.;3]);
        }
        a.input(true,t+Duration::from_secs(12));
        assert_eq!(a.mode,crate::autonomy::Mode::Manual);
        assert_eq!(a.explorer.phase,Phase::Observe);
        assert!(a.explorer.reason.is_empty());
    }
    #[test] fn temporary_feedback_pause_preserves_budget_but_discards_old_scan(){
        let mut e=explorer();let t=Instant::now();e.attempts=2;
        e.left=Some(1000.);e.enter(Phase::Right,t);e.twist=[0.,0.,1.];
        e.pause_feedback();
        assert_eq!(e.attempts,2);assert_eq!(e.epoch,1);
        assert_eq!(e.phase,Phase::Observe);assert!(e.left.is_none());assert_eq!(e.twist,[0.;3]);
        run(&mut e,t,100,1,1000.);assert_eq!(e.attempts,2);
    }
    #[test] fn repeated_scan_turn_cycles_stop_before_fourth_turn(){
        let mut e=explorer();let t=Instant::now();
        for attempt in 0..4 {
            let start=t+Duration::from_secs(attempt*10);
            for (n,mm) in [400.,1000.,1000.,400.].into_iter().enumerate(){
                settle(&mut e,start+Duration::from_secs(n as u64*2),mm);
            }
            if attempt==3 {assert_eq!(e.phase,Phase::Blocked);assert_eq!(e.attempts,3);break;}
            assert_eq!(e.phase,Phase::Turn);assert_eq!(e.attempts,attempt as u8+1);
            let mut i=input(999+attempt,400.);i.yaw=e.direction as f64*(e.turn_degrees+1.).to_radians();
            assert_eq!(e.tick(start+Duration::from_secs(8),i),[0.;3]);
            assert_eq!(e.phase,Phase::Observe);
        }
    }
    #[test] fn last_turn_may_find_clear_front_but_cannot_turn_again(){
        let t=Instant::now();let mut e=explorer();e.attempts=MAX_TURN_ATTEMPTS;
        e.enter(Phase::Center,t);settle(&mut e,t,1000.);assert_eq!(e.phase,Phase::Forward);
        e.enter(Phase::Center,t+Duration::from_secs(2));
        settle(&mut e,t+Duration::from_secs(2),400.);assert_eq!(e.phase,Phase::Blocked);
    }
}
