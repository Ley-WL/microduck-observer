//! Local Linux evdev controller. No network bridge and no second UART owner.
use crate::{ducklink_motion::Motion, App};
use anyhow::{ensure, Context, Result};
use axum::Router;
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    time::Duration,
    sync::Arc,
};

const A: usize = 304;
const B: usize = 305;
const X: usize = 307;
const Y: usize = 308;
const LB: usize = 310;
const RB: usize = 311;
const BACK: usize = 314;
const START: usize = 315;
const BUTTONS: [usize; 8] = [A, B, X, Y, LB, RB, BACK, START];

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

#[derive(Default, Clone)]
struct Sample {
    axes: [f64; 4],
    keys: [bool; 8],
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
        ensure!(
            BUTTONS
                .iter()
                .all(|k| key_bits[k / 8] & (1 << (k % 8)) != 0),
            "gamepad lacks Xbox-layout buttons"
        );
        let mut abs_bits = [0u8; 8];
        read_ioctl(&file, 0x23, &mut abs_bits)?; // EVIOCGBIT(EV_ABS)
        let hats = [16usize, 17].map(|code| abs_bits[code / 8] & (1 << (code % 8)) != 0);
        let device = Self { file, axes, hats };
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
}
impl Input {
    fn update(&mut self, sample: &Sample, ready: bool) -> Option<&'static str> {
        let edge = |key| sample.down(key) && !self.previous.down(key);
        let action = if edge(BACK) {
            Some("estop")
        } else if edge(B) {
            Some("release")
        } else if !self.neutral_seen {
            None
        } else if sample.down(LB) && sample.centered() && edge(A) {
            Some("stand")
        } else if sample.down(LB) && edge(X) {
            Some("sit")
        } else if sample.down(LB) && sample.centered() && edge(Y) {
            Some("getup")
        } else if sample.down(LB) && edge(RB) {
            Some("quack")
        } else {
            None
        };
        if sample.neutral() {
            self.neutral_seen = true;
        }
        if !ready || action.is_some() {
            self.drive_armed = false;
        } else if sample.neutral() {
            self.drive_armed = true;
        }
        self.previous = sample.clone();
        action
    }
    fn drive(&self, sample: &Sample, seq: u64) -> serde_json::Value {
        let enabled = self.drive_armed && sample.down(LB);
        let [lx, ly, rx, ry] = if enabled { sample.axes } else { [0.; 4] };
        json!({"seq":seq,"action":"drive","forward":-ly,"turn":-lx,"headYaw":-rx,"headPitch":-ry,"speed":0.5,"compact":true})
    }
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
    for (key, down) in ["a", "b", "x", "y", "lb", "rb", "back", "start"].iter().zip(current.keys) {
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
    tokio::spawn(async move {
        let mut motion = Motion::local(app.clone(), router);
        let mut seq = 0u64;
        let mut last_error = String::new();
        while !app.shutdown.load(std::sync::atomic::Ordering::Acquire) {
            let result = async {
                let device = Arc::new(Device::open(&path, &name, axes)?);
                let (_monitor, samples) = monitor(app.clone(), name.clone(), device)?;
                last_error.clear();
                report(
                    &app,
                    "手柄已连接；松开全部按键并回中，再按 LB+A 取得控制",
                );
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
                    let ready = motion.leased && motion.status()["headReady"] == true;
                    if let Some(action) = input.update(&sample, ready) {
                        if action == "estop" {
                            // Emergency stop is global, including when a phone owns the lease.
                            motion.request("servos/disable", Some(json!({}))).await?;
                            motion.close().await;
                            input = Input::default();
                            report(&app, "手柄急停卸力");
                        } else if action == "release" {
                            motion.close().await;
                            input = Input::default();
                            report(&app, "手柄已停止保持并释放控制");
                        } else {
                            if !motion.leased {
                                if !matches!(action, "stand" | "getup") {
                                    continue;
                                }
                                if let Err(e) = motion.open() {
                                    report(&app, &e.to_string());
                                    continue;
                                }
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
    fn sample(keys: &[usize], axes: [f64; 4]) -> Sample {
        Sample {
            axes,
            keys: BUTTONS.map(|key| keys.contains(&key)),
            dpad: [false; 4],
        }
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
        assert_eq!(input.update(&start, false), None);
        input.update(&Sample::default(), false);
        assert_eq!(input.update(&start, false), Some("stand"));
        assert_eq!(input.update(&start, false), None);
    }
    #[test]
    fn start_is_reserved_and_stand_requires_centered_sticks() {
        let mut input = Input::default();
        input.update(&Sample::default(), false);
        assert_eq!(input.update(&sample(&[START], [0.; 4]), false), None);
        input.update(&Sample::default(), false);
        assert_eq!(input.update(&sample(&[LB, START], [0.; 4]), false), None);
        input.update(&Sample::default(), false);
        assert_eq!(input.update(&sample(&[LB, A], [1.; 4]), false), None);
        input.update(&Sample::default(), false);
        assert_eq!(input.update(&sample(&[LB, A], [0.; 4]), false), Some("stand"));
    }
    #[test]
    fn drive_requires_ready_then_release_and_deadman() {
        let mut input = Input::default();
        let held = sample(&[LB], [0., -1., 1., -1.]);
        input.update(&Sample::default(), false);
        input.update(&held, true);
        assert_eq!(input.drive(&held, 1)["forward"], 0.);
        input.update(&Sample::default(), true);
        input.update(&held, true);
        let drive = input.drive(&held, 2);
        assert_eq!(drive["forward"], 1.);
        assert_eq!(drive["headYaw"], -1.);
        assert_eq!(input.drive(&sample(&[], held.axes), 3)["forward"], 0.);
        input.update(&held, false);
        input.update(&held, true);
        assert_eq!(input.drive(&held, 4)["forward"], 0.);
    }
    #[test]
    fn emergency_and_release_work_before_neutral_and_override_skills() {
        let mut input = Input::default();
        assert_eq!(
            input.update(&sample(&[BACK, LB, START], [1.; 4]), false),
            Some("estop")
        );
        assert_eq!(
            Input::default().update(&sample(&[B], [1.; 4]), false),
            Some("release")
        );
    }
}
