use std::io::{self, Read};
use serde_json::{json, Value};

fn valid(v: &Value) -> Option<&str> {
    let object = v.as_object()?;
    if object.len() != 1 { return None; }
    match object.get("action")?.as_str()? { "poweroff" => Some("poweroff"), "reboot" => Some("reboot"), _ => None }
}

#[tokio::main]
async fn main() {
    let mut input = String::new();
    let action = io::stdin().take(257).read_to_string(&mut input).ok()
        .and_then(|_| serde_json::from_str::<Value>(&input).ok());
    let reply = if let Some(action) = action.as_ref().and_then(valid) {
        let mut cmd = tokio::process::Command::new("/usr/bin/systemd-run");
        cmd.args(["--quiet", "--unit=microduck-power-pending", "--on-active=5s", "--timer-property=AccuracySec=1s", "/usr/bin/systemctl", action]).kill_on_drop(true);
        match tokio::time::timeout(std::time::Duration::from_secs(2), cmd.output()).await {
            Ok(Ok(out)) if out.status.success() => json!({"accepted":true,"action":action,"delaySeconds":5}),
            _ => json!({"accepted":false,"detail":"已有电源请求或系统调度失败"}),
        }
    } else { json!({"accepted":false,"detail":"Invalid power action"}) };
    println!("{reply}");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn only_fixed_actions() {
        assert_eq!(valid(&json!({"action":"reboot"})), Some("reboot"));
        for v in [json!({"action":"shutdown"}), json!({"action":"reboot","extra":1}), json!([]), json!({"action":null})] { assert_eq!(valid(&v), None); }
    }
}
