use crate::telemetry::Shared;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
#[cfg(unix)]
async fn bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    out: &mut Vec<u8>,
) -> std::io::Result<usize> {
    use tokio::io::AsyncBufReadExt;
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(out.len());
        };
        let end = chunk.iter().position(|c| *c == b'\n').map(|i| i + 1);
        let n = end.unwrap_or(chunk.len());
        if out.len() + n > 65536 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ToF frame exceeds 64KiB",
            ));
        };
        out.extend_from_slice(&chunk[..n]);
        reader.consume(n);
        if end.is_some() {
            return Ok(out.len());
        }
    }
}
#[cfg(unix)]
pub async fn run(path: String, shared: Shared) {
    use tokio::io::{AsyncWriteExt, BufReader};
    loop {
        let result=async{let mut stream=tokio::net::UnixStream::connect(&path).await?;stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tof.stream\",\"params\":{}}\n").await?;let mut reader=BufReader::new(stream);let mut previous=None;let mut times=std::collections::VecDeque::new();loop{let mut line=vec![];let n=tokio::time::timeout(Duration::from_secs(3),bounded_line(&mut reader,&mut line)).await??;if n==0||line.len()>65536{anyhow::bail!("ToF stream closed or oversized")}let v:Value=serde_json::from_slice(&line)?;if !v["error"].is_null(){anyhow::bail!("ToF subscription rejected")}if v["method"]!="tof.frame"{continue}let p=&v["params"];validate(p)?;let seq=p["seq"].as_u64().unwrap();let ns=p["t_ns"].as_u64().unwrap();if previous.is_some_and(|(s,t)|seq<=s||ns<=t){anyhow::bail!("ToF sequence/timestamp reset")}times.push_back(ns);if times.len()>30{times.pop_front();}let hz=if times.len()>1{(times.len()-1)as f64*1e9/(ns-*times.front().unwrap())as f64}else{0.};let mut s=shared.write().unwrap();s.tof=json!({"state":"streaming","error":"","ageMs":0,"hz":hz});s.sample("tof",json!({"rows":8,"cols":8,"distanceMm":p["distance_mm"],"status":p["status"],"sensorSeq":seq,"sensorTimeNs":ns,"hz":hz,"sequenceGap":previous.map(|(s,_)|seq-s-1).unwrap_or(0),"sensor":"VL53L5CX"}),true,Instant::now());if let Some(v)=s.latest.get_mut("tof"){v["source"]=json!("hardware");}previous=Some((seq,ns));}#[allow(unreachable_code)]Ok::<(),anyhow::Error>(())}.await;
        if let Err(e) = result {
            shared.write().unwrap().tof =
                json!({"state":"offline","error":e.to_string(),"ageMs":null,"hz":0});
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
#[cfg(not(unix))]
pub async fn run(_path: String, _shared: Shared) {}
fn validate(p: &Value) -> anyhow::Result<()> {
    if p["rows"] != 8 || p["cols"] != 8 {
        anyhow::bail!("Expected an 8x8 ToF frame")
    }
    for (key, max) in [("distance_mm", 65535), ("status", 255)] {
        let a = p[key]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid ToF {key}"))?;
        if a.len() != 64 || a.iter().any(|x| x.as_u64().map_or(true, |x| x > max)) {
            anyhow::bail!("Invalid ToF {key}")
        }
    }
    if p["seq"].as_u64().is_none() || p["t_ns"].as_u64().is_none() {
        anyhow::bail!("Invalid ToF timestamp")
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_frames() {
        assert!(validate(&json!({"rows":8,"cols":8,"seq":1,"t_ns":2,"distance_mm":vec![0;64],"status":vec![5;64]})).is_ok());
        assert!(validate(&json!({"rows":8,"cols":8,"seq":1,"t_ns":2,"distance_mm":vec![0;63],"status":vec![5;64]})).is_err());
    }
}
