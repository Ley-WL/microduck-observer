//! HTTP routes are linked by production camera code but never served by hardware examples.
use axum::{body::Bytes, http::{HeaderMap, StatusCode}, response::{IntoResponse, Response}, Json};
#[derive(Clone)]
pub struct App { pub hardware: bool }
pub fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}
pub fn check_write(_: &HeaderMap, _: &Bytes, _: usize) -> Result<serde_json::Value, Response> {
    Err(error(StatusCode::FORBIDDEN, "HTTP control is disabled in hardware acceptance examples"))
}
