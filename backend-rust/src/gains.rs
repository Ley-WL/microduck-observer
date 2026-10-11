//! Persist only model runtime P/D; servo EEPROM and manual/HOME gains stay intact.
use anyhow::{ensure, Context, Result};
use std::{path::PathBuf, sync::{Mutex, OnceLock}};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gains { pub p: u8, pub d: u8 }
impl Default for Gains { fn default() -> Self { Self { p: 6, d: 20 } } }
static CURRENT: OnceLock<Mutex<Gains>> = OnceLock::new();
static WALK: OnceLock<Mutex<Option<Gains>>> = OnceLock::new();
fn walk_path() -> PathBuf { path().with_file_name("walk-model-gains.json") }
pub fn walking(default:Gains)->Gains { WALK.get_or_init(||Mutex::new(None)).lock().unwrap().unwrap_or(default) }
pub fn ui_current()->Gains { walking(current()) }
pub fn save_ui(value:Gains)->Result<()> {
    let mut walk=WALK.get_or_init(||Mutex::new(None)).lock().unwrap();
    if walk.is_some() {write(&walk_path(),value)?;*walk=Some(value);Ok(())} else {drop(walk);save(value)}
}
fn path() -> PathBuf {
    std::env::var_os("MICRODUCK_GAINS_FILE").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE").unwrap_or("./state/calibration.json".into()))
            .with_file_name("model-gains.json")
    })
}
pub fn init() -> Result<()> {
    let file = path();
    let value = if file.exists() { parse(&serde_json::from_slice(&std::fs::read(file)?)?).context("模型增益配置无效")? } else { Gains::default() };
    CURRENT.get_or_init(|| Mutex::new(value));
    let file=walk_path();
    let walk=if file.exists(){Some(parse(&serde_json::from_slice(&std::fs::read(file)?)?)?)}else{None};
    WALK.get_or_init(||Mutex::new(walk));
    Ok(())
}
pub fn current() -> Gains { *CURRENT.get_or_init(|| Mutex::new(Gains::default())).lock().unwrap() }
pub fn parse(v: &serde_json::Value) -> Result<Gains> {
    let p = v["p"].as_u64().context("P必须是0–255整数")?;
    let d = v["d"].as_u64().context("D必须是0–255整数")?;
    ensure!(p <= 255 && d <= 255, "P/D范围为0–255");
    Ok(Gains { p: p as u8, d: d as u8 })
}
pub fn save(value: Gains) -> Result<()> {
    let mut state = CURRENT.get_or_init(|| Mutex::new(Gains::default())).lock().unwrap();
    write(&path(), value)?;
    *state = value;
    Ok(())
}
fn write(file: &std::path::Path, value: Gains) -> Result<()> {
    std::fs::create_dir_all(file.parent().unwrap())?;
    let temp = file.with_extension("tmp");
    use std::io::Write;
    let mut output=std::fs::File::create(&temp)?;
    output.write_all(&serde_json::to_vec(&serde_json::json!({"p":value.p,"d":value.d}))?)?;
    output.sync_all()?;
    drop(output);
    std::fs::rename(temp, file)?;
    Ok(())
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn malformed_gains_cannot_wrap_or_truncate() {
        for v in [serde_json::json!({"p":256,"d":20}),serde_json::json!({"p":-1,"d":20}),serde_json::json!({"p":6.5,"d":20}),serde_json::json!({"p":6})] { assert!(parse(&v).is_err()); }
        assert_eq!(parse(&serde_json::json!({"p":255,"d":0})).unwrap(), Gains {p:255,d:0});
    }
    #[test] fn saved_default_is_replaced_and_can_be_loaded_after_restart() {
        let file = std::env::temp_dir().join(format!("ducklink-gains-{}.json",std::process::id()));
        write(&file, Gains::default()).unwrap();
        let value=Gains {p:8,d:24};write(&file,value).unwrap();
        assert_eq!(parse(&serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap()).unwrap(),value);
        std::fs::remove_file(file).unwrap();
    }
}
