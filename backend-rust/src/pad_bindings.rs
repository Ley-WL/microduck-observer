//! Shared virtual/evdev button bindings. Fixed safety controls cannot be overridden.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Write, path::{Path, PathBuf}, sync::{Mutex, OnceLock}};

pub const BUTTONS: [&str;8] = ["a","x","y","lb","rb","up","left","right"];
pub const ACTIONS: [&str;12] = ["none","standup","sit","getup","demo","pick","roulade","kick_left","kick_right","dance","flamingo","quack"];
#[derive(Clone, Debug, PartialEq)]
pub struct Config {pub schema_version:u32,pub revision:u64,pub bindings:BTreeMap<String,String>}
impl Default for Config {
    fn default()->Self {Self {schema_version:1,revision:0,bindings:BUTTONS.into_iter().zip(["standup","sit","getup","demo","pick","dance","flamingo","roulade"]).map(|(k,v)|(k.into(),v.into())).collect()}}
}
impl Config {
    fn json(&self)->Value{json!({"schemaVersion":self.schema_version,"revision":self.revision,"bindings":self.bindings})}
    fn parse(value:Value)->Result<Self>{
        ensure!(value["schemaVersion"]==1,"手柄配置版本不支持");
        let config=Self{schema_version:1,revision:value["revision"].as_u64().context("手柄配置版本号无效")?,bindings:serde_json::from_value(value["bindings"].clone())?};
        config.validate()?;Ok(config)
    }

    pub fn action(&self,button:&str)->&str {
        let value=self.bindings.get(button).map(String::as_str).unwrap_or("none");
        value
    }
    fn validate(&self)->Result<()> {
        ensure!(self.schema_version==1,"手柄配置版本不支持");
        ensure!(self.bindings.len()==BUTTONS.len() && BUTTONS.iter().all(|k|self.bindings.contains_key(*k)),"必须提供8个可配置按键；固定按键不能修改");
        ensure!(self.bindings.values().all(|v|ACTIONS.contains(&v.as_str()) || crate::model_registry::is_custom(v)),"未知手柄动作");
        Ok(())
    }
}
static CURRENT:OnceLock<Mutex<Config>>=OnceLock::new();
fn current()->&'static Mutex<Config>{CURRENT.get_or_init(||Mutex::new(Config::default()))}
fn path()->PathBuf {PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE").unwrap_or("./state/calibration.json".into())).with_file_name("gamepad-bindings.json")}
pub fn init()->Result<()> {
    let p=path();let config=if p.exists(){Config::parse(serde_json::from_slice(&std::fs::read(p)?)?)?}else{Config::default()};
    config.validate()?;*current().lock().unwrap()=config;Ok(())
}
pub fn snapshot()->Config{current().lock().unwrap().clone()}
pub fn info()->Value {
 let custom=crate::model_registry::custom_actions();
 let mut actions:Vec<String>=ACTIONS.iter().map(|v|v.to_string()).collect();
 actions.extend(custom.iter().filter_map(|a|a["id"].as_str().map(str::to_owned)));
 json!({"config":snapshot().json(),"defaults":Config::default().bindings,"buttons":BUTTONS,"actions":actions,"customActions":custom})
}
fn next(old:&Config,body:&Value)->Result<Config>{
    ensure!(body["revision"].as_u64()==Some(old.revision),"手柄配置已更新，请刷新后重试");
    let config=Config {schema_version:1,revision:old.revision.checked_add(1).context("版本号溢出")?,bindings:serde_json::from_value(body["bindings"].clone())?};
    config.validate()?;Ok(config)
}
fn persist(path:&Path,config:&Config)->Result<()> {
    let parent=path.parent().context("手柄配置目录无效")?;std::fs::create_dir_all(parent)?;
    let tmp=path.with_extension(format!("{}.tmp",uuid::Uuid::new_v4()));
    let result=(||->Result<()> {
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(&serde_json::to_vec_pretty(&config.json())?)?;file.sync_all()?;drop(file);
        std::fs::rename(&tmp,path)?;
        if let Err(e)=std::fs::File::open(parent).and_then(|f|f.sync_all()){eprintln!("gamepad bindings directory sync after commit: {e}");}
        Ok(())
    })();
    if result.is_err(){let _=std::fs::remove_file(tmp);}result
}
pub fn update(body:&Value)->Result<Value>{
    let mut old=current().lock().unwrap();let config=next(&old,body)?;
    persist(&path(),&config)?;*old=config.clone();Ok(json!({"saved":true,"config":config.json(),"message":"已保存。实体手柄松开全部按键并回中后生效；网页返回姿态页读取新配置。"}))
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn invalid_bindings_and_stale_revisions_do_not_replace_config(){
        let old=Config::default();let mut body=json!({"revision":0,"bindings":old.bindings});
        body["bindings"]["x"]=json!("kick_left");assert_eq!(next(&old,&body).unwrap().action("x"),"kick_left");
        body["revision"]=json!(1);assert!(next(&old,&body).is_err());body["revision"]=json!(0);
        body["bindings"]["back"]=json!("none");assert!(next(&old,&body).is_err());body["bindings"].as_object_mut().unwrap().remove("back");
        body["bindings"]["x"]=json!("poweroff");assert!(next(&old,&body).is_err());
        body["bindings"].as_object_mut().unwrap().remove("y");assert!(next(&old,&body).is_err());
    }
    #[test] fn saved_mapping_survives_restart_and_failed_write_keeps_old_file(){
        let dir=tempfile::tempdir().unwrap();let p=dir.path().join("bindings.json");
        let mut config=Config::default();config.bindings.insert("rb".into(),"none".into());persist(&p,&config).unwrap();
        let loaded=Config::parse(serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap()).unwrap();assert_eq!(loaded,config);
        assert!(persist(&p.join("invalid"),&Config::default()).is_err());
        assert_eq!(Config::parse(serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()).unwrap(),config);
    }
}
