//! Narrow BlueZ pairing controls exposed to the local Observer UI.
use anyhow::{anyhow, Context, Result};
use axum::{body::Bytes, extract::State, http::{HeaderMap, StatusCode}, response::{IntoResponse, Response}, routing::post, Json, Router};
use serde_json::{json, Value};
use std::{time::Duration, process::Stdio};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, process::Command};
use crate::{check_action, error, App};

async fn command(args: &[&str], input: Option<&str>, timeout: Duration) -> Result<String> {
    let mut child = Command::new("bluetoothctl").args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn().context("无法启动 bluetoothctl；请检查主板 BlueZ 是否已安装")?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(text.as_bytes()).await?;
        stdin.shutdown().await?;
    }
    let output = tokio::time::timeout(timeout, child.wait_with_output()).await.context("蓝牙操作超时")??;
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if !output.status.success() || text.lines().any(|line| line.contains("Failed") || line.contains("not available")) {
        let clean = strip_terminal_noise(&text);
        let reason = clean.lines().map(str::trim).find(|line| line.starts_with("Failed") || line.contains("No agent is registered") || line.contains("AuthenticationFailed") || line.contains("not available"))
            .or_else(|| clean.lines().map(str::trim).rev().find(|line| !line.is_empty()))
            .unwrap_or("BlueZ 操作失败");
        return Err(anyhow!(reason.to_string()));
    }
    Ok(text)
}

async fn pair_during_discovery(address: &str) -> Result<String> {
    let mut child = Command::new("bluetoothctl").args(["--agent", "NoInputNoOutput"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true)
        .spawn().context("无法启动 bluetoothctl 配对会话")?;
    let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("无法打开蓝牙会话输入"))?;
    let mut stdout = child.stdout.take().ok_or_else(|| anyhow!("无法读取蓝牙会话输出"))?;
    let mut stderr = child.stderr.take().ok_or_else(|| anyhow!("无法读取蓝牙会话错误"))?;
    let stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes).await;
        bytes
    });
    let mut output = Vec::new();
    let mut buffer = [0u8; 2048];

    // Keep discovery alive in the same bluetoothctl/BlueZ client as Pair().
    // BlueZ may remove unpaired LE device objects when a short scan exits, so
    // a MAC returned by the separate UI scan can otherwise become unavailable.
    stdin.write_all(b"scan on\n").await?;
    let scan_until = tokio::time::Instant::now() + Duration::from_secs(4);
    loop {
        tokio::select! {
            result = stdout.read(&mut buffer) => {
                let n = result?;
                if n == 0 { break; }
                output.extend_from_slice(&buffer[..n]);
            }
            _ = tokio::time::sleep_until(scan_until) => break,
        }
    }
    stdin.write_all(format!("pair {address}\n").as_bytes()).await?;
    let pair_until = tokio::time::Instant::now() + Duration::from_secs(25);
    let mut pair_finished = false;
    loop {
        tokio::select! {
            result = stdout.read(&mut buffer) => {
                let n = result?;
                if n == 0 { break; }
                output.extend_from_slice(&buffer[..n]);
                let text = strip_terminal_noise(&String::from_utf8_lossy(&output));
                if text.contains("Pairing successful") || text.contains("Paired: yes") || text.contains("Failed to pair")
                    || text.contains("not available") || text.contains("No agent is registered")
                    || text.contains("Failed to register agent") { pair_finished = true; break; }
            }
            _ = tokio::time::sleep_until(pair_until) => break,
        }
    }
    let _ = stdin.write_all(b"scan off\nquit\n").await;
    let _ = stdin.shutdown().await;
    drop(stdin);
    if tokio::time::timeout(Duration::from_secs(3), child.wait()).await.is_err() { let _ = child.kill().await; }
    let errors = tokio::time::timeout(Duration::from_secs(2), stderr_task).await.ok().and_then(Result::ok).unwrap_or_default();
    output.extend_from_slice(&errors);
    let text = strip_terminal_noise(&String::from_utf8_lossy(&output));
    if text.contains("Failed to pair") || text.contains("not available")
        || text.contains("No agent is registered") || text.contains("Failed to register agent") {
        let reason = text.lines().map(str::trim).find(|line| line.starts_with("Failed") || line.contains("No agent") || line.contains("not available"))
            .unwrap_or("蓝牙配对失败");
        return Err(anyhow!(reason.to_string()));
    }
    if !pair_finished || !(text.contains("Pairing successful") || text.contains("Paired: yes")) {
        return Err(anyhow!("扫描期间没有收到配对完成结果；请让手柄保持快闪，再重新扫描并立即配对"));
    }
    Ok(text)
}

fn strip_terminal_noise(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for code in chars.by_ref() { if ('@'..='~').contains(&code) { break; } }
            }
        } else if ch == '\u{8}' {
            clean.pop();
        } else {
            clean.push(ch);
        }
    }
    clean.replace('\r', "")
}

fn parse_devices(output: &str) -> Vec<Value> {
    let mut devices: std::collections::BTreeMap<String, String> = Default::default();
    for line in output.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("Device ").or_else(|| line.split_once("] Device ").map(|(_, rest)| rest)) else { continue };
        let Some((address, raw_name)) = rest.split_once(' ') else { continue };
        if !valid_address(address) { continue; }
        // Ignore RSSI, Connected, Paired and other property-change lines. Keep only an actual name.
        let raw_name = raw_name.trim();
        let name = if let Some((field, value)) = raw_name.split_once(':') {
            if field == "Name" || field == "Alias" { value.trim() } else { continue; }
        } else { raw_name };
        if name.is_empty() { continue; }
        devices.insert(address.to_ascii_uppercase(), name.to_string());
    }
    devices.into_iter().map(|(address, name)| {
        let n = name.to_ascii_lowercase();
        let likely_controller = n.contains("controller") || n.contains("gamesir") || n.contains("xbox");
        json!({"address":address,"name":name,"likelyController":likely_controller})
    }).collect()
}
fn valid_address(s: &str) -> bool {
    s.len() == 17 && s.split(':').count() == 6 && s.split(':').all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn router() -> Router<App> {
    Router::new().route("/api/v1/gamepad/bluetooth/scan", post(scan)).route("/api/v1/gamepad/bluetooth/connect", post(connect))
}
async fn scan(State(_): State<App>, headers: HeaderMap, body: Bytes) -> Response {
    if let Err(response) = check_action(&headers, &body, 64) { return response; }
    match command(&["--timeout", "10", "scan", "on"], None, Duration::from_secs(14)).await {
        Ok(output) => Json(json!({"devices":parse_devices(&output),"message":"扫描结束。手柄进入配对模式后未出现时，请重新扫描。"})).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"detail":format!("蓝牙扫描失败：{e}")}))).into_response(),
    }
}
async fn connect(State(_): State<App>, headers: HeaderMap, body: Bytes) -> Response {
    let value = match check_action(&headers, &body, 128) { Ok(v) => v, Err(r) => return r };
    let address = value["address"].as_str().unwrap_or("").to_ascii_uppercase();
    if !valid_address(&address) { return error(StatusCode::UNPROCESSABLE_ENTITY, "蓝牙设备地址无效"); }
    let already_paired = command(&["info", &address], None, Duration::from_secs(8)).await.map(|text|text.lines().any(|line|line.trim()=="Paired: yes")).unwrap_or(false);
    if !already_paired {
        let pairable_before = match command(&["show"], None, Duration::from_secs(8)).await {
            Ok(text) => pairable_state(&text),
            Err(e) => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"detail":format!("读取主板蓝牙配对状态失败：{e}")}))).into_response(),
        };
        if !pairable_before {
            if let Err(e) = command(&["pairable", "on"], None, Duration::from_secs(8)).await {
                return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"detail":format!("无法临时开启蓝牙配对：{e}")}))).into_response();
            }
        }
        // `--agent` registers bluetoothctl's Agent1 handler before running its
        // non-interactive Pair command. Registering it with an `agent ...`
        // stdin command fails on the board's BlueZ build before Pair begins.
        let pair_result = pair_during_discovery(&address).await;
        let restore_result = if pairable_before { Ok(String::new()) } else { command(&["pairable", "off"], None, Duration::from_secs(8)).await };
        if let Err(e) = restore_result {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"detail":format!("配对尝试已结束，但恢复主板蓝牙配对状态失败：{e}")}))).into_response();
        }
        if let Err(e) = pair_result {
            let msg=e.to_string(); let lower=msg.to_ascii_lowercase();
            if !lower.contains("already paired") && !lower.contains("alreadyexists") {
                return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"detail":format!("配对失败：{msg}")}))).into_response();
            }
        }
    }
    if let Err(e) = command(&["trust", &address], None, Duration::from_secs(10)).await { return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"detail":format!("已配对，但设为信任设备失败：{e}")}))).into_response(); }
    match command(&["connect", &address], None, Duration::from_secs(20)).await {
        Ok(_) => Json(json!({"address":address,"connected":true,"message":"配对并连接成功。手柄控制是否可用取决于主板是否创建了兼容的 Linux 输入设备。"})).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"detail":format!("已配对并信任，但连接失败：{e}")}))).into_response(),
    }
}

fn pairable_state(output: &str) -> bool {
    output.lines().any(|line| line.trim() == "Pairable: yes")
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn address_validation_is_strict() { assert!(valid_address("AA:bb:01:02:FE:99")); assert!(!valid_address("AA:BB:01:02:FE:GG")); assert!(!valid_address("bluetoothctl;reboot")); }
    #[test] fn scan_parses_names_and_ignores_property_updates() { let d=parse_devices("[NEW] Device AA:BB:CC:DD:EE:FF Name: Wireless Controller\n[CHG] Device AA:BB:CC:DD:EE:FF RSSI: -48\n[CHG] Device 10:20:30:40:50:60 ManufacturerData.Value: aabb\nDevice nope bad\n"); assert_eq!(d.len(),1); assert_eq!(d[0]["name"],"Wireless Controller"); assert_eq!(d[0]["likelyController"],true); }
    #[test] fn pairable_state_only_accepts_explicit_yes() { assert!(pairable_state("Controller hci0\n\tPairable: yes\n")); assert!(!pairable_state("\tPairable: no\n")); }
    #[test] fn terminal_noise_is_removed_and_backspaces_are_applied() { assert_eq!(strip_terminal_noise("\x1b[0;94m[bluetoothctl]> \x1b[0mFailedd\x08 to pair\r\n"), "[bluetoothctl]> Failed to pair\n"); }
}
