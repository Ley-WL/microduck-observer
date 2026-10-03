use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub fn enabled() -> bool {
    std::env::var("MICRODUCK_POWER_CONTROL").as_deref() == Ok("1")
}

pub async fn request(action: &str) -> anyhow::Result<Value> {
    #[cfg(unix)]
    {
        let mut socket = tokio::net::UnixStream::connect("/run/microduck-power.sock").await?;
        socket.write_all(format!("{{\"action\":\"{action}\"}}\n").as_bytes()).await?;
        socket.shutdown().await?;
        let mut reply = Vec::new();
        socket.take(4096).read_to_end(&mut reply).await?;
        let result: Value = serde_json::from_slice(&reply)?;
        anyhow::ensure!(result["accepted"] == true, "{}", result["detail"].as_str().unwrap_or("电源请求失败"));
        Ok(result)
    }
    #[cfg(not(unix))]
    {
        let _ = action;
        anyhow::bail!("仅支持Linux主板")
    }
}

pub fn validate(action: &str, value: &Value, boot: &str) -> bool {
    matches!(action, "poweroff" | "reboot") && value == &json!({"confirm":true,"bootId":boot})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unconfirmed_stale_and_arbitrary_commands() {
        let body = json!({"confirm":true,"bootId":"current"});
        assert!(validate("reboot", &body, "current"));
        assert!(validate("poweroff", &body, "current"));
        assert!(!validate("reboot", &body, "old"));
        assert!(!validate("reboot; id", &body, "current"));
        assert!(!validate("reboot", &json!({"bootId":"current"}), "current"));
        assert!(!validate("reboot", &json!({"confirm":true,"bootId":"current","command":"id"}), "current"));
    }
}
