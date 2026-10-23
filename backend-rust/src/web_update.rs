//! Browser transport for signed local bundles; privileged work stays in the OTA helper.
use crate::{App,check_write,error,observer_ota};
use axum::{body::{Body,Bytes},extract::{DefaultBodyLimit,State},http::{HeaderMap,StatusCode},response::{IntoResponse,Response},routing::{get,post},Json,Router};
use futures::StreamExt;
use serde_json::{json,Value};
use std::{path::PathBuf,time::Duration};
use tokio::io::AsyncWriteExt;
const MAX:u64=128*1024*1024;
static UPLOAD:tokio::sync::Mutex<()>=tokio::sync::Mutex::const_new(());
pub const UPLOAD_DIR:&str="/var/lib/microduck-observer/update-uploads";
pub fn router()->Router<App>{Router::new()
    .route("/api/v1/update",get(status))
    .route("/api/v1/update/upload",post(upload).layer(DefaultBodyLimit::disable()))
    .route("/api/v1/update/install",post(install))}
async fn status(State(a):State<App>)->Json<Value>{
    let helper=if a.hardware {observer_ota::request_value(json!({"op":"bundle-info"})).await.ok()} else {None};
    Json(json!({"protocol":1,"available":helper.is_some(),"current":crate::service_version(),"bootId":a.telemetry.read().unwrap().boot,"maxBytes":MAX,"status":observer_ota::status(),"message":if helper.is_some(){""}else{"需要先安装支持网页更新的 OTA helper"}}))
}
fn upload_headers(h:&HeaderMap)->Result<(),Response>{
    if h.get("x-microduck-update").and_then(|v|v.to_str().ok())!=Some("1") {return Err(error(StatusCode::BAD_REQUEST,"缺少更新请求标识"));}
    // Use the same origin policy as other state-changing Observer endpoints.
    let mut origin_headers=h.clone();origin_headers.insert("content-type","application/json".parse().unwrap());
    check_write(&origin_headers,&Bytes::from_static(b"{}"),16).map(|_|())
}
struct Temp(PathBuf);
impl Drop for Temp {fn drop(&mut self){let _=std::fs::remove_file(&self.0);}}
async fn upload(State(a):State<App>,h:HeaderMap,body:Body)->Response{
    if let Err(e)=upload_headers(&h){return e;}
    if !a.hardware || !observer_ota::enabled(){return error(StatusCode::SERVICE_UNAVAILABLE,"主板更新组件未就绪");}
    let Ok(_guard)=UPLOAD.try_lock() else{return error(StatusCode::CONFLICT,"已有文件正在上传");};
    if observer_ota::busy(&observer_ota::status()){return error(StatusCode::CONFLICT,"正在更新，请稍后再上传");}
    let result=async {
        use anyhow::ensure;
        if let Some(n)=h.get("content-length").and_then(|v|v.to_str().ok()).and_then(|v|v.parse::<u64>().ok()){ensure!(n>0&&n<=MAX,"更新包大小须在128MiB以内");}
        let dir=PathBuf::from(UPLOAD_DIR);tokio::fs::create_dir_all(&dir).await?;
        let id=uuid::Uuid::new_v4().to_string();let temp=Temp(dir.join(format!("{id}.tar.gz")));
        let mut file=tokio::fs::OpenOptions::new().create_new(true).write(true).open(&temp.0).await?;
        let mut stream=body.into_data_stream();let mut size=0u64;
        tokio::time::timeout(Duration::from_secs(180),async {
            while let Some(chunk)=stream.next().await {let chunk=chunk?;size+=chunk.len() as u64;ensure!(size<=MAX,"更新包超过128MiB");file.write_all(&chunk).await?;}
            ensure!(size>0,"更新包为空");file.sync_all().await?;Ok::<_,anyhow::Error>(())
        }).await??;
        drop(file);
        observer_ota::request_value(json!({"op":"bundle-stage","id":id})).await
    }.await;
    match result {Ok(v)=>Json(v).into_response(),Err(e)=>error(StatusCode::UNPROCESSABLE_ENTITY,&e.to_string())}
}
async fn install(State(a):State<App>,h:HeaderMap,b:Bytes)->Response{
    let v=match check_write(&h,&b,1024){Ok(v)=>v,Err(e)=>return e};
    let result=async {
        anyhow::ensure!(a.hardware&&v["confirm"]==true,"请确认更新真实主板");
        anyhow::ensure!(v["bootId"]==a.telemetry.read().unwrap().boot,"主板已重启，请重新确认");
        let id=v["id"].as_str().ok_or_else(||anyhow::anyhow!("缺少更新包编号"))?;
        anyhow::ensure!(uuid::Uuid::parse_str(id).is_ok(),"无效更新包编号");
        let permit=a.gate.clone().try_acquire_owned().map_err(|_|anyhow::anyhow!("设备有其他任务，请先停止并卸力"))?;
        let snapshot=crate::snapshot(State(a.clone())).await.0;
        anyhow::ensure!(observer_ota::idle(&snapshot),"必须15个舵机在线且全部卸力，才能安装");
        let result=observer_ota::request_value(json!({"op":"bundle-apply","id":id})).await?;
        anyhow::ensure!(result["accepted"]==true,"更新未被接受");
        tokio::spawn(async move {let _permit=permit;while observer_ota::busy(&observer_ota::status()){tokio::time::sleep(Duration::from_secs(1)).await;}});
        Ok::<_,anyhow::Error>(result)
    }.await;
    match result {Ok(v)=>(StatusCode::ACCEPTED,Json(v)).into_response(),Err(e)=>error(StatusCode::CONFLICT,&e.to_string())}
}
#[cfg(test)] mod tests {use super::*;
    #[test] fn upload_requires_non_simple_header_and_same_origin(){
        let mut h=HeaderMap::new();assert!(upload_headers(&h).is_err());
        h.insert("x-microduck-update","1".parse().unwrap());assert!(upload_headers(&h).is_ok());
        h.insert("host","duck:8877".parse().unwrap());h.insert("origin","http://evil.test".parse().unwrap());assert!(upload_headers(&h).is_err());
        h.insert("origin","http://duck:8877".parse().unwrap());assert!(upload_headers(&h).is_ok());
    }
}
