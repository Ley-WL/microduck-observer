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

pub struct Motion {
    app: App,
    router: Router,
    pub leased: bool,
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
    home_reply: Option<tokio::sync::oneshot::Receiver<Result<Value>>>,
}
impl Motion {
    pub fn new(app: App, router: Router) -> Self {
        Self {
            app,
            router,
            leased: false,
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
            home_reply: None,
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
    fn zero(&mut self) {
        self.forward = 0.;
        self.turn = 0.;
        self.lateral = 0.;
    }
    fn control(&self) -> Value {
        self.app.telemetry.read().unwrap().control.clone()
    }
    fn sync(&mut self) {
        if let Some(reply) = self.home_reply.as_mut() {
            match reply.try_recv() {
                Ok(result) => {
                    self.home_reply = None;
                    self.pending = false;
                    match result {
                        Ok(_) => self.phase = "prepared".into(),
                        Err(e) => {
                            self.phase = "failed".into();
                            self.error = e.to_string();
                        }
                    }
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(_) => {
                    self.home_reply = None;
                    self.pending = false;
                    self.phase = "failed".into();
                    self.error = "舵机服务已退出".into();
                }
            }
        }
        let ctrl = self.control();
        if let Some(session) = &self.drive {
            if ctrl["state"] == "failed" {
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
    }
    pub fn status(&mut self) -> Value {
        self.sync();
        let s = self.app.telemetry.read().unwrap();
        let ctrl = s.control.clone();
        let mut observer = json!({"control":ctrl});
        observer["control"]["ageMs"] = json!(0);
        for (topic, target) in [("joints", "joints"), ("imu.orientation", "orientation")] {
            if let Some(sample) = s.latest.get(topic) {
                let v = s.stamp(sample);
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
        observer["calibration"] = self.app.cal.lock().unwrap().read(&s.boot);
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
        json!({"simulation":!self.app.hardware,"motion":self.app.owner.is_some() && crate::policy::metadata_for("xgoduck").is_some(),"mode":mode,"stopped":self.stopped,"forward":self.forward,"turn":self.turn,"attitude":null,"voltage":null,"imu":"integrated","protocol":1,"activeSkill":skill,"skillProgressSeconds":ctrl["skillProgressSeconds"].as_f64().unwrap_or(0.),"skillsPending":self.pending,"policy":"xgoduck","policyPhase":self.phase,"motionError":self.error,"observer":observer})
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
    pub async fn halt(&mut self, unload: bool) -> Result<()> {
        self.sync();
        self.zero();
        let preparing = self.home_reply.take().is_some();
        self.pending = false;
        // Only cancel this BLE controller's policy; explicit emergency stop unloads globally.
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
        if unload {
            self.request("servos/disable", Some(json!({}))).await?;
        } else if owned || starting || preparing {
            self.request("policy/stop", Some(json!({}))).await?;
        }
        Ok(())
    }
    pub async fn close(&mut self) {
        let _ = self.halt(false).await;
        self.leased = false;
        self.stopped = false;
        self.sequence = None;
    }
    pub async fn command(&mut self, body: Value) -> Result<Value> {
        ensure!(self.leased, "控制会话无效，请重新连接");
        let seq = body["seq"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("缺少指令序号"))?;
        ensure!(self.sequence.is_none_or(|last| seq > last), "指令序号过期");
        let action = body["action"].as_str().unwrap_or("");
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
                "sit",
                "standup",
                "mouth_open",
                "mouth_close"
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
            (-1. ..=1.).contains(&f) && (-1. ..=1.).contains(&t) && (0. ..=0.5).contains(&speed),
            "控制量超出范围"
        );
        self.sequence = Some(seq);
        self.last = Instant::now();
        self.sync();
        let execution:Result<()> = async { match action {
            "estop"=>{self.stopped=true;self.halt(true).await?;},
            "release"=>self.close().await,
            "reset"=>{self.zero();self.stopped=false;self.error.clear();},
            _ if self.stopped=>{},
            "idle"=>self.halt(false).await?,
            "prepare"|"stand"|"getup"|"standup" if self.drive.is_none()=>{
                ensure!(self.home_reply.is_none(),"预备动作处理中，请稍候");self.zero();let cal=self.request("calibration",None).await?;
                let kind=match action {"getup"=>"xgoduck_getup","standup"=>"sitstand_stand",_=>"xgoduck"};
                if action=="prepare" {
                    let owner=self.app.owner.as_ref().filter(|_|self.app.hardware).ok_or_else(||anyhow::anyhow!("需要实机舵机服务"))?;
                    let permit=self.app.gate.clone().try_acquire_owned().map_err(|_|anyhow::anyhow!("已有标定或运动任务正在执行"))?;
                    let boot=self.app.telemetry.read().unwrap().boot.clone();let current_cal=self.app.cal.lock().unwrap().read(&boot);
                    self.home_reply=Some(owner.submit(crate::policy_home_command(kind,current_cal)?,Some(permit))?);
                    self.phase="preparing".into();self.pending=true;
                } else {
                    let result=self.request("policy/start",Some(json!({"kind":kind,"speed":0,"revision":cal["revision"]}))).await?;
                    self.drive=Some(result["driveSession"].as_str().ok_or_else(||anyhow::anyhow!("missing drive session"))?.into());self.drive_seq=0;self.phase="starting".into();
                }
                self.error.clear();
            },
            "stand"=>{},
            "prepare"=>anyhow::bail!("请先停止模型，再进入预备姿势"),
            "getup"|"pick"|"roulade"|"sit"|"standup"|"mouth_open"|"mouth_close"=>{
                self.zero();let mouth=action.starts_with("mouth_");
                if let Some(session)=&self.drive {self.drive_seq+=1;
                    self.request(if mouth{"policy/mouth"}else{"policy/skill"},Some(json!({"session":session,"sequence":self.drive_seq,"skill":action,"angleDeg":if action=="mouth_open"{30}else{0}}))).await?;self.pending=!mouth;
                } else if mouth {let cal=self.request("calibration",None).await?;self.request("servos/angle",Some(json!({"id":34,"angleDeg":if action=="mouth_open"{30}else{0},"revision":cal["revision"]}))).await?;}
                else {anyhow::bail!("请先启动平衡");}self.error.clear();
            },
            "drive"|"strafe_left"|"strafe_right"=>{
                let ctrl=self.control();
                if self.phase=="active" && ctrl["feedbackHolding"]!=true && ctrl["activeSkill"]=="xgoduck" && !self.pending {
                    self.error.clear();self.forward=f*speed;self.turn=t*speed;self.lateral=if action=="strafe_left"{0.2*speed}else if action=="strafe_right"{-0.2*speed}else{0.};
                }else{self.zero();}
            },_=>{}
        }
        Ok(()) }.await;
        if let Err(e) = execution {
            self.zero();
            self.error = e.to_string();
            if self.drive.is_none() && matches!(action, "prepare" | "stand" | "getup" | "standup") {
                self.phase = "failed".into();
            }
        }
        Ok(self.status())
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
            plans: Arc::new(Mutex::new(None)),
            shutdown: Arc::new(AtomicBool::new(false)),
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
    async fn prepare_reply_does_not_block_stop_and_late_reply_cannot_restore() {
        let (mut m, calls, _tmp) = fixture();
        m.open().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        m.home_reply = Some(rx);
        m.phase = "preparing".into();
        m.pending = true;
        let status = m.command(drive(1)).await.unwrap();
        assert_eq!(status["policyPhase"], "preparing");
        assert!(calls.lock().unwrap().is_empty());
        m.command(json!({"seq":2,"action":"idle"})).await.unwrap();
        assert!(tx.send(Ok(json!({}))).is_err());
        assert_eq!(m.status()["policyPhase"], "idle");
        assert_eq!(
            calls.lock().unwrap().last().unwrap().0,
            "/api/v1/policy/stop"
        );
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
}
