use serde_json::{json,Value};
pub const STATE:&str="/var/lib/microduck-observer/ota/status.json";
pub fn busy(v:&Value)->bool{matches!(v["phase"].as_str(),Some("queued"|"downloading"|"installing"|"recovering"))}
fn expire(v:Value,age:std::time::Duration)->Value{if busy(&v)&&v["phase"]!="recovering"&&age.as_secs()>330{json!({"phase":"failed","message":"更新进程已超时，请重新检查版本"})}else{v}}
pub fn status()->Value{let v=std::fs::read(STATE).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(json!({"phase":"idle"}));let age=std::fs::metadata(STATE).and_then(|m|m.modified()).ok().and_then(|m|m.elapsed().ok()).unwrap_or_default();expire(v,age)}
// Reacquire the motion gate after the OTA restarts this daemon, through validation.
pub fn startup_lock(gate:std::sync::Arc<tokio::sync::Semaphore>){if !busy(&status()){return;}if let Ok(permit)=gate.try_acquire_owned(){tokio::spawn(async move{let _permit=permit;while busy(&status()){tokio::time::sleep(std::time::Duration::from_secs(1)).await;}});}}

pub fn enabled()->bool{std::path::Path::new("/run/microduck-observer-ota.sock").exists()}
pub fn idle(s:&Value)->bool {
    s["system"]["data"]["servoControl"]["state"].as_str().is_some_and(|v|matches!(v,"idle"|"disabled"|"holding"|"failed"))
    && s["joints"]["ageMs"].as_f64().is_some_and(|a|a>=0.&&a<1000.)
    && s["joints"]["data"]["servos"].as_array().is_some_and(|r|r.len()==15&&r.iter().all(|v|v["online"]==true&&v["torque"].as_i64()==Some(0)&&v["ageMs"].as_f64().is_some_and(|a|a>=0.&&a<1000.)))
}
pub async fn request(op:&str)->anyhow::Result<Value>{
    anyhow::ensure!(matches!(op,"check"|"apply"),"不支持的更新操作");
    request_value(json!({"op":op})).await
}
pub async fn request_value(request:Value)->anyhow::Result<Value>{
    #[cfg(unix)]{
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let mut socket=tokio::net::UnixStream::connect("/run/microduck-observer-ota.sock").await?;
        socket.write_all(format!("{request}\n").as_bytes()).await?;socket.shutdown().await?;
        let mut bytes=Vec::new();tokio::time::timeout(std::time::Duration::from_secs(75),socket.take(16384).read_to_end(&mut bytes)).await??;
        let v:Value=serde_json::from_slice(&bytes)?;anyhow::ensure!(v["error"].is_null(),"{}",v["error"]);Ok(v)
    }
    #[cfg(not(unix))]{let _=request;anyhow::bail!("仅Linux主板支持OTA")}
}
#[cfg(test)]mod tests{use super::*;#[test]fn requires_all_unloaded(){let mut s=json!({"system":{"data":{"servoControl":{"state":"disabled"}}},"joints":{"ageMs":0,"data":{"servos":(0..15).map(|_|json!({"online":true,"torque":0,"ageMs":0})).collect::<Vec<_>>()}}});assert!(idle(&s));s["joints"]["data"]["servos"][0]["torque"]=json!(1);assert!(!idle(&s));s["joints"]["data"]["servos"][0]["torque"]=json!(0);s["system"]["data"]["servoControl"]["state"]=json!("policy");assert!(!idle(&s));}}

#[cfg(test)]mod freshness_tests{use super::*;#[test]fn rejects_stale_snapshot_even_when_rows_were_fresh(){let mut s=json!({"system":{"data":{"servoControl":{"state":"idle"}}},"joints":{"ageMs":1500,"data":{"servos":(0..15).map(|_|json!({"online":true,"torque":0,"ageMs":0})).collect::<Vec<_>>()}}});assert!(!idle(&s));s["joints"]["ageMs"]=json!(0);assert!(idle(&s));s["system"]["data"]["servoControl"]["state"]=Value::Null;assert!(!idle(&s));}}

#[cfg(test)]mod state_tests{use super::*;#[test]fn updating_stays_locked_across_restart_and_expires(){for phase in ["queued","downloading","installing"]{let v=json!({"phase":phase});assert!(busy(&v));assert!(busy(&expire(v.clone(),std::time::Duration::from_secs(330))));assert!(!busy(&expire(v,std::time::Duration::from_secs(331))));}assert!(busy(&expire(json!({"phase":"recovering"}),std::time::Duration::from_secs(999))));assert!(!busy(&json!({"phase":"success"})));assert!(!busy(&json!({"phase":"failed"})));}}
