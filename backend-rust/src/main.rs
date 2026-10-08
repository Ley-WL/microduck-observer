mod calibration;
mod control;
mod ducklink;
mod ducklink_motion;
mod ducklink_protocol;
mod guided;
mod imu;
mod orientation;
mod policy;
mod power;
mod resources;
mod servos;
mod telemetry;
mod tof;
use axum::{
    body::Bytes,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Path, Query, State,
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use calibration::Calibration;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};
use telemetry::{rates, Shared, Telemetry, IDS};
use tower_http::{cors::CorsLayer, services::ServeDir};
#[derive(Clone)]
struct App {
    telemetry: Shared,
    cal: Arc<Mutex<Calibration>>,
    topics: BTreeMap<String, Value>,
    hardware: bool,
    servo: bool,
    tof: bool,
    owner: Option<control::Owner>,
    gate: Arc<tokio::sync::Semaphore>,
    plans: guided::Plans,
    shutdown: Arc<AtomicBool>,
}
fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"detail":message}))).into_response()
}
fn check_write(headers: &HeaderMap, body: &Bytes, max: usize) -> Result<Value, Response> {
    if let Some(origin) = headers.get("origin") {
        let host = headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let http = format!("http://{host}");
        if ![
            http.as_str(),
            "http://localhost:5173",
            "http://127.0.0.1:5173",
        ]
        .contains(&origin.to_str().unwrap_or(""))
        {
            return Err(error(StatusCode::FORBIDDEN, "Origin not allowed"));
        }
    }
    if headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        != Some("application/json")
    {
        return Err(error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "JSON required"));
    }
    if body.len() > max {
        return Err(error(StatusCode::PAYLOAD_TOO_LARGE, "Request too large"));
    }
    let v: Value = serde_json::from_slice(body)
        .map_err(|_| error(StatusCode::UNPROCESSABLE_ENTITY, "Invalid request"))?;
    if !v.is_object() {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "Invalid request"));
    }
    Ok(v)
}
fn check_action(h: &HeaderMap, b: &Bytes, max: usize) -> Result<Value, Response> {
    check_write(h, b, max).map_err(|r| {
        if r.status() == StatusCode::UNPROCESSABLE_ENTITY {
            error(StatusCode::CONFLICT, "Invalid request")
        } else {
            r
        }
    })
}
async fn info(State(a): State<App>) -> Json<Value> {
    let s = a.telemetry.read().unwrap();
    Json(
        json!({"name":if a.hardware{"MicroDuck · MS901M"}else{"MicroDuck Lab"},"imuModel":if a.hardware{json!("MS901M")}else{Value::Null},"protocolVersion":1,"bootId":s.boot,"source":s.source,"topics":a.topics,"capabilities":{"pose":!a.hardware,"imu":true,"sensorOrientation":a.hardware,"joints":a.servo,"camera":false,"tof":a.tof,"scenarios":!a.hardware}}),
    )
}
async fn health(State(a): State<App>) -> Json<Value> {
    let s = a.telemetry.read().unwrap();
    Json(
        json!({"status":if a.hardware&&s.imu_health()["state"]!="streaming"{"degraded"}else{"ok"},"source":s.source,"imu":s.imu_health(),"bluetooth":s.ble_health,"tof":if a.tof{s.tof_health()}else{Value::Null},"joints":s.latest.get("joints").map(|v|s.stamp(v)),"logCount":s.logs.len()}),
    )
}
async fn power_action(State(a): State<App>, Path(action): Path<String>, headers: HeaderMap, body: Bytes) -> Response {
    let value = match check_write(&headers, &body, 256) { Ok(v) => v, Err(r) => return r };
    if !a.hardware || !power::enabled() {
        return error(StatusCode::FORBIDDEN, "当前设备未启用电源控制");
    }
    let boot = a.telemetry.read().unwrap().boot.clone();
    if !power::validate(&action, &value, &boot) {
        return error(StatusCode::CONFLICT, "请重新连接当前主板并确认关机或重启");
    }
    match tokio::time::timeout(Duration::from_secs(3), power::request(&action)).await {
        Ok(Ok(v)) => (StatusCode::ACCEPTED, Json(v)).into_response(),
        Ok(Err(e)) => error(StatusCode::SERVICE_UNAVAILABLE, &format!("电源请求失败：{e}")),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "电源请求结果未确认，请检查主板状态，勿直接断电"),
    }
}
async fn snapshot(State(a): State<App>) -> Json<Value> {
    let s = a.telemetry.read().unwrap();
    let mut out: BTreeMap<String, Value> = s
        .latest
        .iter()
        .map(|(k, v)| (k.clone(), s.stamp(v)))
        .collect();
    out.insert("calibration".into(), a.cal.lock().unwrap().read(&s.boot));
    Json(json!(out))
}
async fn get_cal(State(a): State<App>) -> Json<Value> {
    let boot = a.telemetry.read().unwrap().boot.clone();
    Json(a.cal.lock().unwrap().read(&boot))
}
async fn set_cal(State(a): State<App>, h: HeaderMap, b: Bytes) -> Response {
    let v = match check_write(&h, &b, 16384) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let _permit = match a.gate.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return error(StatusCode::CONFLICT, "舵机或标定任务运行中，请先停止模型"),
    };
    let boot = a.telemetry.read().unwrap().boot.clone();
    let mut c = a.cal.lock().unwrap();
    if let Err(e) = calibration::validate(&v["patch"]) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string());
    }
    match c.update(&v["revision"], &v["patch"], &boot, v["migrate"] == true) {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(StatusCode::CONFLICT, &e.to_string()),
    }
}
async fn limits() -> Json<Value> {
    let map: BTreeMap<String, Value> = IDS
        .iter()
        .map(|id| (id.to_string(), json!(telemetry::limit(*id).unwrap())))
        .collect();
    Json(json!({"limits":map,"unit":"deg"}))
}
async fn poses() -> Json<Value> {
    Json(serde_json::from_str(include_str!("poses.json")).expect("bundled poses"))
}
async fn guided_action(
    State(a): State<App>,
    Path(action): Path<String>,
    h: HeaderMap,
    b: Bytes,
) -> Response {
    if !["preview", "execute"].contains(&action.as_str()) {
        return error(StatusCode::NOT_FOUND, "Not Found");
    }
    let body = match check_action(&h, &b, 16384) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let permit = match a.gate.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return error(StatusCode::CONFLICT, "已有标定或运动任务正在执行"),
    };
    // A browser disconnect does not drop the lock while EEPROM can still write.
    let task = tokio::spawn(async move {
        let _permit = permit;
        if action == "preview" {
            guided::preview(&body, &a.telemetry, &a.cal, &a.plans).await
        } else if body["confirm"] != true {
            Err(anyhow::anyhow!("请确认姿势和写入预览"))
        } else {
            guided::execute(
                &body["token"],
                &a.telemetry,
                &a.cal,
                &a.plans,
                a.owner.as_ref(),
            )
            .await
        }
    });
    match task.await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error(StatusCode::CONFLICT, &e.to_string()),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}
async fn servo_action(
    State(a): State<App>,
    Path(action): Path<String>,
    h: HeaderMap,
    b: Bytes,
) -> Response {
    if !["enable", "disable", "stand", "angle", "profile"].contains(&action.as_str()) {
        return error(StatusCode::NOT_FOUND, "Not Found");
    }
    let v = match check_action(&h, &b, 1024) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(owner) = a.owner.as_ref() else {
        return error(StatusCode::CONFLICT, "未连接实机舵机服务");
    };
    let permit = if action == "disable" {
        None
    } else {
        match a.gate.clone().try_acquire_owned() {
            Ok(p) => Some(p),
            Err(_) => return error(StatusCode::CONFLICT, "已有标定或运动任务正在执行"),
        }
    };
    let boot = a.telemetry.read().unwrap().boot.clone();
    let cal = a.cal.lock().unwrap().read(&boot);
    if ["stand", "angle"].contains(&action.as_str()) && v["revision"] != cal["revision"] {
        return error(StatusCode::CONFLICT, "标定已更新，请刷新后重试");
    }
    if action == "stand"
        && (v["confirmCalibration"] != true
            || IDS[..14]
                .iter()
                .any(|id| cal["joints"]["references"].get(id.to_string()).is_none()))
    {
        return error(
            StatusCode::CONFLICT,
            "请确认位置标定正确并完成双腿及头颈14个关节标定",
        );
    }
    if ["angle", "profile"].contains(&action.as_str()) {
        let degrees = if action == "angle" {
            v["angleDeg"].clone()
        } else {
            json!(0)
        };
        if let Err(e) = control::angle_request(&v["id"], &degrees) {
            return error(StatusCode::CONFLICT, &e.to_string());
        }
    }
    let command = json!({"action":action,"calibration":cal,"id":v["id"],"angleDeg":v["angleDeg"]});
    let rx = match owner.submit(command, permit) {
        Ok(rx) => rx,
        Err(e) => return error(StatusCode::CONFLICT, &e.to_string()),
    };
    match rx.await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error(StatusCode::CONFLICT, &e.to_string()),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "舵机服务已退出"),
    }
}
async fn logs(State(a): State<App>, Query(q): Query<BTreeMap<String, String>>) -> Response {
    if q.get("cursor").is_some_and(|v| v.parse::<u64>().is_err())
        || q.get("limit").is_some_and(|v| v.parse::<usize>().is_err())
    {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "Invalid log query");
    }
    let cursor = q
        .get("cursor")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(200);
    if !(1..=2000).contains(&limit) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "Invalid limit");
    }
    let s = a.telemetry.read().unwrap();
    let items: Vec<Value> = s
        .logs
        .iter()
        .filter(|v| v["seq"].as_u64().unwrap() > cursor)
        .take(limit)
        .map(|v| s.stamp(v))
        .collect();
    let next = items
        .last()
        .map(|v| v["seq"].as_u64().unwrap())
        .unwrap_or(cursor);
    Json(json!({"items":items,"nextCursor":next,"gap":cursor>0&&s.logs.front().is_some_and(|v|cursor<v["seq"].as_u64().unwrap()-1)})).into_response()
}
async fn scenario(State(a): State<App>, Json(v): Json<Value>) -> Response {
    if a.hardware {
        return error(
            StatusCode::NOT_FOUND,
            "Simulator controls are unavailable on hardware",
        );
    }
    let name = v["name"].as_str().unwrap_or("");
    if !["motion", "steady", "imu_pause", "invalid", "log_burst"].contains(&name) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "Invalid scenario");
    }
    let mut s = a.telemetry.write().unwrap();
    if name == "log_burst" {
        for i in 0..100 {
            s.log(
                if i % 5 == 0 { "WARN" } else { "INFO" },
                "scenario",
                &format!("日志压力场景 · 事件 {}", i + 1),
            );
        }
    } else {
        s.scenario = name.into();
    }
    s.log("INFO", "scenario", &format!("切换场景：{name}"));
    Json(json!({"scenario":name})).into_response()
}
async fn policy_info(State(a): State<App>) -> Json<Value> {
    let s = a.telemetry.read().unwrap();
    let meta = policy::metadata_for("stand");
    let models: Vec<Value> = ["stand","walk","xgoduck","xgoduck_getup","xgoduck_pick","xgoduck_roulade","sitstand_sit","sitstand_stand"].iter().map(|kind| {
        let metadata=policy::metadata_for(kind);
        json!({"kind":kind,"policy":policy::model_id(kind),"available":metadata.is_some(),"metadata":metadata})
    }).collect();
    Json(
        json!({"available":meta.is_some(),"policy":"hd1910-head-v5","kind":"stand","controlHz":50,"metadata":meta,"models":models,"skillActions":["getup","pick","roulade","sit","standup"],"control":if a.servo{s.control.clone()}else{Value::Null}}),
    )
}
fn policy_home_command(kind:&str,cal:Value)->anyhow::Result<Value> {
    let targets=policy::home_targets_for(kind)?;
    Ok(json!({"action":"stand","calibration":cal,"standTargets":targets,"policy":policy::model_id(kind),"kind":kind}))
}
async fn policy_action(
    State(a): State<App>,
    Path(action): Path<String>,
    h: HeaderMap,
    b: Bytes,
) -> Response {
    if !["start", "stop", "shadow", "home", "command", "skill", "mouth"].contains(&action.as_str()) {
        return error(StatusCode::NOT_FOUND, "Not Found");
    }
    let body = match check_action(&h, &b, 1024) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(owner) = a.owner.as_ref().filter(|_| a.hardware) else {
        return error(StatusCode::CONFLICT, "需要实机传感器与舵机服务");
    };
    if action == "stop" {
        let mut shared=a.telemetry.write().unwrap();shared.drive=None;shared.pending_skill=None;
        owner.halt.store(true, Ordering::Release);
        return Json(json!({"state":"stopping","message":"停止模型请求已发送，保持最后目标"}))
            .into_response();
    }
    if action=="skill" {
        let kind=match policy::skill_kind(&body["skill"]) {Ok(v)=>v,Err(e)=>return error(StatusCode::BAD_REQUEST,&e.to_string())};
        if policy::metadata_for(kind).is_none() {return error(StatusCode::CONFLICT,"动作模型不可用");}
        let Some(sequence)=body["sequence"].as_u64() else {return error(StatusCode::BAD_REQUEST,"缺少动作序号");};
        let mut shared=a.telemetry.write().unwrap();
        if !shared.queue_skill(body["session"].as_str().unwrap_or(""),sequence,kind) {
            return error(StatusCode::CONFLICT,"动作会话/序号无效，或当前动作/反馈尚未就绪");
        }
        return Json(json!({"state":"queued","skill":body["skill"],"sequence":sequence})).into_response();
    }
    if action=="mouth" {
        let Some(degrees)=body["angleDeg"].as_f64().filter(|v|v.is_finite() && (0. ..=30.).contains(v)) else {return error(StatusCode::BAD_REQUEST,"嘴部角度须为0–30°");};
        let Some(sequence)=body["sequence"].as_u64() else {return error(StatusCode::BAD_REQUEST,"缺少指令序号");};
        let mut shared=a.telemetry.write().unwrap();
        if !shared.queue_mouth(body["session"].as_str().unwrap_or(""),sequence,degrees) {return error(StatusCode::CONFLICT,"嘴部会话/序号无效，或动作/反馈尚未就绪");}
        return Json(json!({"state":"queued","angleDeg":degrees})).into_response();
    }
    if action == "command" {
        let twist=match policy::drive_twist(&body) { Ok(v)=>v, Err(e)=>return error(StatusCode::BAD_REQUEST,&e.to_string()) };
        let Some(sequence)=body["sequence"].as_u64() else { return error(StatusCode::BAD_REQUEST,"缺少行走指令序号"); };
        let mut shared=a.telemetry.write().unwrap();
        if shared.control["state"]!="policy" || shared.control["kind"]!="xgoduck" {
            return error(StatusCode::CONFLICT,"请先启动XgoDuck零速度平衡，等待预备姿势完成");
        }
        if shared.control["feedbackHolding"]==true {
            let message = if shared.control["policyPhase"]=="imu-hold" {
                "IMU数据暂时过期，行走已暂停；恢复后请松开并重新按住方向"
            } else {
                "舵机反馈暂缺，行走已暂停；恢复后请松开并重新按住方向"
            };
            return error(StatusCode::CONFLICT,message);
        }
        if shared.control["activeSkill"]=="sitstand_sit" {
            return error(StatusCode::CONFLICT,"坐姿维持中，请先执行站起，再按住方向行走");
        }
        if shared.pending_skill.is_some() || shared.control["activeSkill"].as_str().is_some_and(|kind|kind!="xgoduck") {
            return error(StatusCode::CONFLICT,"动作执行中，方向指令暂不可用");
        }
        let Some(drive)=shared.drive.as_mut() else { return error(StatusCode::CONFLICT,"行走会话已结束"); };
        if !drive.update(body["session"].as_str().unwrap_or(""),sequence,twist) {
            return error(StatusCode::CONFLICT,"行走会话或指令序号已过期");
        }
        return Json(json!({"state":"commanded","twist":twist,"sequence":sequence})).into_response();
    }
    let (kind, speed) = match policy::request_profile(&body) {
        Ok(profile) => profile,
        Err(e) => return error(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    if action=="home" && matches!(kind,"xgoduck_getup"|"sitstand_stand") {return error(StatusCode::BAD_REQUEST,"起身从当前姿态启动，不使用站立HOME");}
    let permit = match a.gate.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return error(StatusCode::CONFLICT, "已有标定或运动任务正在执行"),
    };
    let boot = a.telemetry.read().unwrap().boot.clone();
    let cal = a.cal.lock().unwrap().read(&boot);
    if body["revision"] != cal["revision"] {
        return error(StatusCode::CONFLICT, "标定已更新，请刷新后重试");
    }
    let command = if action == "home" {
        match policy_home_command(kind,cal) {Ok(command)=>command,Err(e)=>return error(StatusCode::CONFLICT,&e.to_string())}
    } else {
        json!({"action":"policy","calibration":cal,"shadow":action=="shadow","kind":kind,"speed":speed})
    };
    let drive_session=uuid::Uuid::new_v4().to_string();
    if action=="start" {
        {let mut state=a.telemetry.write().unwrap();state.pending_skill=None;state.pending_mouth=None;state.mouth_degrees=0.;}
        a.telemetry.write().unwrap().drive = if policy::is_xgoduck(kind) { Some(crate::telemetry::DriveCommand {
            session:drive_session.clone(),sequence:0,twist:[0.;3],updated:Instant::now(),
        }) } else {None};
    }
    let rx = match owner.submit(command, Some(permit)) {
        Ok(rx) => rx,
        Err(e) => return error(StatusCode::CONFLICT, &e.to_string()),
    };
    if action == "start" {
        tokio::spawn(async move {
            let _ = rx.await;
        });
        return Json(
            json!({"state":"preflight","policy":policy::model_id(kind),"kind":kind,"driveSession":drive_session,"message":if kind=="xgoduck_getup" {"起身模型启动，从当前姿态使能后推理"} else {"模型启动，先过渡到所选模型姿势，再持续推理"}}),
        )
        .into_response();
    }
    match rx.await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error(StatusCode::CONFLICT, &e.to_string()),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "舵机服务已退出"),
    }
}
async fn stream(State(a): State<App>, h: HeaderMap, ws: WebSocketUpgrade) -> Response {
    if let Some(origin) = h.get("origin") {
        let host = h.get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
        if ![
            format!("http://{host}"),
            "http://localhost:5173".into(),
            "http://127.0.0.1:5173".into(),
        ]
        .iter()
        .any(|s| origin.to_str().ok() == Some(s))
        {
            return error(StatusCode::FORBIDDEN, "Origin not allowed");
        }
    }
    ws.on_upgrade(move |socket| client(socket, a))
        .into_response()
}
async fn send(ws: &mut WebSocket, v: Value) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            ws.send(Message::Text(v.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    )
}
async fn shutdown_signal(a: App) {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler");
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    if let Some(owner) = &a.owner {
        owner.cancel.store(true, Ordering::Release);
    }
    a.shutdown.store(true, Ordering::Release);
}
async fn api_errors(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let api = request.uri().path().starts_with("/api/");
    let response = next.run(request).await;
    if api && response.status() == StatusCode::NOT_FOUND {
        return error(StatusCode::NOT_FOUND, "Not Found");
    };
    response
}
async fn client(mut ws: WebSocket, a: App) {
    let v = match tokio::time::timeout(Duration::from_secs(5), ws.recv()).await {
        Ok(Some(Ok(Message::Text(t)))) => serde_json::from_str::<Value>(&t).unwrap_or(Value::Null),
        _ => return,
    };
    if v["type"] != "subscribe" || !v["topics"].is_object() {
        let _ = ws
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1008,
                reason: "Invalid subscription".into(),
            })))
            .await;
        return;
    }
    let mut rates = BTreeMap::new();
    for (t, r) in v["topics"].as_object().unwrap() {
        if !a.topics.contains_key(t) {
            continue;
        }
        if t == "logs" {
            rates.insert(t.clone(), None);
        } else if let Some(r) = r.as_f64().filter(|r| *r > 0.) {
            rates.insert(t.clone(), Some(r.min(a.topics[t].as_f64().unwrap())));
        }
    }
    let boot = a.telemetry.read().unwrap().boot.clone();
    if !send(&mut ws,json!({"type":"subscribed","protocolVersion":1,"bootId":boot,"requestId":v["requestId"],"topics":rates})).await{return}
    let mut sent: BTreeMap<String, u64> = BTreeMap::new();
    let mut deadlines: BTreeMap<String, Instant> = BTreeMap::new();
    let mut heartbeat = Instant::now();
    let mut timer = tokio::time::interval(Duration::from_millis(10));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {incoming=ws.recv()=>match incoming{None|Some(Err(_))|Some(Ok(Message::Close(_)))=>break,Some(Ok(Message::Ping(p)))=>{if ws.send(Message::Pong(p)).await.is_err(){break}},_=>{}},_=timer.tick()=>{if a.shutdown.load(Ordering::Acquire){break;}let now=Instant::now();if now>=heartbeat{if !send(&mut ws,json!({"type":"heartbeat","bootId":boot})).await{break}heartbeat=now+Duration::from_secs(2);}let mut messages=vec![];{let s=a.telemetry.read().unwrap();for(t,r)in &rates{let old=*sent.get(t).unwrap_or(&0);if t=="logs"{let batch:Vec<Value>=s.logs.iter().filter(|v|v["seq"].as_u64().unwrap()>old).take(100).map(|v|s.stamp(v)).collect();if let Some(last)=batch.last(){sent.insert(t.clone(),last["seq"].as_u64().unwrap());messages.push(json!({"type":"log_batch","gap":old>0&&batch[0]["seq"].as_u64().unwrap()>old+1,"items":batch}));}}else if deadlines.get(t).map_or(true,|d|now>=*d){if let Some(i)=s.latest.get(t){let n=i["seq"].as_u64().unwrap();if n>old{messages.push(s.stamp(i));sent.insert(t.clone(),n);deadlines.insert(t.clone(),deadlines.get(t).copied().unwrap_or(now).checked_add(Duration::from_secs_f64((1./r.unwrap()).min(3153600000.))).unwrap_or(now).max(now));}}}}}for m in messages{if !send(&mut ws,m).await{return}}}}
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--verify-policy-fixture") {
        let path = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("Fixture path required"))?;
        println!("{}", policy::verify_fixture(std::path::Path::new(path))?);
        return Ok(());
    }
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or(d.into());
    let source = env("MICRODUCK_SOURCE", "simulation");
    if !["simulation", "hardware"].contains(&source.as_str()) {
        anyhow::bail!("Invalid MICRODUCK_SOURCE")
    }
    let hardware = source == "hardware";
    if hardware && env("MICRODUCK_IMU_DRIVER", "ms901m") != "ms901m" {
        anyhow::bail!("BNO085 native migration is pending; keep the existing production service")
    };
    let port = env("MICRODUCK_SERVO_PORT", "");
    let tof_path = env(
        "MICRODUCK_TOF_SOCKET",
        if hardware { "/run/tofd/tof.sock" } else { "" },
    );
    let static_dir = env("MICRODUCK_STATIC_DIR", "../frontend/dist");
    let cal = Calibration::open(PathBuf::from(env(
        "MICRODUCK_CALIBRATION_FILE",
        "./state/calibration.json",
    )))?;
    let shared = Arc::new(RwLock::new(Telemetry::new(hardware)));
    let stop = Arc::new(AtomicBool::new(false));
    let owner = if !port.is_empty() {
        let ids = env("MICRODUCK_SERVO_IDS", "11,12,13,14,21,22,23,24")
            .split(',')
            .map(|v| v.trim().parse::<u8>())
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty()
            || ids.iter().any(|id| !IDS.contains(id))
            || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
        {
            anyhow::bail!("Servo IDs must be unique configured IDs")
        }
        Some(control::spawn(
            port.clone(),
            ids,
            shared.clone(),
            stop.clone(),
        ))
    } else {
        None
    };
    let a = App {
        telemetry: shared.clone(),
        cal: Arc::new(Mutex::new(cal)),
        topics: rates(hardware, !port.is_empty(), !tof_path.is_empty()),
        hardware,
        servo: !port.is_empty(),
        tof: !tof_path.is_empty(),
        owner,
        gate: Arc::new(tokio::sync::Semaphore::new(1)),
        plans: Arc::new(Mutex::new(None)),
        shutdown: stop.clone(),
    };
    if hardware {
        resources::spawn(shared.clone(),stop.clone());
        imu::spawn_reader(
            env("MICRODUCK_IMU_PORT", "/dev/ttyS9"),
            shared.clone(),
            stop.clone(),
        );
    }
    if !tof_path.is_empty() {
        tokio::spawn(tof::run(tof_path.clone(), shared.clone()));
    }
    let data = shared.clone();
    let producer_stop = stop.clone();
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_millis(20));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut system = Instant::now() - Duration::from_secs(1);
        loop {
            timer.tick().await;
            if producer_stop.load(Ordering::Acquire) {
                break;
            }
            let mut s = data.write().unwrap();
            if !hardware {
                s.simulate()
            }
            if system.elapsed() >= Duration::from_secs(1) {
                let d = json!({"resources":s.resources,"uptimeSeconds":s.started.elapsed().as_secs_f64(),"servoControl":if !port.is_empty(){s.control.clone()}else{Value::Null},"tof":if !tof_path.is_empty(){s.tof_health()}else{Value::Null},"imu":s.imu_health(),"scenario":if hardware{Value::Null}else{json!(s.scenario)}});
                s.sample("system", d, true, Instant::now());
                system = Instant::now();
            }
        }
    });
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://localhost:5173"),
            HeaderValue::from_static("http://127.0.0.1:5173"),
        ])
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE]);
    let shutdown_app = a.clone();
    let ble_app = a.clone();
    let owner = a.owner.clone();
    let router = Router::new()
        .route("/api/v1/info", get(info))
        .route("/api/v1/health", get(health))
        .route("/api/v1/system/{action}", post(power_action))
        .route("/api/v1/snapshot", get(snapshot))
        .route("/api/v1/calibration", get(get_cal).post(set_cal))
        .route("/api/v1/calibration/poses", get(poses))
        .route("/api/v1/servos/limits", get(limits))
        .route("/api/v1/logs", get(logs))
        .route("/api/v1/policy", get(policy_info))
        .route("/api/v1/policy/{action}", post(policy_action))
        .route("/api/v1/servos/{action}", post(servo_action))
        .route("/api/v1/calibration/{action}", post(guided_action))
        .route("/api/v1/debug/scenario", post(scenario))
        .route("/api/v1/stream", get(stream))
        .fallback_service(ServeDir::new(static_dir).append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(16384))
        .layer(cors)
        .layer(axum::middleware::from_fn(api_errors))
        .with_state(a);
    if env("MICRODUCK_BLE_ENABLED", "0") == "1" {
        ducklink::spawn(ble_app, router.clone(), stop.clone());
    }
    let listener = tokio::net::TcpListener::bind(env("MICRODUCK_BIND", "127.0.0.1:8878")).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(shutdown_app))
        .await?;
    stop.store(true, Ordering::Relaxed);
    if let Some(owner) = owner {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !owner.done.load(Ordering::Acquire) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    Ok(())
}
