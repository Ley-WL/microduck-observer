//! BLE intent control uses the same in-process routes and sole UART owner as Web.
use super::App;
use anyhow::{ensure, Result};
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tower::ServiceExt;

// Opt-in wire reduction only; the control loop and its full diagnostics stay unchanged.
fn compact_status(mut status: Value, request: &Value) -> Value {
    let observer = &mut status["observer"];
    if request["telemetryEncoding"] == "columns-v1" {
        let columns = ["id", "online", "position", "ageMs", "fault", "voltage", "temperature"];
        let joints = &mut observer["joints"];
        if let Some(rows) = joints["servos"].as_array() {
            let values: Vec<Value> = rows.iter().map(|r| Value::Array(columns.iter().map(|key| r[*key].clone()).collect())).collect();
            joints["rowEncoding"] = json!("columns-v1");
            joints["columns"] = json!(columns);
            joints["values"] = json!(values);
            joints.as_object_mut().unwrap().remove("servos");
        }
    }
    if let Some(control) = observer["control"].as_object_mut() {
        control.retain(|key, _| ["state", "message", "feedbackHolding", "posture", "kpRun", "kdRun", "activeSkill", "commandTwist", "commandAlpha", "ageMs"].contains(&key.as_str()));
    }
    let revision = observer["calibration"]["revision"].clone();
    observer["calibrationRevision"] = revision.clone();
    if !revision.is_null() && request["calibrationRevision"] == revision
        && request["calibrationBoot"] == observer["system"]["bootId"] {
        observer.as_object_mut().unwrap().remove("calibration");
    }
    status
}

pub struct Motion {
    app: App,
    router: Router,
    pub leased: bool,
    input_permit: Option<tokio::sync::OwnedSemaphorePermit>,
    source: &'static str,
    sequence: Option<u64>,
    last: Instant,
    drive: Option<String>,
    drive_seq: u64,
    forward: f64,
    turn: f64,
    lateral: f64,
    stopped: bool,
    phase: String,
    error: String,
    pending: bool,
    kick_models: [bool; 2],
}
impl Motion {
    pub fn new(app: App, router: Router) -> Self {
        Self {
            app,
            router,
            leased: false,
            input_permit: None,
            source: "ble-command",
            sequence: None,
            last: Instant::now(),
            drive: None,
            drive_seq: 0,
            forward: 0.,
            turn: 0.,
            lateral: 0.,
            stopped: false,
            phase: "idle".into(),
            error: String::new(),
            pending: false,
            kick_models: ["kick_left","kick_right"].map(|kind|crate::policy::metadata_for(kind).is_some()),
        }
    }
    pub async fn request(&self, path: &str, body: Option<Value>) -> Result<Value> {
        let mut request = Request::builder().uri(format!("/api/v1/{path}"));
        let data = if let Some(body) = body {
            request = request
                .method("POST")
                .header("content-type", "application/json");
            Body::from(serde_json::to_vec(&body)?)
        } else {
            Body::empty()
        };
        let response = self.router.clone().oneshot(request.body(data)?).await?;
        let status = response.status();
        let value: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
        ensure!(
            status.is_success(),
            "{}",
            value["detail"].as_str().unwrap_or("主板请求失败")
        );
        Ok(value)
    }
    pub fn local(app: App, router: Router) -> Self {
        let mut motion = Self::new(app, router);
        motion.source = "gamepad";
        motion
    }
    fn zero(&mut self) {
        if self.input_permit.is_some() {
            self.app.telemetry.write().unwrap().head_intent.clear();
        }
        self.zero_drive();
    }
    fn zero_drive(&mut self) {
        self.forward = 0.;
        self.turn = 0.;
        self.lateral = 0.;
    }
    fn quack_input(&mut self, head: [f64; 2]) {
        self.zero_drive();
        if self.input_permit.is_none() { return; }
        let ctrl = self.control();
        let ready = self.phase == "active" && !self.stopped && !self.pending
            && self.drive.as_deref().is_some_and(|session| ctrl["driveSession"] == session)
            && crate::head_control::ready(&ctrl);
        let mut telemetry = self.app.telemetry.write().unwrap();
        if ready { telemetry.head_intent.set(head); }
        else { telemetry.head_intent.clear(); }
    }
    fn control(&self) -> Value {
        self.app.telemetry.read().unwrap().control.clone()
    }
    fn sync(&mut self) {
        let ctrl = self.control();
        if let Some(session) = &self.drive {
            // A newly accepted start can precede the worker's first status.
            // The previous episode's failure must not discard that new lease.
            // Worker failures carry their job session, including preflight errors.
            if ctrl["state"] == "failed" && (ctrl["driveSession"] == session.as_str()
                || (ctrl["driveSession"].is_null() && self.phase != "starting")) {
                self.phase = "failed".into();
                self.error = ctrl["message"].as_str().unwrap_or("模型失败").into();
                self.drive = None;
                self.zero();
            } else if ctrl["state"] == "policy" && ctrl["driveSession"] == session.as_str() {
                self.phase = "active".into();
                if ctrl["feedbackHolding"] == true || ctrl["activeSkill"] != "xgoduck" {
                    self.zero();
                }
                if ctrl["activeSkill"] != "xgoduck" {
                    self.pending = false;
                }
            } else if self.phase == "active"
                || (self.phase == "starting"
                    && !self
                        .app
                        .telemetry
                        .read()
                        .unwrap()
                        .drive
                        .as_ref()
                        .is_some_and(|d| d.session == *session))
            {
                self.drive = None;
                self.phase = "idle".into();
                self.zero();
                self.pending = false;
            }
        }
        if self.drive.is_none() { self.input_permit = None; }
    }
    fn acquire_input(&mut self) -> Result<()> {
        if self.input_permit.is_none() {
            self.input_permit = Some(self.app.input_gate.clone().try_acquire_owned()
                .map_err(|_| anyhow::anyhow!("另一端正在控制，请先在该端停止保持再操作"))?);
        }
        Ok(())
    }
    pub fn status(&mut self) -> Value {
        self.sync();
        // Local evdev only consumes readiness/errors, never the App telemetry payload.
        if self.source == "gamepad" {
            let s=self.app.telemetry.read().unwrap();
            return json!({"headReady":self.phase=="active" && !self.stopped && !self.pending && crate::head_control::ready(&s.control),
                "motionError":self.error,"controlOwned":self.input_permit.is_some(),
                "controlBusy":self.input_permit.is_none() && self.app.input_gate.available_permits()==0});
        }
        // Copy the small snapshot under the lock; formatting, calibration and disk I/O
        // must not hold up the servo/IMU writers.
        let (ctrl, samples, boot, resources, uptime) = {
            let s=self.app.telemetry.read().unwrap();
            (s.control.clone(), ["joints","imu.orientation"].map(|topic|
                s.latest.get(topic).map(|v|s.stamp(v))),
                s.boot.clone(),s.resources.clone(),s.started.elapsed().as_secs_f64())
        };
        let mut observer = json!({"control":ctrl});
        observer["control"]["ageMs"] = json!(0);
        for (sample, target) in samples.into_iter().zip(["joints", "orientation"]) {
            if let Some(v) = sample {
                if v["source"] != "hardware" || v["valid"] != true {
                    continue;
                }
                let mut item = json!({"ageMs":v["ageMs"],"sequence":v["seq"],"bootId":v["bootId"]});
                if target == "joints" {
                    item["servos"] = v["data"]["servos"]
                        .as_array()
                        .map(|rows| {
                            Value::Array(
                                rows.iter()
                                    .map(|r| {
                                        let mut row = json!({});
                                        for key in [
                                            "id",
                                            "online",
                                            "position",
                                            "ageMs",
                                            "fault",
                                            "voltage",
                                            "temperature",
                                        ] {
                                            if !r[key].is_null() {
                                                row[key] = r[key].clone();
                                            }
                                        }
                                        row
                                    })
                                    .collect(),
                            )
                        })
                        .unwrap_or(json!([]));
                } else {
                    item["quaternion"] = v["data"]["quaternion"].clone();
                    item["frame"] = v["data"]["frame"].clone();
                }
                observer[target] = item;
            }
        }
        observer["modelGains"] = json!({"p":crate::gains::ui_current().p,"d":crate::gains::ui_current().d,"supported":true,"appliesOn":"next-model-start"});
        observer["observerOta"]=crate::observer_ota::status();observer["observerOta"]["supported"]=json!(crate::observer_ota::enabled());
        observer["calibration"] = self.app.cal.lock().unwrap().read(&boot);
        observer["system"] = json!({"bootId":boot,"source":if self.app.hardware {"hardware"} else {"simulation"},
            "resources":resources,"resourceAgeMs":resources["sampleMonoMs"].as_f64().map(|at|(crate::telemetry::monotonic()*1000.-at).max(0.)),
            "serviceUptimeSeconds":uptime,"powerControl":self.app.hardware && crate::power::enabled()});
        observer["system"]["wifiStatusReadOnly"]=json!(true);
        observer["kickModels"]=json!({"left":self.kick_models[0],"right":self.kick_models[1]});
        let skill = ctrl["activeSkill"].as_str().unwrap_or("xgoduck");
        let mode = if self.stopped {
            "estop"
        } else if self.phase == "active" {
            if skill == "sitstand_sit" {
                "sit"
            } else if self.forward.abs() + self.turn.abs() + self.lateral.abs() > 0.001 {
                "walk"
            } else {
                "stand"
            }
        } else {
            "idle"
        };
        json!({"controlOwned":self.input_permit.is_some(),"controlBusy":self.input_permit.is_none() && self.app.input_gate.available_permits()==0,"headControl":true,"headReady":self.phase=="active" && !self.stopped && !self.pending && crate::head_control::ready(&ctrl),"simulation":!self.app.hardware,"motion":self.app.owner.is_some() && crate::policy::metadata_for("xgoduck").is_some(),"mode":mode,"stopped":self.stopped,"forward":self.forward,"turn":self.turn,"attitude":null,"voltage":null,"imu":"integrated","protocol":1,"activeSkill":skill,"skillProgressSeconds":ctrl["skillProgressSeconds"].as_f64().unwrap_or(0.),"sitSettleSeconds":ctrl["sitSettleSeconds"].as_f64().unwrap_or(2.),"standAllowed":ctrl["standAllowed"]==true,"skillsPending":self.pending,"policy":"xgoduck","policyPhase":self.phase,"motionError":self.error,"observer":observer})
    }
    pub fn open(&mut self) -> Result<Value> {
        ensure!(!self.leased, "已有控制设备连接");
        self.leased = true;
        self.sequence = None;
        self.last = Instant::now();
        self.stopped = false;
        self.zero();
        self.error.clear();
        let mut status = self.status();
        status["session"] = json!(uuid::Uuid::new_v4().to_string());
        Ok(status)
    }
    pub async fn power(&mut self, mut body: Value) -> Result<Value> {
        ensure!(self.leased && self.app.hardware, "请连接真实主板后操作电源");
        let action = body["action"].as_str().unwrap_or("").to_owned();
        body.as_object_mut().ok_or_else(||anyhow::anyhow!("INVALID_COMMAND"))?.remove("type");
        body.as_object_mut().unwrap().remove("action");
        let boot = self.app.telemetry.read().unwrap().boot.clone();
        ensure!(crate::power::validate(&action, &body, &boot), "请重新连接当前主板并确认关机或重启");
        ensure!(!self.pending && !matches!(self.control()["state"].as_str(), Some("policy"|"moving"|"preflight"|"disabling")), "请先停止保持，再操作主板电源");
        self.zero();
        self.request(&format!("system/{action}"), Some(body)).await
    }
    pub async fn observer_ota(&mut self,body:Value)->Result<Value>{
        ensure!(self.leased&&self.app.hardware,"请连接真实主板");
        let op=body["op"].as_str().unwrap_or("");
        if op=="check" {return crate::observer_ota::request(op).await;}
        ensure!(op=="apply"&&body["confirm"]==true,"请确认更新操作");
        ensure!(body["bootId"]==self.app.telemetry.read().unwrap().boot,"主板会话已变化，请重新确认");
        let permit=self.app.gate.clone().try_acquire_owned()?;
        let rows={let telemetry=self.app.telemetry.read().unwrap();telemetry.latest.get("joints").map(|s|telemetry.stamp(s)).unwrap_or(Value::Null)};
        ensure!(rows["ageMs"].as_f64().is_some_and(|age|age<1000.),"反馈过期，不能更新");
        ensure!(crate::observer_ota::idle(&json!({"system":{"data":{"servoControl":self.control()}},"joints":{"ageMs":rows["ageMs"],"data":rows["data"]}})),"请先急停卸力，15个舵机必须全部在线且失能");
        self.zero();let result=crate::observer_ota::request(op).await?;ensure!(result["accepted"]==true,"主板未接受更新");
        tokio::spawn(async move{let _permit=permit;for _ in 0..300{tokio::time::sleep(Duration::from_secs(1)).await;if matches!(crate::observer_ota::status()["phase"].as_str(),Some("failed"|"success")){break;}}});
        Ok(result)
    }
    pub async fn halt(&mut self, unload: bool) -> Result<()> {
        self.sync();
        self.zero();
        self.pending = false;
        // Only cancel this controller's policy; explicit emergency stop unloads globally.
        let ctrl = self.control();
        let owned = self
            .drive
            .as_ref()
            .is_some_and(|session| ctrl["driveSession"] == session.as_str());
        let starting = self.drive.as_ref().is_some_and(|session| {
            self.app
                .telemetry
                .read()
                .unwrap()
                .drive
                .as_ref()
                .is_some_and(|d| d.session == *session)
        });
        self.drive = None;
        self.phase = "idle".into();
        let result = if unload {
            self.request("servos/disable", Some(json!({}))).await.map(|_| ())
        } else if owned || starting {
            self.request("policy/stop", Some(json!({}))).await.map(|_| ())
        } else { Ok(()) };
        self.input_permit = None;
        result
    }
    pub async fn close(&mut self) {
        if !self.leased { return; }
        let _ = self.halt(false).await;
        self.leased = false;
        self.stopped = false;
        self.sequence = None;
        self.input_permit = None;
    }
    pub async fn command(&mut self, body: Value) -> Result<Value> {
        ensure!(self.leased, "控制会话无效，请重新连接");
        let seq = body["seq"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("缺少指令序号"))?;
        ensure!(self.sequence.is_none_or(|last| seq > last), "指令序号过期");
        let action = body["action"].as_str().unwrap_or("");
        if !matches!(action,"drive"|"strafe_left"|"strafe_right") {
            self.app.telemetry.write().unwrap().log("INFO",self.source,&format!("控制动作已收到：{action}，seq={seq}"));
        }
        ensure!(
            [
                "drive",
                "stand",
                "prepare",
                "idle",
                "estop",
                "reset",
                "release",
                "strafe_left",
                "strafe_right",
                "getup",
                "pick",
                "roulade",
                "dance",
                "kick_left",
                "kick_right",
                "sit",
                "standup",
                "mouth_open",
                "mouth_close",
                "quack"
            ]
            .contains(&action),
            "不支持的指令"
        );
        let mut vals = [0.; 3];
        for (i, k) in ["forward", "turn", "speed"].iter().enumerate() {
            if !body[k].is_null() {
                vals[i] = body[k]
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| anyhow::anyhow!("控制量必须为有限数字"))?;
            }
        }
        let [f, t, speed] = vals;
        ensure!(
            (-1. ..=1.).contains(&f) && (-1. ..=1.).contains(&t) && (0. ..=1.0).contains(&speed),
            "控制量超出范围"
        );
        let head=crate::head_control::parse(&body)?;
        self.sequence = Some(seq);
        self.last = Instant::now();
        self.sync();
        let execution:Result<()> = async {
            // Connections/heartbeats never reserve motion. Only an explicit action
            // (or input for an existing owned drive session) may take the permit.
            if !self.stopped && !matches!(action, "estop"|"release"|"reset"|"idle")
                && (!matches!(action, "drive"|"strafe_left"|"strafe_right") || self.drive.is_some()) {
                self.acquire_input()?;
            }
            match action {
            "estop"=>{self.stopped=true;self.halt(true).await?;},
            "release"=>self.close().await,
            "reset"=>{self.zero();self.stopped=false;self.error.clear();},
            _ if self.stopped=>{},
            "idle"=>self.halt(false).await?,
            "prepare"|"stand"|"getup"|"standup" if self.drive.is_none()=>{
                self.zero();let cal=self.request("calibration",None).await?;
                let kind=match action {"prepare"=>"sitstand_sit","getup"=>"xgoduck_getup",_=>"sitstand_stand"};
                let result=self.request("policy/start",Some(json!({"kind":kind,"speed":0,"revision":cal["revision"]}))).await?;
                self.drive=Some(result["driveSession"].as_str().ok_or_else(||anyhow::anyhow!("missing drive session"))?.into());self.drive_seq=0;self.phase="starting".into();
                self.error.clear();
            },
            "stand"=>{self.zero();if let Some(session)=&self.drive {self.drive_seq+=1;self.request("policy/skill",Some(json!({"session":session,"sequence":self.drive_seq,"skill":"standup"}))).await?;self.pending=true;}},
            "prepare"=>anyhow::bail!("请先停止模型，再进入坐下预备"),
            "quack"=>{
                self.quack_input(head);
                if let Some(session)=&self.drive {self.drive_seq+=1;self.request("policy/quack",Some(json!({"session":session,"sequence":self.drive_seq}))).await?;}
                else {let cal=self.request("calibration",None).await?;self.request("servos/quack",Some(json!({"revision":cal["revision"]}))).await?;}
                self.error.clear();
            },
            "dance"|"getup"|"pick"|"roulade"|"kick_left"|"kick_right"|"sit"|"standup"|"mouth_open"|"mouth_close"=>{
                self.zero();let mouth=action.starts_with("mouth_");
                if let Some(session)=&self.drive {self.drive_seq+=1;
                    self.request(if mouth{"policy/mouth"}else{"policy/skill"},Some(json!({"session":session,"sequence":self.drive_seq,"skill":action,"angleDeg":if action=="mouth_open"{30}else{-5}}))).await?;self.pending=!mouth;
                } else if mouth {let cal=self.request("calibration",None).await?;self.request("servos/angle",Some(json!({"id":34,"angleDeg":if action=="mouth_open"{30}else{-5},"revision":cal["revision"]}))).await?;}
                else {anyhow::bail!("请先站起或坐下预备");}self.error.clear();
            },
            "drive"|"strafe_left"|"strafe_right"=>{
                let ctrl=self.control();
                if self.phase=="active" && ctrl["feedbackHolding"]!=true && ctrl["activeSkill"]=="xgoduck" && !self.pending {
                    ensure!(head==[0.;2] || crate::head_control::ready(&ctrl),"当前模型不支持头部控制");
                    self.app.telemetry.write().unwrap().head_intent.set(head);
                    self.error.clear();self.forward=f*speed;self.turn=(t*speed).clamp(-1.0,1.0);self.lateral=if action=="strafe_left"{0.3*speed}else if action=="strafe_right"{-0.3*speed}else{0.};
                }else{self.zero();}
            },_=>{}
        }
        Ok(()) }.await;
        if let Err(e) = execution {
            self.app.telemetry.write().unwrap().log("WARN",self.source,&format!("控制动作失败：{action}，{e}"));
            if action == "quack" { self.sync(); self.quack_input(head); } else { self.zero(); }
            self.error = e.to_string();
            if self.drive.is_none() && matches!(action, "prepare" | "stand" | "getup" | "standup") {
                self.phase = "failed".into();
            }
        }
        if self.drive.is_none() { self.input_permit = None; }
        let status = self.status();
        Ok(if body["compact"] == true {
            compact_status(status, &body)
        } else { status })
    }
    pub async fn tick(&mut self) {
        if !self.leased {
            return;
        }
        if self.last.elapsed() > Duration::from_millis(500) {
            if let Err(e) = self.halt(false).await {
                self.error = e.to_string();
            }
            return;
        }
        self.sync();
        if self.phase != "active" || self.pending {
            return;
        }
        let ctrl = self.control();
        if ctrl["activeSkill"] != "xgoduck" || ctrl["feedbackHolding"] == true {
            self.zero();
            return;
        }
        let Some(session) = self.drive.clone() else {
            return;
        };
        self.drive_seq += 1;
        let twist = if !self.stopped && self.last.elapsed() < Duration::from_millis(250) {
            [self.forward * 0.4, self.lateral, self.turn]
        } else {
            [0.; 3]
        };
        match self
            .request(
                "policy/command",
                Some(json!({"session":session,"sequence":self.drive_seq,"twist":twist})),
            )
            .await
        {
            Ok(_) => self.error.clear(),
            Err(e) => {
                self.zero();
                self.error = e.to_string();
            }
        }
    }
}
#[cfg(unix)]
pub async fn wifi(body: Value) -> Result<Value> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut stream = tokio::net::UnixStream::connect("/run/ducklink-wifi/control.sock").await?;
        let mut request = serde_json::to_vec(&body)?;
        request.push(b'\n');
        stream.write_all(&request).await?;
        let mut data = Vec::new();
        loop {
            let mut chunk = [0; 1024];
            let n = stream.read(&mut chunk).await?;
            ensure!(n > 0, "Wi-Fi服务断开");
            data.extend_from_slice(&chunk[..n]);
            ensure!(data.len() <= 8000, "Wi-Fi响应过大");
            if data.contains(&b'\n') {
                break;
            }
        }
        Ok(serde_json::from_slice(&data)?)
    })
    .await
    .map_err(|_| anyhow::anyhow!("Wi-Fi服务超时"))?
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn column_wire_preserves_all_servo_values_and_headers() {
        let servos: Vec<Value> = (10..25).map(|id|json!({"id":id,"online":false,"position":2048,"ageMs":123.456789,"fault":143,"voltage":3.8,"temperature":55})).collect();
        let full = json!({"observer":{"control":{},"joints":{"sequence":7,"bootId":"b","ageMs":42.5,"servos":servos},"calibration":{"revision":1},"system":{"bootId":"b"}}});
        let packed = compact_status(full.clone(), &json!({"telemetryEncoding":"columns-v1"}));
        let joints=&packed["observer"]["joints"];
        let columns=joints["columns"].as_array().unwrap();
        let rows:Vec<Value>=joints["values"].as_array().unwrap().iter().map(|row| {
            let mut v=json!({});for (i,key) in columns.iter().enumerate() {if !row[i].is_null(){v[key.as_str().unwrap()]=row[i].clone();}}v
        }).collect();
        assert_eq!(json!(rows),full["observer"]["joints"]["servos"]);
        for key in ["sequence","bootId","ageMs"] {assert_eq!(joints[key],full["observer"]["joints"][key]);}
        assert!(serde_json::to_vec(joints).unwrap().len()<serde_json::to_vec(&full["observer"]["joints"]).unwrap().len()*2/3);
        assert_eq!(compact_status(full.clone(), &json!({}))["observer"]["joints"],full["observer"]["joints"]);
    }
    #[test]
    fn compact_wire_keeps_safety_and_refreshes_calibration() {
        let full = json!({"motionError":"fault", "observer":{"control":{"state":"failed","message":"fault","feedbackHolding":true,"posture":"seated","kpRun":5,"kdRun":20,"cycleCosts":{"large":vec![1;1000]}},"calibration":{"revision":9},"system":{"bootId":"boot"},"joints":{"servos":[{"id":10,"fault":143}]}}});
        let same = compact_status(full.clone(), &json!({"calibrationRevision":9,"calibrationBoot":"boot"}));
        assert_eq!(same["motionError"],full["motionError"]);
        for key in ["state","message","feedbackHolding","posture","kpRun","kdRun"] {assert_eq!(same["observer"]["control"][key],full["observer"]["control"][key]);}
        assert_eq!(same["observer"]["joints"],full["observer"]["joints"]);
        assert_eq!(same["observer"]["system"],full["observer"]["system"]);
        assert!(same["observer"]["control"]["cycleCosts"].is_null());
        assert!(same["observer"]["calibration"].is_null());
        assert_eq!(same["observer"]["calibrationRevision"],9);
        for request in [json!({}),json!({"calibrationRevision":8,"calibrationBoot":"boot"}),json!({"calibrationRevision":9,"calibrationBoot":"other"})] {
            assert_eq!(compact_status(full.clone(), &request)["observer"]["calibration"],full["observer"]["calibration"]);
        }
        assert!(serde_json::to_vec(&same).unwrap().len()<serde_json::to_vec(&full).unwrap().len()/2);
    }
    use std::{
        collections::BTreeMap,
        sync::{atomic::AtomicBool, Arc, Mutex, RwLock},
    };
    pub(crate) fn fixture() -> (Motion, Arc<Mutex<Vec<(String, Value)>>>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let app = App {
            telemetry: Arc::new(RwLock::new(crate::telemetry::Telemetry::new(false))),
            cal: Arc::new(Mutex::new(
                crate::calibration::Calibration::open(tmp.path().join("cal.json")).unwrap(),
            )),
            topics: BTreeMap::new(),
            hardware: false,
            servo: false,
            tof: false,
            owner: None,
            gate: Arc::new(tokio::sync::Semaphore::new(1)),
            input_gate: Arc::new(tokio::sync::Semaphore::new(1)),
            plans: Arc::new(Mutex::new(None)),
            shutdown: Arc::new(AtomicBool::new(false)),
            gamepad_debug: Arc::new(AtomicBool::new(true)),
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        let capture = calls.clone();
        let router = Router::new().fallback(move |req: Request<Body>| {
            let capture = capture.clone();
            async move {
                let path = req.uri().path().to_string();
                let data = to_bytes(req.into_body(), 8192).await.unwrap();
                let body = serde_json::from_slice(&data).unwrap_or(Value::Null);
                capture.lock().unwrap().push((path, body));
                axum::Json(json!({"revision":1,"driveSession":"ble-session"}))
            }
        });
        (Motion::new(app, router), calls, tmp)
    }
    fn drive(seq: u64) -> Value {
        json!({"seq":seq,"action":"drive","forward":1,"turn":0,"speed":0.5})
    }
    #[test]
    fn local_status_avoids_calibration_lock_and_preserves_readiness_gates() {
        let (phone,_,_tmp)=fixture();
        let mut pad=Motion::local(phone.app.clone(),phone.router.clone());
        let _cal_guard=phone.app.cal.lock().unwrap();
        // Holding calibration cannot block the controller's internal status path.
        pad.open().unwrap();
        pad.drive=Some("ble-session".into());pad.phase="active".into();
        pad.app.telemetry.write().unwrap().control=json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck","runtimeProfile":"kissqy-sync50-v1"});
        assert_eq!(pad.status()["headReady"],true);
        assert!(pad.status().get("observer").is_none());
        pad.pending=true;assert_eq!(pad.status()["headReady"],false);
        pad.pending=false;pad.stopped=true;assert_eq!(pad.status()["headReady"],false);
        pad.stopped=false;
        pad.app.telemetry.write().unwrap().control["feedbackHolding"]=json!(true);
        assert_eq!(pad.status()["headReady"],false);
    }
    #[tokio::test]
    async fn simultaneous_connections_do_not_steal_or_clear_input_and_handoff_needs_no_reconnect() {
        let (mut phone, calls, _tmp) = fixture();
        let mut pad = Motion::local(phone.app.clone(), phone.router.clone());
        pad.open().unwrap();
        pad.acquire_input().unwrap();
        pad.drive = Some("ble-session".into());
        pad.phase = "active".into();
        pad.app.telemetry.write().unwrap().control = json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck","runtimeProfile":"kissqy-sync50-v1"});
        pad.app.telemetry.write().unwrap().head_intent.set([0.5,-0.5]);
        assert_eq!(phone.open().unwrap()["controlBusy"], true);
        phone.command(drive(1)).await.unwrap();
        phone.command(json!({"seq":2,"action":"reset"})).await.unwrap();
        phone.command(json!({"seq":3,"action":"idle"})).await.unwrap();
        phone.tick().await;
        let denied = phone.command(json!({"seq":4,"action":"stand"})).await.unwrap();
        assert!(denied["motionError"].as_str().unwrap().contains("另一端"));
        let denied_quack = phone.command(json!({"seq":5,"action":"quack"})).await.unwrap();
        assert!(denied_quack["motionError"].as_str().unwrap().contains("另一端"));
        assert_eq!(pad.app.telemetry.read().unwrap().head_intent.current(Instant::now()),[0.5,-0.5]);
        assert!(calls.lock().unwrap().is_empty());
        phone.close().await;
        assert!(pad.input_permit.is_some());
        phone.open().unwrap();
        pad.command(json!({"seq":1,"action":"idle"})).await.unwrap();
        assert!(pad.leased && phone.leased);
        phone.app.telemetry.write().unwrap().drive = Some(crate::telemetry::DriveCommand {session:"ble-session".into(),sequence:0,twist:[0.;3],updated:Instant::now()});
        phone.command(json!({"seq":1,"action":"stand"})).await.unwrap();
        assert!(phone.input_permit.is_some());
        assert!(pad.acquire_input().is_err());
    }
    #[tokio::test]
    async fn nonowner_emergency_stop_is_global_and_idle_connection_timeout_is_harmless() {
        let (mut phone,calls,_tmp)=fixture();
        let mut pad=Motion::local(phone.app.clone(),phone.router.clone());
        phone.open().unwrap();pad.open().unwrap();
        pad.acquire_input().unwrap();
        pad.app.telemetry.write().unwrap().head_intent.set([0.5,0.5]);
        phone.last=Instant::now()-Duration::from_secs(1);
        phone.tick().await;
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(pad.app.telemetry.read().unwrap().head_intent.current(Instant::now()),[0.5,0.5]);
        phone.command(json!({"seq":1,"action":"estop"})).await.unwrap();
        assert_eq!(calls.lock().unwrap().last().unwrap().0,"/api/v1/servos/disable");
        assert!(phone.leased && pad.leased);
    }
    #[tokio::test]
    async fn power_checks_current_boot_and_idle_control_before_using_shared_route() {
        let (mut motion, calls, _tmp) = fixture();
        motion.open().unwrap();
        motion.app.hardware = true;
        let boot = motion.app.telemetry.read().unwrap().boot.clone();
        assert!(motion.power(json!({"type":"power","action":"reboot","confirm":true,"bootId":"old"})).await.is_err());
        assert!(motion.power(json!({"type":"power","action":"poweroff","bootId":boot})).await.is_err());
        motion.app.telemetry.write().unwrap().control=json!({"state":"policy"});
        assert!(motion.power(json!({"type":"power","action":"reboot","confirm":true,"bootId":boot})).await.is_err());
        assert!(calls.lock().unwrap().is_empty());
        motion.app.telemetry.write().unwrap().control=json!({"state":"idle"});
        for action in ["reboot","poweroff"] {
            motion.power(json!({"type":"power","action":action,"confirm":true,"bootId":boot})).await.unwrap();
        }
        let requests=calls.lock().unwrap();
        assert_eq!(requests.len(),2);
        assert_eq!(requests[0],("/api/v1/system/reboot".into(),json!({"confirm":true,"bootId":boot})));
        assert_eq!(requests[1].0,"/api/v1/system/poweroff");
    }
    #[tokio::test]
    async fn observer_ota_rejects_wrong_boot_missing_confirmation_busy_and_enabled_servos() {
        let (mut motion,calls,_tmp)=fixture();
        assert!(motion.observer_ota(json!({"op":"check"})).await.is_err());
        motion.open().unwrap();assert!(motion.observer_ota(json!({"op":"apply","confirm":true})).await.is_err());
        motion.app.hardware=true;let boot=motion.app.telemetry.read().unwrap().boot.clone();
        assert!(motion.observer_ota(json!({"op":"apply","bootId":boot})).await.is_err());
        assert!(motion.observer_ota(json!({"op":"apply","confirm":true,"bootId":"old"})).await.is_err());
        let request=json!({"op":"apply","confirm":true,"bootId":boot});
        let permit=motion.app.gate.clone().try_acquire_owned().unwrap();assert!(motion.observer_ota(request.clone()).await.is_err());drop(permit);
        assert!(motion.observer_ota(request.clone()).await.is_err());
        motion.app.telemetry.write().unwrap().sample("joints",json!({"servos":(0..15).map(|_|json!({"online":true,"torque":1,"ageMs":0})).collect::<Vec<_>>()}),true,Instant::now());
        motion.app.telemetry.write().unwrap().control=json!({"state":"idle"});
        assert!(motion.observer_ota(request).await.is_err());assert!(calls.lock().unwrap().is_empty());
    }
    #[test]
    fn status_includes_resource_age_and_boot_for_app() {
        let (mut motion, _, _tmp) = fixture();
        motion.app.telemetry.write().unwrap().resources=json!({"cpuUsagePercent":12.,"sampleMonoMs":crate::telemetry::monotonic()*1000.});
        let status=motion.status();
        assert_eq!(status["observer"]["system"]["resources"]["cpuUsagePercent"],12.);
        assert!(status["observer"]["system"]["bootId"].is_string());
        assert!(status["observer"]["system"]["resourceAgeMs"].as_f64().unwrap()>=0.);
        assert_eq!(status["observer"]["system"]["powerControl"],false);
    }
    #[tokio::test]
    async fn backward_drive_reaches_policy_with_negative_velocity_and_release_zeroes_it() {
        let (mut m, calls, _tmp) = fixture();
        m.open().unwrap();
        m.drive = Some("ble-session".into());
        m.phase = "active".into();
        m.app.telemetry.write().unwrap().control = json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck","feedbackHolding":false});
        m.command(json!({"seq":1,"action":"drive","forward":-1,"turn":0,"speed":0.5})).await.unwrap();
        m.tick().await;
        let request=calls.lock().unwrap().last().unwrap().clone();
        assert_eq!(request.0,"/api/v1/policy/command");
        assert_eq!(crate::policy::drive_twist(&request.1).unwrap(),[-0.2,0.,0.]);
        m.command(json!({"seq":2,"action":"drive","forward":0,"turn":0,"speed":0.5})).await.unwrap();
        m.tick().await;
        assert_eq!(calls.lock().unwrap().last().unwrap().1["twist"],json!([0.,0.,0.]));
    }
    #[tokio::test]
    async fn seated_prepare_starts_policy_and_rise_keeps_same_session() {
        let (mut m,calls,_tmp)=fixture();m.open().unwrap();
        m.app.telemetry.write().unwrap().drive=Some(crate::telemetry::DriveCommand{session:"ble-session".into(),sequence:0,twist:[0.;3],updated:Instant::now()});
        m.command(json!({"seq":1,"action":"prepare"})).await.unwrap();
        let request=calls.lock().unwrap().last().unwrap().clone();
        assert_eq!(request.0,"/api/v1/policy/start");assert_eq!(request.1["kind"],"sitstand_sit");
        assert_eq!(m.drive.as_deref(),Some("ble-session"));m.phase="active".into();
        m.app.telemetry.write().unwrap().control=json!({"state":"policy","driveSession":"ble-session","activeSkill":"sitstand_sit","sitSettleSeconds":4,"skillProgressSeconds":4.1});
        assert_eq!(m.status()["sitSettleSeconds"],4.);
        m.command(json!({"seq":2,"action":"standup"})).await.unwrap();
        let request=calls.lock().unwrap().last().unwrap().clone();
        assert_eq!(request.0,"/api/v1/policy/skill");assert_eq!(request.1["session"],"ble-session");assert_eq!(request.1["skill"],"standup");
    }
    #[tokio::test]
    async fn pure_yaw_preserves_both_signs_without_forward_motion() {
        let (mut m,calls,_tmp)=fixture();
        m.open().unwrap();m.drive=Some("ble-session".into());m.phase="active".into();
        m.app.telemetry.write().unwrap().control=json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck","feedbackHolding":false});
        for (seq,turn) in [(1,1.),(2,-1.)] {
            m.command(json!({"seq":seq,"action":"drive","forward":0,"turn":turn,"speed":0.5})).await.unwrap();
            m.tick().await;
            assert_eq!(crate::policy::drive_twist(&calls.lock().unwrap().last().unwrap().1).unwrap(),[0.,0.,turn as f32*0.5]);
        }
    }
    #[tokio::test]
    async fn full_speed_preserves_other_axis_limits() {
        let (mut m,calls,_tmp)=fixture();
        m.open().unwrap();m.drive=Some("ble-session".into());m.phase="active".into();
        m.app.telemetry.write().unwrap().control=json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck","feedbackHolding":false});
        for (seq,speed,forward,action,expected) in [
            (1,0.5,1.,"drive",[0.2,0.,0.5]),
            (2,1.,1.,"drive",[0.4,0.,1.]),
            (3,1.,-1.,"drive",[-0.4,0.,1.]),
            (4,1.,0.,"strafe_left",[0.,0.3,1.]),
            (5,1.,0.,"strafe_right",[0.,-0.3,1.]),
            (6,0.,1.,"drive",[0.,0.,0.]),
        ] {
            m.command(json!({"seq":seq,"action":action,"forward":forward,"turn":1.,"speed":speed})).await.unwrap();
            m.tick().await;
            assert_eq!(crate::policy::drive_twist(&calls.lock().unwrap().last().unwrap().1).unwrap(),expected);
        }
        assert!(m.command(json!({"seq":7,"action":"drive","forward":1.,"turn":0.,"speed":1.01})).await.is_err());
    }
    #[tokio::test]
    async fn reconnect_defaults_ready_without_motor_calls() {
        let (mut m, calls, _tmp) = fixture();
        assert_eq!(m.open().unwrap()["stopped"], false);
        m.command(json!({"seq":1,"action":"estop"})).await.unwrap();
        assert!(m.stopped);
        m.close().await;
        calls.lock().unwrap().clear();
        let status = m.open().unwrap();
        assert_eq!(status["stopped"], false);
        assert_eq!(status["mode"], "idle");
        assert!(calls.lock().unwrap().is_empty());
        m.command(drive(2)).await.unwrap();
        m.tick().await;
        assert!(calls.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn retry_getup_ignores_previous_failure_and_becomes_controllable() {
        let (mut m,calls,_tmp)=fixture();m.open().unwrap();
        m.app.telemetry.write().unwrap().control=json!({"state":"failed","message":"previous roll timeout"});
        m.app.telemetry.write().unwrap().drive=Some(crate::telemetry::DriveCommand{session:"ble-session".into(),sequence:0,twist:[0.;3],updated:Instant::now()});
        let response=m.command(json!({"seq":1,"action":"getup"})).await.unwrap();
        assert_eq!(response["policyPhase"],"starting");assert_eq!(m.drive.as_deref(),Some("ble-session"));
        m.app.telemetry.write().unwrap().control=json!({"state":"failed","driveSession":"previous-session","message":"previous roll timeout"});
        assert_eq!(m.status()["policyPhase"],"starting");
        m.app.telemetry.write().unwrap().control=json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck_getup"});
        assert_eq!(m.status()["activeSkill"],"xgoduck_getup");
        m.app.telemetry.write().unwrap().control=json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck","feedbackHolding":false});
        assert_eq!(m.status()["policyPhase"],"active");
        m.command(drive(2)).await.unwrap();m.tick().await;
        let last=calls.lock().unwrap().last().unwrap().clone();assert_eq!(last.0,"/api/v1/policy/command");assert_eq!(last.1["twist"][0],0.2);
    }
    #[tokio::test]
    async fn current_episode_preflight_failure_still_clears_the_lease() {
        let (mut m,_calls,_tmp)=fixture();m.open().unwrap();m.drive=Some("ble-session".into());m.phase="starting".into();
        m.app.telemetry.write().unwrap().control=json!({"state":"failed","driveSession":"ble-session","message":"current preflight failed"});
        assert_eq!(m.status()["policyPhase"],"failed");assert!(m.drive.is_none());assert_eq!(m.error,"current preflight failed");
    }
    #[tokio::test]
    async fn paused_feedback_cancels_intent_and_disconnect_halts_only_owned_policy() {
        let (mut m, calls, _tmp) = fixture();
        m.open().unwrap();
        m.drive = Some("ble-session".into());
        m.phase = "active".into();
        m.app.telemetry.write().unwrap().control = json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck","feedbackHolding":false});
        m.command(drive(1)).await.unwrap();
        m.tick().await;
        assert_eq!(calls.lock().unwrap().last().unwrap().1["twist"][0], 0.2);
        m.app.telemetry.write().unwrap().control["feedbackHolding"] = json!(true);
        m.tick().await;
        assert_eq!(m.forward, 0.);
        m.app.telemetry.write().unwrap().control["feedbackHolding"] = json!(false);
        m.tick().await;
        assert_eq!(
            calls.lock().unwrap().last().unwrap().1["twist"],
            json!([0., 0., 0.])
        );
        m.close().await;
        assert_eq!(
            calls.lock().unwrap().last().unwrap().0,
            "/api/v1/policy/stop"
        );
        m.open().unwrap();
        calls.lock().unwrap().clear();
        m.app.telemetry.write().unwrap().control["driveSession"] = json!("web-session");
        m.close().await;
        assert!(calls.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn stale_sequence_and_bad_values_do_not_renew_timeout() {
        let (mut m, calls, _tmp) = fixture();
        m.open().unwrap();
        m.command(drive(1)).await.unwrap();
        assert!(m.command(drive(1)).await.is_err());
        let last = m.last;
        assert!(m
            .command(json!({"seq":2,"action":"drive","forward":true}))
            .await
            .is_err());
        assert_eq!(m.last, last);
        m.drive = Some("ble-session".into());
        m.phase = "active".into();
        m.app.telemetry.write().unwrap().control =
            json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck"});
        m.last = Instant::now() - Duration::from_secs(1);
        m.tick().await;
        assert!(!m.stopped);
        assert!(m.drive.is_none());
        assert_eq!(
            calls.lock().unwrap().last().unwrap().0,
            "/api/v1/policy/stop"
        );
    }
    #[tokio::test]
    async fn quack_preserves_current_head_even_when_mouth_busy_but_honors_safety() {
        let (mut m,calls,_tmp)=fixture();m.open().unwrap();
        m.drive=Some("ble-session".into());m.phase="active".into();
        m.app.telemetry.write().unwrap().control=json!({"state":"policy","driveSession":"ble-session","activeSkill":"xgoduck","runtimeProfile":"kissqy-sync50-v1","feedbackHolding":false});
        m.forward=0.5;m.turn=0.5;
        m.command(json!({"seq":1,"action":"quack","headPitch":0.6,"headYaw":-0.7})).await.unwrap();
        assert_eq!(calls.lock().unwrap().last().unwrap().0,"/api/v1/policy/quack");
        assert_eq!((m.forward,m.turn,m.lateral),(0.,0.,0.));
        assert_eq!(m.app.telemetry.read().unwrap().head_intent.current(Instant::now()),[0.6,-0.7]);
        m.router=Router::new().fallback(||async{(axum::http::StatusCode::CONFLICT,axum::Json(json!({"detail":"mouth busy"})))});
        m.command(json!({"seq":2,"action":"quack","headPitch":-0.4,"headYaw":0.8})).await.unwrap();
        assert_eq!(m.error,"mouth busy");
        assert_eq!(m.app.telemetry.read().unwrap().head_intent.current(Instant::now()),[-0.4,0.8]);
        assert_eq!(m.app.telemetry.read().unwrap().head_intent.current(Instant::now()+Duration::from_millis(251)),[0.,0.]);
        m.command(json!({"seq":3,"action":"quack"})).await.unwrap();
        assert_eq!(m.app.telemetry.read().unwrap().head_intent.current(Instant::now()),[0.,0.]);
        m.app.telemetry.write().unwrap().control["feedbackHolding"]=json!(true);
        m.command(json!({"seq":4,"action":"quack","headPitch":1})).await.unwrap();
        assert_eq!(m.app.telemetry.read().unwrap().head_intent.current(Instant::now()),[0.,0.]);
        m.app.telemetry.write().unwrap().control["feedbackHolding"]=json!(false);
        m.stopped=true;
        m.command(json!({"seq":5,"action":"quack","headPitch":1})).await.unwrap();
        assert_eq!(m.app.telemetry.read().unwrap().head_intent.current(Instant::now()),[0.,0.]);
    }

}
