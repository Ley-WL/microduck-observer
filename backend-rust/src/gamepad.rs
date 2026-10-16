//! Local Linux evdev controller. No network bridge and no second UART owner.
use crate::{ducklink_motion::Motion, App};
use anyhow::{ensure, Context, Result};
use axum::{body::Bytes, extract::State, http::{HeaderMap, StatusCode}, response::{IntoResponse, Response}, routing::{get, post}, Json, Router};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    time::Duration,
    sync::{atomic::Ordering, Arc},
};

const A: usize = 304;
const B: usize = 305;
const X: usize = 307;
const Y: usize = 308;
const LB: usize = 310;
const RB: usize = 311;
const LT: usize = 312;
const RT: usize = 313;
const BACK: usize = 314;
const START: usize = 315;
const BUTTONS: [usize; 10] = [A, B, X, Y, LB, RB, LT, RT, BACK, START];

static MODE_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn mode_path() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE")
        .unwrap_or("./state/calibration.json".into())).with_file_name("gamepad-mode.json")
}
fn read_mode(path: &std::path::Path, default: bool) -> Result<bool> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)?["debug"].as_bool()
            .context("saved gamepad mode must contain boolean debug"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(default),
        Err(e) => Err(e.into()),
    }
}
pub fn initial_debug() -> bool {
    let default = std::env::var("MICRODUCK_GAMEPAD_DIAGNOSTIC_ONLY").unwrap_or("1".into()) == "1";
    read_mode(&mode_path(), default).unwrap_or_else(|e| {
        eprintln!("无法读取已保存手柄模式，使用只读诊断：{e}");
        true
    })
}
fn persist_mode(path: &std::path::Path, enabled: bool) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().context("gamepad mode parent missing")?;
    std::fs::create_dir_all(parent)?;
    let tmp = path.with_extension("tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(&serde_json::to_vec(&json!({"debug":enabled}))?)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn mode_router() -> Router<App> {
    Router::new().route("/api/v1/gamepad/mode", get(get_mode)).route("/api/v1/gamepad/mode", post(set_mode))
}
async fn get_mode(State(app): State<App>) -> Json<Value> {
    Json(json!({"debug":app.gamepad_debug.load(Ordering::Acquire)}))
}
async fn set_mode(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Response {
    let value = match crate::check_action(&headers, &body, 64) { Ok(v) => v, Err(r) => return r };
    let Some(enabled) = value["debug"].as_bool() else {
        return crate::error(StatusCode::UNPROCESSABLE_ENTITY, "debug must be boolean");
    };
    let _guard = MODE_WRITE.lock().unwrap();
    if let Err(e) = persist_mode(&mode_path(), enabled) {
        return crate::error(StatusCode::INTERNAL_SERVER_ERROR, &format!("手柄模式保存失败：{e}"));
    }
    app.gamepad_debug.store(enabled, Ordering::Release);
    Json(json!({"debug":enabled,"message":if enabled{"调试模式已开启：输入只显示在网页，不发送实体控制指令"}else{"调试模式已关闭：实体手柄控制已允许，请先确认机器人周围安全"}})).into_response()
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct AbsInfo {
    value: i32,
    min: i32,
    max: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

// Linux generic ioctl ABI, shared by the supported aarch64 and x86_64 targets.
fn read_ioctl<T>(file: &File, number: u32, out: &mut T) -> Result<()> {
    let request =
        (2u32 << 30) | ((std::mem::size_of::<T>() as u32) << 16) | (b'E' as u32) << 8 | number;
    let result = unsafe { libc::ioctl(file.as_raw_fd(), request as libc::c_ulong, out as *mut T) };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn axis(raw: AbsInfo) -> Result<f64> {
    ensure!(
        raw.max > raw.min && raw.value >= raw.min && raw.value <= raw.max,
        "invalid gamepad axis range"
    );
    let half = (f64::from(raw.max) - f64::from(raw.min)) / 2.;
    let value = (f64::from(raw.value) - (f64::from(raw.max) + f64::from(raw.min)) / 2.) / half;
    let dead = (f64::from(raw.flat).max(0.) / half).clamp(0.15, 0.95);
    Ok(if value.abs() <= dead {
        0.
    } else {
        value.signum() * (value.abs() - dead) / (1. - dead)
    })
}

fn trigger(raw: AbsInfo) -> Result<bool> {
    ensure!(raw.max > raw.min && (raw.min..=raw.max).contains(&raw.value),
        "invalid gamepad trigger range");
    let span = f64::from(raw.max) - f64::from(raw.min);
    Ok(f64::from(raw.value) - f64::from(raw.min) > (span * 0.25).max(f64::from(raw.flat)))
}

#[derive(Default, Clone)]
struct Sample {
    axes: [f64; 4],
    keys: [bool; 10],
    dpad: [bool; 4],
}
impl Sample {
    fn down(&self, code: usize) -> bool {
        self.keys[BUTTONS.iter().position(|v| *v == code).unwrap()]
    }
    fn centered(&self) -> bool {
        self.axes.iter().all(|v| *v == 0.)
    }
    fn neutral(&self) -> bool {
        self.centered() && self.keys.iter().chain(self.dpad.iter()).all(|v| !v)
    }
}

struct Device {
    file: File,
    axes: [u32; 4],
    hats: [bool; 2],
    triggers: [bool; 2], // ABS_BRAKE (LT), ABS_GAS (RT); never reuse stick axes.
}
impl Device {
    fn open(path: &str, expected_name: &str, axes: [u32; 4]) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .open(path)
            .context("open gamepad evdev")?;
        let mut name = [0u8; 256];
        read_ioctl(&file, 0x06, &mut name)?;
        let name = String::from_utf8_lossy(
            &name[..name.iter().position(|b| *b == 0).unwrap_or(name.len())],
        );
        ensure!(name == expected_name, "gamepad name mismatch: {name}");
        let mut key_bits = [0u8; 96];
        read_ioctl(&file, 0x21, &mut key_bits)?; // EVIOCGBIT(EV_KEY)
        let mut abs_bits = [0u8; 8];
        read_ioctl(&file, 0x23, &mut abs_bits)?; // EVIOCGBIT(EV_ABS)
        let triggers = [10usize, 9].map(|code| !axes.contains(&(code as u32)) && abs_bits[code / 8] & (1 << (code % 8)) != 0);
        ensure!(
            BUTTONS
                .iter()
                .all(|k| key_bits[k / 8] & (1 << (k % 8)) != 0
                    || (*k == LT && triggers[0]) || (*k == RT && triggers[1])),
            "gamepad lacks Xbox-layout buttons"
        );
        let hats = [16usize, 17].map(|code| abs_bits[code / 8] & (1 << (code % 8)) != 0);
        let device = Self { file, axes, hats, triggers };
        device.sample()?;
        Ok(device)
    }
    fn sample(&self) -> Result<Sample> {
        // Read current kernel state instead of replaying queued/old input events.
        // ioctl fails with ENODEV when evdev is disconnected, even if axes were held.
        let mut keys = [0u8; 96];
        read_ioctl(&self.file, 0x18, &mut keys)?; // EVIOCGKEY
        let mut sample = Sample::default();
        for (i, code) in BUTTONS.iter().enumerate() {
            sample.keys[i] = keys[code / 8] & (1 << (code % 8)) != 0;
        }
        for (i, code) in [10u32, 9].iter().enumerate() {
            if self.triggers[i] {
                let mut abs = AbsInfo::default();
                read_ioctl(&self.file, 0x40 + code, &mut abs)?;
                sample.keys[6 + i] |= trigger(abs)?;
            }
        }
        for (i, code) in self.axes.iter().enumerate() {
            let mut abs = AbsInfo::default();
            read_ioctl(&self.file, 0x40 + code, &mut abs)?;
            sample.axes[i] = axis(abs)?;
        }
        let mut hat = [0; 2];
        for (i, supported) in self.hats.iter().enumerate() {
            if *supported {
                let mut abs = AbsInfo::default();
                read_ioctl(&self.file, 0x40 + 16 + i as u32, &mut abs)?;
                hat[i] = abs.value;
            }
        }
        // Xbox hats and Linux BTN_DPAD variants are both display-only.
        sample.dpad = [hat[1] < 0, hat[0] < 0, hat[0] > 0, hat[1] > 0];
        for (i, code) in [544usize, 546, 547, 545].iter().enumerate() {
            sample.dpad[i] |= keys[code / 8] & (1 << (code % 8)) != 0;
        }
        Ok(sample)
    }
}

#[derive(Default)]
struct Input {
    previous: Sample,
    neutral_seen: bool,
    drive_armed: bool,
    start_since: Option<std::time::Instant>,
    start_triggered: bool,
    y_hold_ticks: u8,
    y_hold_triggered: bool,
}
impl Input {
    fn update(&mut self, sample: &Sample, ready: bool, diagnostic_only: bool) -> Option<&'static str> {
        self.update_at(sample, ready, diagnostic_only, std::time::Instant::now())
    }
    fn update_at(&mut self, sample: &Sample, ready: bool, diagnostic_only: bool, now: std::time::Instant) -> Option<&'static str> {
        if diagnostic_only {
            self.start_since = None;
            self.start_triggered = false;
            self.previous = sample.clone();
            self.neutral_seen = sample.neutral();
            self.drive_armed = false;
            self.y_hold_ticks = 0;
            self.y_hold_triggered = false;
            return None;
        }
        if !sample.down(START) {
            self.start_since = None;
            self.start_triggered = false;
        } else if self.neutral_seen && !self.previous.down(START) {
            self.start_since = Some(now);
        }
        let long_start = !self.start_triggered && self.start_since
            .is_some_and(|since| now.duration_since(since) >= Duration::from_secs(2));
        if long_start { self.start_triggered = true; }
        let edge = |key| sample.down(key) && !self.previous.down(key);
        if sample.down(Y) && sample.centered() {
            self.y_hold_ticks = self.y_hold_ticks.saturating_add(1);
        } else {
            self.y_hold_ticks = 0;
            self.y_hold_triggered = false;
        }
        let long_y = self.y_hold_ticks >= 14 && !self.y_hold_triggered;
        if long_y {
            self.y_hold_triggered = true;
        }
        let action = if edge(BACK) {
            Some("estop")
        } else if edge(B) {
            Some("release")
        } else if !self.neutral_seen {
            None
        } else if long_start {
            Some("poweroff")
        } else if long_y {
            Some("getup")
        } else if sample.centered() && edge(A) {
            Some("stand")
        } else if edge(X) {
            Some("sit")
        } else if (sample.down(RB) && !self.previous.down(RB))
            || (sample.down(RT) && !self.previous.down(RT))
        {
            Some("quack")
        } else if ready && sample.centered() && sample.dpad[0] && !self.previous.dpad[0] {
            Some("dance")
        } else if ready && sample.centered() && sample.dpad[1] && !self.previous.dpad[1] {
            Some("flamingo")
        } else if ready && sample.centered() && sample.dpad[2] && !self.previous.dpad[2] {
            Some("kick_right")
        } else if ready && sample.centered() && edge(LB)
            && sample.keys.iter().zip(BUTTONS).all(|(down, key)| key == LB || !down)
            && !sample.dpad.iter().any(|down| *down)
        {
            Some("pick")
        } else {
            None
        };
        if sample.neutral() {
            self.neutral_seen = true;
        }
        if !ready || action.is_some_and(|action|action!="quack") {
            self.drive_armed = false;
        } else if sample.neutral() {
            self.drive_armed = true;
        }
        self.previous = sample.clone();
        action
    }
    fn drive(&self, sample: &Sample, seq: u64) -> serde_json::Value {
        let enabled = self.drive_armed;
        let [lx, ly, rx, ry] = if enabled { sample.axes } else { [0.; 4] };
        let boosted = enabled && sample.down(LT);
        // Encode axes independently: Motion multiplies both by speed, then vx by 0.4.
        // A gentle center retains fine control; full stick reaches 0.6 rad/s yaw.
        let forward = -ly * if boosted { 1.0 } else { 0.5 };
        let turn = -0.6 * lx * (0.5 + 0.5 * lx.abs());
        json!({"seq":seq,"action":"drive","forward":forward,"turn":turn,"headYaw":-rx,"headPitch":-ry,"speed":1.0,"compact":true})
    }
}

// Cancel any owner first, then hold the shared motion gate across a second
// confirmed unload and power scheduling so no client can re-enable the servos.
fn unload_confirmed(value: &Value) -> bool {
    value["state"] == "disabled" && value["unconfirmedIds"] == json!([])
}
async fn shutdown_board(app: &App, motion: &mut Motion) -> Result<()> {
    ensure!(app.hardware && crate::power::enabled(), "当前设备未启用电源控制");
    motion.request("servos/disable", Some(json!({}))).await?;
    motion.close().await;
    let permit = tokio::time::timeout(Duration::from_secs(3), app.gate.clone().acquire_owned()).await??;
    let unloaded = motion.request("servos/disable", Some(json!({}))).await?;
    ensure!(unload_confirmed(&unloaded),
        "卸力未全部确认，取消关机：{unloaded}");
    let boot = app.telemetry.read().unwrap().boot.clone();
    // A timeout can mean the helper already scheduled shutdown. Keep motion
    // locked in that case as well; restarting the service restores the gate.
    let result = motion.request("system/poweroff", Some(json!({"confirm":true,"bootId":boot}))).await;
    tokio::spawn(async move {
        let _permit = permit;
        std::future::pending::<()>().await;
    });
    result.context("电源请求已发出；运动已锁定，若主板未关机请检查电源服务，重启Observer服务可解除锁定")?;
    Ok(())
}

fn report(app: &App, message: &str) {
    app.telemetry
        .write()
        .unwrap()
        .log("INFO", "gamepad", message);
}

fn input_data(name: &str, sample: Option<&Sample>) -> serde_json::Value {
    let empty = Sample::default();
    let current = sample.unwrap_or(&empty);
    let mut buttons = serde_json::Map::new();
    for (key, down) in ["a", "b", "x", "y", "lb", "rb", "lt", "rt", "back", "start"].iter().zip(current.keys) {
        buttons.insert((*key).into(), json!(down));
    }
    for (key, down) in ["up", "left", "right", "down"].iter().zip(current.dpad) {
        buttons.insert((*key).into(), json!(down));
    }
    json!({"connected":sample.is_some(),"device":name,"buttons":buttons,"axes":current.axes})
}
fn publish_input(app: &App, name: &str, sample: Option<&Sample>) {
    app.telemetry.write().unwrap().sample("gamepad", input_data(name, sample), true, std::time::Instant::now());
}
// Sampling is independent of motion HTTP requests: release feedback remains live
// while a stand/getup request is waiting on the robot's control service.
struct InputMonitor { task: tokio::task::JoinHandle<()>, app: App, name: String }
impl Drop for InputMonitor {
    fn drop(&mut self) {
        self.task.abort();
        publish_input(&self.app, &self.name, None);
    }
}
fn monitor(app: App, name: String, device: Arc<Device>) -> Result<(InputMonitor, tokio::sync::watch::Receiver<std::result::Result<(std::time::Instant, Sample), String>>)> {
    let first = device.sample()?;
    publish_input(&app, &name, Some(&first));
    let (tx, rx) = tokio::sync::watch::channel(Ok((std::time::Instant::now(), first)));
    let reader_app = app.clone();
    let reader_name = name.clone();
    let task = tokio::spawn(async move {
        let mut clock = tokio::time::interval(Duration::from_millis(40));
        clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            clock.tick().await;
            match device.sample() {
                Ok(sample) => {
                    publish_input(&reader_app, &reader_name, Some(&sample));
                    if tx.send(Ok((std::time::Instant::now(), sample))).is_err() { break; }
                }
                Err(error) => {
                    publish_input(&reader_app, &reader_name, None);
                    let _ = tx.send(Err(error.to_string()));
                    break;
                }
            }
        }
    });
    Ok((InputMonitor { task, app, name }, rx))
}

pub fn spawn(app: App, router: Router) {
    publish_input(&app, "", None);
    let Ok(path) = std::env::var("MICRODUCK_GAMEPAD_DEVICE") else {
        return;
    };
    let name = std::env::var("MICRODUCK_GAMEPAD_NAME").unwrap_or_default();
    if !app.hardware || name.trim().is_empty() || !path.starts_with("/dev/input/") {
        report(
            &app,
            "手柄未启用：需实机模式、/dev/input/路径及精确设备名称",
        );
        return;
    }
    let axes = match std::env::var("MICRODUCK_GAMEPAD_RIGHT_AXES")
        .as_deref()
        .unwrap_or("3,4")
    {
        "3,4" => [0, 1, 3, 4],
        "2,5" => [0, 1, 2, 5],
        _ => {
            report(&app, "手柄右摇杆轴配置无效，仅支持3,4或2,5");
            return;
        }
    };
    let debug = app.gamepad_debug.clone();
    tokio::spawn(async move {
        let mut motion = Motion::local(app.clone(), router);
        let mut seq = 0u64;
        let mut last_error = String::new();
        while !app.shutdown.load(std::sync::atomic::Ordering::Acquire) {
            let result = async {
                let device = Arc::new(Device::open(&path, &name, axes)?);
                let (_monitor, samples) = monitor(app.clone(), name.clone(), device)?;
                last_error.clear();
                report(&app, if debug.load(Ordering::Acquire) {
                    "手柄只读诊断已连接；网页显示按键和摇杆输入，实体动作已屏蔽"
                } else {
                    "手柄已连接；长按 Y 起身，起身就绪后回中即可直接用摇杆控制"
                });
                let mut input = Input::default();
                let mut clock = tokio::time::interval(Duration::from_millis(40));
                clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut last_tick = std::time::Instant::now();
                loop {
                    clock.tick().await;
                    if app.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                    // A stalled task must never resume old held-stick input.
                    if last_tick.elapsed() > Duration::from_millis(250) {
                        motion.close().await;
                        input = Input::default();
                        report(&app, "手柄采样中断，已释放控制，请回中后重新启动");
                    }
                    last_tick = std::time::Instant::now();
                    let (sampled_at, sample) = samples.borrow().clone().map_err(anyhow::Error::msg)?;
                    ensure!(sampled_at.elapsed() < Duration::from_millis(250), "gamepad sample expired");
                    let diagnostic_only = debug.load(Ordering::Acquire);
                    if diagnostic_only && motion.leased {
                        motion.close().await;
                        input = Input::default();
                        report(&app, "已启用只读诊断，手柄控制权已安全释放");
                    }
                    // Require a fresh deliberate input after neutral; merely connecting
                    // or holding a key through reconnect must not steal control.
                    if !diagnostic_only && input.neutral_seen && !sample.neutral() {
                        if motion.take_over_policy() {input.drive_armed=true;}
                    }
                    let status=motion.status();
                    let ready = motion.leased && (status["headReady"] == true || status["flamingoReady"] == true);
                    if let Some(action) = input.update(&sample, ready, diagnostic_only) {
                        if action == "estop" {
                            // Emergency stop is global, including when a phone owns the lease.
                            motion.request("servos/disable", Some(json!({}))).await?;
                            motion.close().await;
                            input = Input::default();
                            report(&app, "手柄急停卸力");
                        } else if action == "poweroff" {
                            report(&app, "Start 长按2秒：正在卸力并确认关机");
                            let result = shutdown_board(&app, &mut motion).await;
                            input = Input::default();
                            match result {
                                Ok(()) => report(&app, "15颗舵机卸力已确认，主板即将关机"),
                                Err(e) => report(&app, &format!("关机未完成：{e}")),
                            }
                        } else if action == "release" {
                            motion.close().await;
                            input = Input::default();
                            report(&app, "手柄已停止保持并释放控制");
                        } else {
                            if !motion.leased {
                                if !matches!(action, "stand" | "getup" | "quack") {
                                    continue;
                                }
                                if let Err(e) = motion.open() {
                                    report(&app, &e.to_string());
                                    continue;
                                }
                            }
                            if action == "quack" && ready {
                                seq += 1;
                                motion.command(input.drive(&sample, seq)).await?;
                            }
                            seq += 1;
                            let status = motion
                                .command(json!({"seq":seq,"action":action,"compact":true}))
                                .await?;
                            if let Some(error) =
                                status["motionError"].as_str().filter(|s| !s.is_empty())
                            {
                                report(&app, error);
                            }
                        }
                    } else if motion.leased {
                        seq += 1;
                        motion.command(input.drive(&sample, seq)).await?;
                    }
                    motion.tick().await;
                }
                Ok::<(), anyhow::Error>(())
            }
            .await;
            motion.close().await;
            if let Err(error) = result {
                let error = error.to_string();
                if error != last_error {
                    report(&app, &format!("手柄不可用，控制已释放：{error}"));
                    last_error = error;
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn poweroff_requires_explicit_complete_unload_confirmation() {
        assert!(unload_confirmed(&json!({"state":"disabled","unconfirmedIds":[]})));
        for value in [json!({"state":"failed","unconfirmedIds":[31]}),
            json!({"state":"disabled","unconfirmedIds":[31]}),
            json!({"state":"disabled"}), json!({"state":"policy","unconfirmedIds":[]})] {
            assert!(!unload_confirmed(&value));
        }
    }
    #[test]
    fn start_requires_fresh_two_second_hold_and_rearms_on_release() {
        let now = std::time::Instant::now();
        let neutral = Sample::default();
        let mut held = neutral.clone(); held.keys[9] = true;
        let mut input = Input::default();
        assert_eq!(input.update_at(&held, false, false, now), None);
        assert_eq!(input.update_at(&held, false, false, now+Duration::from_secs(3)), None);
        input.update_at(&neutral, false, false, now);
        assert_eq!(input.update_at(&held, false, false, now), None);
        assert_eq!(input.update_at(&held, false, false, now+Duration::from_millis(1999)), None);
        assert_eq!(input.update_at(&held, false, false, now+Duration::from_secs(2)), Some("poweroff"));
        assert_eq!(input.update_at(&held, false, false, now+Duration::from_secs(3)), None);
        input.update_at(&neutral, false, false, now+Duration::from_secs(4));
        input.update_at(&held, false, false, now+Duration::from_secs(4));
        assert_eq!(input.update_at(&held, false, false, now+Duration::from_secs(6)), Some("poweroff"));
        input.update_at(&held, false, true, now+Duration::from_secs(7));
        assert_eq!(input.update_at(&held, false, false, now+Duration::from_secs(10)), None);
    }
    #[test]
    fn saved_mode_survives_restart_and_overrides_environment_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gamepad-mode.json");
        assert!(read_mode(&path, true).unwrap());
        persist_mode(&path, false).unwrap();
        assert!(!read_mode(&path, true).unwrap());
        persist_mode(&path, true).unwrap();
        assert!(read_mode(&path, false).unwrap());
        std::fs::write(&path, b"{\"debug\":\"false\"}").unwrap();
        assert!(read_mode(&path, false).is_err());
    }
    fn sample(keys: &[usize], axes: [f64; 4]) -> Sample {
        Sample {
            axes,
            keys: BUTTONS.map(|key| keys.contains(&key)),
            dpad: [false; 4],
        }
    }
    #[test]
    fn analog_lt_capture_boosts_and_releases_without_digital_key() {
        let mut input = Input::default();
        input.drive_armed = true;
        for (value, expected) in [(0, 0.2), (63, 0.2), (255, 0.2), (1023, 0.4), (0, 0.2)] {
            let mut raw = sample(&[], [0., -1., 0., 0.]);
            raw.keys[6] = trigger(AbsInfo {value, min:0, max:1023, flat:63, ..Default::default()}).unwrap();
            let command = input.drive(&raw, 1);
            assert!((command["forward"].as_f64().unwrap()*0.4-expected).abs()<1e-9);
            assert_eq!(input_data("test-pad", Some(&raw))["buttons"]["lt"], expected == 0.4);
        }
        assert!(trigger(AbsInfo::default()).is_err());
        assert!(trigger(AbsInfo {value:1024, min:0, max:1023, ..Default::default()}).is_err());
    }
    #[test]
    fn input_feedback_preserves_raw_keys_and_clears_on_disconnect() {
        let mut raw = sample(&[A, LB, START], [0.5, -1., 0., 1.]);
        raw.dpad = [true, false, true, false];
        let data = input_data("test-pad", Some(&raw));
        assert_eq!(data["buttons"]["a"], true);
        assert_eq!(data["buttons"]["start"], true);
        assert_eq!(data["buttons"]["up"], true);
        assert_eq!(data["axes"][1], -1.);
        let offline = input_data("test-pad", None);
        assert_eq!(offline["connected"], false);
        assert!(offline["buttons"].as_object().unwrap().values().all(|v| v == false));
        assert_eq!(offline["axes"], json!([0., 0., 0., 0.]));
        assert!(!raw.neutral());
    }
    #[test]
    fn normalize_signed_unsigned_deadzone_and_invalid_ranges() {
        for (min, max) in [(-32768, 32767), (0, 255)] {
            for (value, expected) in [(min, -1.), ((min + max) / 2, 0.), (max, 1.)] {
                assert_eq!(
                    axis(AbsInfo {
                        min,
                        max,
                        value,
                        ..Default::default()
                    })
                    .unwrap(),
                    expected
                );
            }
        }
        assert!(axis(AbsInfo::default()).is_err());
        assert!(axis(AbsInfo {
            min: 0,
            max: 255,
            value: 256,
            ..Default::default()
        })
        .is_err());
    }
    #[test]
    fn plug_in_held_buttons_never_starts_and_requires_new_edge() {
        let mut input = Input::default();
        let start = sample(&[LB, A], [0.; 4]);
        assert_eq!(input.update(&start, false, false), None);
        input.update(&Sample::default(), false, false);
        assert_eq!(input.update(&start, false, false), Some("stand"));
        assert_eq!(input.update(&start, false, false), None);
    }
    #[test]
    fn start_is_reserved_and_a_stands_without_lb_only_when_centered() {
        let mut input = Input::default();
        input.update(&Sample::default(), false, false);
        assert_eq!(input.update(&sample(&[START], [0.; 4]), false, false), None);
        input.update(&Sample::default(), false, false);
        assert_eq!(input.update(&sample(&[LB, START], [0.; 4]), false, false), None);
        input.update(&Sample::default(), false, false);
        assert_eq!(input.update(&sample(&[A], [1.; 4]), false, false), None);
        input.update(&Sample::default(), false, false);
        assert_eq!(input.update(&sample(&[A], [0.; 4]), false, false), Some("stand"));
    }
    #[test]
    fn lb_pick_requires_ready_centered_fresh_single_press() {
        let mut input = Input::default();
        let lb = sample(&[LB], [0.; 4]);
        assert_eq!(input.update(&lb, true, false), None);
        input.update(&Sample::default(), true, false);
        assert_eq!(input.update(&lb, false, false), None);
        assert_eq!(input.update(&lb, true, false), None);
        input.update(&Sample::default(), true, false);
        assert_eq!(input.update(&sample(&[LB], [1.; 4]), true, false), None);
        input.update(&Sample::default(), true, false);
        assert_eq!(input.update(&lb, true, false), Some("pick"));
        assert_eq!(input.update(&lb, true, false), None);
        input.update(&Sample::default(), true, false);
        assert_eq!(input.update(&lb, true, true), None);
    }
    #[test]
    fn lb_combinations_do_not_trigger_pick() {
        for other in [A, B, X, Y, RB, RT, LT, START, BACK] {
            let mut input = Input::default();
            input.update(&Sample::default(), true, false);
            assert_ne!(input.update(&sample(&[LB, other], [0.; 4]), true, false), Some("pick"));
        }
    }
    #[test]
    fn drive_requires_ready_then_neutral_but_not_lb() {
        let mut input = Input::default();
        let held = sample(&[], [0., -1., 1., -1.]);
        input.update(&Sample::default(), false, false);
        input.update(&held, true, false);
        assert_eq!(input.drive(&held, 1)["forward"], 0.);
        input.update(&Sample::default(), true, false);
        input.update(&held, true, false);
        let drive = input.drive(&held, 2);
        assert_eq!(drive["forward"], 0.5);
        assert_eq!(drive["headYaw"], -1.);
        assert_eq!(drive["speed"], 1.0);
        let turbo=input.drive(&sample(&[LT], [1.,-1.,1.,-1.]),3);
        assert_eq!(turbo["speed"], 1.0);
        assert_eq!(turbo["turn"], -0.6);
        for (buttons, expected) in [(vec![], 0.2), (vec![LT], 0.4)] {
            for axis in [-1., 0., 1.] {
                let command = input.drive(&sample(&buttons, [1., axis, 0., 0.]), 4);
                let speed = command["speed"].as_f64().unwrap();
                assert!((command["forward"].as_f64().unwrap()*speed*0.4 + axis*expected).abs()<1e-9);
                assert!((command["turn"].as_f64().unwrap()*speed + 0.6).abs()<1e-9);
            }
        }
        input.update(&Sample::default(), true, false);
        assert_eq!(input.drive(&sample(&[], held.axes), 3)["forward"], 0.5);
        input.update(&held, false, false);
        input.update(&held, true, false);
        assert_eq!(input.drive(&held, 4)["forward"], 0.);
    }
    #[test]
    fn long_y_starts_getup_without_lb_and_neutral_arms_direct_stick_control() {
        let mut input = Input::default();
        let y = sample(&[Y], [0.; 4]);
        input.update(&Sample::default(), false, false);
        for _ in 0..13 {
            assert_eq!(input.update(&y, false, false), None);
        }
        assert_eq!(input.update(&y, false, false), Some("getup"));
        assert_eq!(input.update(&y, true, false), None);
        input.update(&Sample::default(), true, false);
        let stick = sample(&[], [0., -1., 0., 0.]);
        input.update(&stick, true, false);
        assert_eq!(input.drive(&stick, 1)["forward"], 0.5);
    }
    #[test]
    fn yaw_curve_is_symmetric_monotonic_and_independent_of_boost() {
        let mut input=Input::default();
        input.update(&Sample::default(),true,false);
        let mut previous=1.;
        for n in -100..=100 {
            let axis=n as f64/100.;
            let command=input.drive(&sample(&[],[axis,0.,0.,0.]),1);
            let boosted=input.drive(&sample(&[LT],[axis,0.,0.,0.]),2);
            let yaw=command["turn"].as_f64().unwrap();
            assert!(yaw<=previous && yaw.abs()<=0.6);
            assert_eq!(command["turn"],boosted["turn"]);
            assert_eq!(command["forward"],0.);
            let opposite=input.drive(&sample(&[],[-axis,0.,0.,0.]),3);
            assert!((yaw+opposite["turn"].as_f64().unwrap()).abs()<1e-9);
            previous=yaw;
        }
        assert_eq!(input.drive(&Sample::default(),4)["turn"],0.);
        assert!((input.drive(&sample(&[],[0.5,0.,0.,0.]),5)["turn"].as_f64().unwrap()+0.225).abs()<1e-9);
        input.drive_armed=false;
        assert_eq!(input.drive(&sample(&[LT],[1.,-1.,0.,0.]),6)["turn"],0.);
    }
    #[test]
    fn emergency_and_release_work_before_neutral_and_override_skills() {
        let mut input = Input::default();
        assert_eq!(
            input.update(&sample(&[BACK, LB, START], [1.; 4]), false, false),
            Some("estop")
        );
        assert_eq!(
            Input::default().update(&sample(&[B], [1.; 4]), false, false),
            Some("release")
        );
    }
    #[test]
    fn quack_keeps_both_sticks_armed_until_release() {
        for key in [RB,RT] {
            let mut input=Input::default();
            input.update(&Sample::default(),true,false);
            let sticks=sample(&[],[0.4,-0.8,0.6,-0.5]);
            input.update(&sticks,true,false);
            let before=input.drive(&sticks,1);
            let pressed=sample(&[key],sticks.axes);
            assert_eq!(input.update(&pressed,true,false),Some("quack"));
            let during=input.drive(&pressed,2);
            for field in ["forward","turn","headYaw","headPitch"] {assert_eq!(before[field],during[field]);assert_ne!(during[field],0.);}
            assert_eq!(input.update(&pressed,true,false),None);
            input.update(&sample(&[B],sticks.axes),true,false);
            assert_eq!(input.drive(&sticks,3)["forward"],0.);
        }
    }
    #[test]
    fn rt_and_rb_share_one_quack_edge_and_triggers_are_reported() {
        let mut input = Input::default();
        input.update(&Sample::default(), false, false);
        let rt = sample(&[RT], [0.; 4]);
        assert_eq!(input.update(&rt, false, false), Some("quack"));
        assert_eq!(input.update(&rt, false, false), None);
        input.update(&Sample::default(), false, false);
        assert_eq!(input.update(&sample(&[RB], [0.; 4]), false, false), Some("quack"));
        let feedback=input_data("test-pad",Some(&rt));
        assert_eq!(feedback["buttons"]["rt"],true);
        assert_eq!(feedback["buttons"]["lt"],false);
    }
    #[test]
    fn sit_and_kicks_do_not_require_lb() {
        let mut input = Input::default();
        input.update(&Sample::default(), true, false);
        assert_eq!(input.update(&sample(&[X], [0.; 4]), true, false), Some("sit"));
        input.update(&Sample::default(), true, false);
        let mut left = Sample::default();
        left.dpad[1] = true;
        assert_eq!(input.update(&left, true, false), Some("flamingo"));
        input.update(&Sample::default(), true, false);
        let mut right = Sample::default();
        right.dpad[2] = true;
        assert_eq!(input.update(&right, true, false), Some("kick_right"));
        input.update(&Sample::default(), true, false);
        let mut up=Sample::default();up.dpad[0]=true;
        assert_eq!(input.update(&up,true,false),Some("dance"));
        assert_eq!(input.update(&up,true,false),None);
    }
    #[test]
    fn diagnostic_only_never_dispatches_even_estop_or_start_actions() {
        let mut input = Input::default();
        input.update(&Sample::default(), false, true);
        for keys in [&[BACK][..], &[B][..], &[LB, A][..], &[LB, X][..], &[LB, Y][..]] {
            assert_eq!(input.update(&sample(keys, [0.; 4]), true, true), None);
            input.update(&Sample::default(), true, true);
        }
    }
}
