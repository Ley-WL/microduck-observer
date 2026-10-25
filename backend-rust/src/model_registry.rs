//! Explicit policy slots. Mutations hold the shared motion gate; inference never reads this file.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeSet, io::Write, path::{Path, PathBuf}, sync::{Mutex, OnceLock}};

pub const MAX_MODEL_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize = 24 * 1024 * 1024;
pub const SLOTS: [&str; 11] = ["walk","sitstand","getup","pick","roulade","kick_left","kick_right","dance","flamingo","swizzle","roller_crouch"];
static CURRENT: OnceLock<Mutex<Value>> = OnceLock::new();

fn builtin_kinds(slot: &str) -> Result<&'static [&'static str]> {
    Ok(match slot {
        "swizzle" => &["swizzle_prepare","swizzle"],
        "roller_crouch" => &["swizzle_crouch"],
        "walk" => &["xgoduck"], "sitstand" => &["sitstand_sit","sitstand_stand"],
        "getup" => &["xgoduck_getup"], "pick" => &["xgoduck_pick"], "roulade" => &["xgoduck_roulade"],
        "kick_left" => &["kick_left"], "kick_right" => &["kick_right"], "dance" => &["dance10"],
        "flamingo" => &["flamingo_hold","flamingo_lower"], _ => bail!("未知模型槽位"),
    })
}
pub const CUSTOM_TEMPLATES: [&str;5] = ["dance","pick","roulade","kick_left","kick_right"];
fn actions_in(registry: &Value) -> Vec<Value> { registry["actions"].as_array().cloned().unwrap_or_default() }
fn kinds_in(registry: &Value, slot: &str) -> Result<&'static [&'static str]> {
    if SLOTS.contains(&slot) { return builtin_kinds(slot); }
    let action=actions_in(registry).into_iter().find(|a|a["id"]==slot).context("未知自定义动作")?;
    builtin_kinds(action["templateSlot"].as_str().context("动作执行模板缺失")?)
}
pub fn kinds(slot: &str) -> Result<&'static [&'static str]> { kinds_in(&current().lock().unwrap(),slot) }
pub fn custom_actions() -> Vec<Value> { actions_in(&current().lock().unwrap()) }
pub fn is_custom(id: &str) -> bool { id.starts_with("custom-") && custom_actions().iter().any(|a|a["id"]==id) }
pub fn execution_kind(id: &str) -> Result<String> {
    if id.starts_with("custom-") { Ok(kinds(id)?[0].to_owned()) } else { Ok(id.to_owned()) }
}
pub fn slot_for(kind: &str) -> Result<&'static str> {
    SLOTS.into_iter().find(|slot| builtin_kinds(slot).unwrap().contains(&kind)).context("未知模型类型")
}
fn state_path() -> PathBuf {
    std::env::var_os("MICRODUCK_MODEL_REGISTRY_FILE").map(PathBuf::from).unwrap_or_else(||
        PathBuf::from(std::env::var("MICRODUCK_CALIBRATION_FILE").unwrap_or("./state/calibration.json".into())).with_file_name("model-registry.json"))
}
fn artifacts() -> PathBuf { state_path().with_file_name("model-artifacts") }
fn defaults() -> Value { serde_json::from_str(include_str!("../../models/registry.json")).expect("bundled model registry") }
fn current() -> &'static Mutex<Value> { CURRENT.get_or_init(|| Mutex::new(defaults())) }
pub fn init() -> Result<()> {
    let file=state_path();
    let mut value=if file.exists() {serde_json::from_slice(&std::fs::read(file)?)?} else {defaults()};
    add_swizzle_slot(&mut value)?;
    validate(&value)?;
    *current().lock().unwrap()=value;
    Ok(())
}
// Additive migration preserves saved choices and rollback entries.
fn add_swizzle_slot(value: &mut Value) -> Result<()> {
    for slot in ["swizzle","roller_crouch"] {
    if value["selected"][slot].is_null() {
        let bundled=defaults();
        let item=bundled["models"].as_array().unwrap().iter().find(|v|v["slot"]==slot).unwrap().clone();
        let id=item["id"].as_str().unwrap().to_owned();
        ensure!(!value["models"].as_array().context("模型列表无效")?.iter().any(|m|m["id"]==id),"轮滑默认模型ID冲突");
        value["models"].as_array_mut().unwrap().push(item);
        value["selected"][slot]=json!(id);
    }
    }
    Ok(())
}
fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len()<=80 && value.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-' || b==b'_')
}
fn entry<'a>(registry: &'a Value, id: &str) -> Result<&'a Value> {
    registry["models"].as_array().context("模型列表缺失")?.iter().find(|m|m["id"]==id).context("模型未注册")
}
fn validate(registry: &Value) -> Result<()> {
    ensure!(registry["schemaVersion"]==1 && registry["revision"].as_u64().is_some(),"模型注册表版本无效");
    let models=registry["models"].as_array().context("模型列表无效")?;
    ensure!(models.len()<=128,"最多注册128个模型");
    let actions=actions_in(registry);
    ensure!(actions.len()<=16,"最多创建16个自定义动作");
    let mut action_ids=BTreeSet::new();
    for a in &actions {
        let id=a["id"].as_str().context("动作ID缺失")?;
        ensure!(id.starts_with("custom-") && id.len()>7 && identifier(id) && action_ids.insert(id),"动作ID无效或重复");
        let label=a["label"].as_str().context("动作名称缺失")?;
        ensure!(!label.trim().is_empty() && label.chars().count()<=80 && !label.chars().any(char::is_control),"动作名称无效");
        ensure!(CUSTOM_TEMPLATES.contains(&a["templateSlot"].as_str().unwrap_or("")),"不支持该动作执行模板");
        ensure!(registry["selected"][id].is_string(),"新动作必须配置模型");
    }
    let mut ids=BTreeSet::new();
    for model in models {
        let id=model["id"].as_str().context("模型ID缺失")?;
        ensure!(identifier(id) && ids.insert(id),"模型ID无效或重复");
        let label=model["label"].as_str().context("模型名称缺失")?;
        ensure!(!label.trim().is_empty() && label.chars().count()<=80 && !label.chars().any(char::is_control),"模型名称无效");
        kinds_in(registry,model["slot"].as_str().context("槽位缺失")?)?;
        if let Some(execution)=model["executionSlot"].as_str() {
            ensure!(builtin_kinds(execution)?==kinds_in(registry,model["slot"].as_str().unwrap())?,"模型执行模板与动作不匹配");
        }
        let file=model["file"].as_str().context("模型文件缺失")?;
        ensure!(file.ends_with(".onnx") && identifier(file.trim_end_matches(".onnx")),"模型文件必须为普通ONNX文件名");
        ensure!(model["imported"].is_null() || model["imported"].is_boolean(),"模型来源无效");
    }
    for field in ["selected","previous"] {
        let map=registry[field].as_object().context("模型选择无效")?;
        for (slot,id) in map {
            kinds_in(registry,slot)?;
            ensure!(entry(registry,id.as_str().context("模型ID无效")?)?["slot"]==slot.as_str(),"模型与槽位不匹配");
        }
    }
    for slot in SLOTS {ensure!(registry["selected"][slot].is_string(),"槽位 {slot} 未选择模型");}
    Ok(())
}
fn resolve_at(model: &Value, bundled: &Path, imported: &Path) -> Result<PathBuf> {
    let root=if model["imported"]==true {imported.join(model["id"].as_str().context("ID缺失")?)} else {bundled.to_path_buf()};
    let root=root.canonicalize().context("模型目录不存在")?;
    let path=root.join(model["file"].as_str().context("文件名缺失")?).canonicalize().context("模型文件不存在")?;
    ensure!(path.starts_with(&root),"模型文件不能指向模型目录之外");
    let metadata=path.with_extension("metadata.json").canonicalize().context("metadata文件不存在")?;
    ensure!(metadata.starts_with(&root),"metadata不能指向模型目录之外");
    Ok(path)
}
fn resolve(model: &Value) -> Result<PathBuf> {
    resolve_at(model,crate::policy::path().parent().context("模型目录无效")?,&artifacts())
}
pub fn selected_path(kind: &str) -> Result<PathBuf> {
    let registry=current().lock().unwrap();
    let slot=if kind.starts_with("custom-") {kind} else {slot_for(kind)?};
    resolve(entry(&registry,registry["selected"][slot].as_str().context("槽位未配置")?)?)
}
fn inspect(model: &Value, warm: bool) -> Result<Value> {
    inspect_path(model["executionSlot"].as_str().or(model["slot"].as_str()).context("槽位缺失")?, &resolve(model)?, warm)
}
fn inspect_path(slot: &str, path: &Path, warm: bool) -> Result<Value> {
    let mut metadata=Value::Null;
    for kind in kinds(slot)? {
        metadata=crate::policy::verified_metadata_at(kind,path)?;
        if warm {crate::policy::preflight_model_at(kind,path)?;}
    }
    Ok(metadata)
}
// Copy only execution settings, never a template model's training or acceptance claims.
fn template_metadata(source: &Value) -> Value {
    let mut result=json!({});
    for key in "artifactType controlHz actorObservationDim actionDim jointOrder homeRadians runtimeKind task actionScale actionEmaOldWeight actionEmaNewWeight actionSemantics runtimeActionAlpha observationNormalization normalization previousAction lastActionObservation headMapping commandEncoding commandGains commandHeightM commandSide durationSeconds phasePeriodSeconds endPhase lowerMinSeconds holdSeconds entrySeconds returnSeconds entryTimeoutSeconds settleSeconds transferSettleTicks poseEntryConsecutiveReadyTicks poseGate supportFoot swingFoot targetEncoding targetLimits voltageAdaptation encoderContract walkExecution skillExecution sitExecution kickFoot standSeconds sitSettleSeconds skillPhysicsRevision trainingTargetClipRadians mechanicalPhysicalJointLimitsRadians mechanicalExecutionContract assembledBodyGroundContactMeshes uprightHoldSeconds getupMinSeconds executionTargetLimitsDegrees angleLimitsRevision".split_whitespace() {
        if let Some(value)=source.get(key) {result[key]=value.clone();}
    }
    for field in ["walkExecution","skillExecution","sitExecution"] {
        if let Some(obj)=result[field].as_object_mut() {obj.remove("policySha256");obj.remove("gainSource");}
    }
    if source["runtimeKind"]=="official_flamingo" {
        result["sourceManifest"]=json!({"robot":{"servos":"xl330"}});
    }
    result["hardwareValidated"]=json!(false);
    result["simulationValidated"]=json!(false);
    result
}
fn bind_weights(mut metadata: Value, bytes: &[u8]) -> Value {
    use sha2::{Digest, Sha256};
    let sha=format!("{:x}",Sha256::digest(bytes));
    metadata["policySha256"]=json!(sha);
    for field in ["walkExecution","skillExecution","sitExecution"] {
        if metadata[field].is_object() {metadata[field]["policySha256"]=json!(sha);}
    }
    metadata
}
fn official_entry(manifest: &Value, file: &str) -> Result<Value> {
    ensure!(manifest.is_object(),"manifest必须是JSON对象");
    let mut result=manifest.clone();
    if let Some(entries)=manifest.get("policies") {
        let matches:Vec<_>=entries.as_array().context("policies必须为数组")?.iter().filter(|e|e["file"]==file).collect();
        ensure!(matches.len()==1,"manifest中必须有且只有一个与ONNX文件名相同的条目");
        result.as_object_mut().unwrap().remove("policies");
        for (key,value) in matches[0].as_object().context("manifest条目无效")? {result[key]=value.clone();}
    }
    ensure!(result["schema_version"]==2,"目前仅适配官方manifest schema 2");
    for (key,expected) in [("model_api",json!(1)),("obs_len",json!(61)),("action_len",json!(14))] {
        ensure!(result[key].is_null() || result[key]==expected,"官方manifest的 {key} 不兼容当前执行器（仅支持前馈API 1、61→14、scale 1）");
    }
    ensure!(result["action_scale"].is_null() || result["action_scale"].as_f64()==Some(1.),"官方action_scale仅支持1");
    for (key,expected) in [("model",json!("microduck")),("servos",json!("xl330")),("control_hz",json!(50))] {
        ensure!(result["robot"][key].is_null() || result["robot"][key]==expected,"官方manifest的robot.{key}不兼容");
    }
    ensure!(result["command"]["encoding"].is_null() || result["command"]["encoding"]=="constant","该命令编码尚未适配官方导入");
    ensure!(result["command"]["idle"].is_null() || result["command"]["idle"].as_array().is_some_and(|v|v.len()==3 && v.iter().all(|n|n.as_f64()==Some(0.))),"官方idle命令不兼容");
    ensure!(result["unwind_s"].is_null() && result["ramp_s"].is_null(),"自定义收尾/过渡时间尚未适配");
    Ok(result)
}
fn official_preset(slot: &str, manifest: &Value) -> Result<&'static str> {
    // The official set omits training.task_id on these two canonical entries.
    let canonical_kick=match (manifest["file"].as_str(),manifest["name"].as_str()) {
        (Some("ball_kick_left.onnx"),Some("kick_left"))=>Some("kick_left"),
        (Some("ball_kick_right.onnx"),Some("kick_right"))=>Some("kick_right"),_=>None,
    };
    if let Some(side)=canonical_kick {ensure!(slot==side,"官方踢球条目与左右脚槽位不匹配");}
    let task=manifest["training"]["task_id"].as_str().or(canonical_kick.map(|_|"Mjlab-BallKick-Flat-MicroDuck"))
        .context("适配需要training.task_id或官方模型集的标准踢球条目")?;
    match (slot,task) {
        ("kick_left"|"kick_right","Mjlab-BallKick-Flat-MicroDuck")=>{
            ensure!(manifest["kind"]=="episodic" && manifest["duration_s"].as_f64()==Some(0.5),"踢球仅支持episodic、0.5秒合同");
            Ok(if slot=="kick_left" {"kick-left-default"} else {"kick-right-default"})
        },
        ("flamingo","Mjlab-FlamingoCycleHard-Flat-MicroDuck")=>{
            ensure!(manifest["kind"]=="perpetual" && manifest["duration_s"].is_null(),"Flamingo需要perpetual合同");
            ensure!(manifest["eval"]["transition_time_s"].is_null() || manifest["eval"]["transition_time_s"].as_f64()==Some(1.5),"Flamingo过渡时间不兼容");
            Ok("flamingo-official")
        },
        _=>bail!("该官方任务/槽位尚未适配；目前支持BallKick左右脚与FlamingoCycleHard。其他模型请提供已适配的完整metadata。"),
    }
}
fn import_metadata(body: &Value, slot: &str, bytes: &[u8]) -> Result<Value> {
    let modes=["metadata","presetId","manifest"].iter().filter(|key|!body[**key].is_null()).count();
    ensure!(modes==1,"请选择一种配置来源：metadata、默认模板或官方manifest");
    if body["metadata"].is_object() {return Ok(body["metadata"].clone());}
    let manifest=if !body["manifest"].is_null() {Some(official_entry(&body["manifest"],body["onnxFilename"].as_str().context("缺少ONNX文件名")?)?)} else {None};
    let id=if let Some(m)=&manifest {official_preset(slot,m)?} else {body["presetId"].as_str().context("默认模板无效")?};
    let bundled=defaults();let model=entry(&bundled,id)?;
    ensure!(model["slot"]==slot,"默认模板与动作槽位不匹配");
    let mut metadata=template_metadata(&inspect(model,false)?);
    metadata["configurationSource"]=json!({"presetId":id,"mode":if manifest.is_some(){"official-manifest"}else{"preset"}});
    if let Some(m)=manifest {metadata["importedManifest"]=m;}
    Ok(bind_weights(metadata,bytes))
}
pub fn snapshot() -> Value {
    let mut value=current().lock().unwrap().clone();
    for model in value["models"].as_array_mut().unwrap() {
        match inspect(model,false) {
            Ok(meta)=>{model["available"]=json!(true);model["metadata"]=meta;},
            Err(e)=>{model["available"]=json!(false);model["error"]=json!(format!("{e:#}"));},
        }
    }
    value["presets"]=json!(value["models"].as_array().unwrap().iter().filter(|m|m["imported"]!=true && m["available"]==true).map(|m|json!({"id":m["id"],"slot":m["slot"],"label":m["label"],"metadata":template_metadata(&m["metadata"])})).collect::<Vec<_>>());
    value
}
pub fn idle(state: &str) -> bool { matches!(state,"idle"|"disabled"|"holding"|"failed") }
fn select_in(registry: &mut Value, slot: &str, id: &str) -> Result<()> {
    kinds_in(registry,slot)?;
    ensure!(entry(registry,id)?["slot"]==slot,"模型与槽位不匹配");
    if registry["selected"][slot]!=id {
        registry["previous"][slot]=registry["selected"][slot].clone();
        registry["selected"][slot]=json!(id);
    }
    Ok(())
}
fn persist(file: &Path, value: &Value) -> Result<()> {
    let parent=file.parent().context("注册表目录无效")?;
    std::fs::create_dir_all(parent)?;
    let temp=file.with_extension(format!("{}.tmp",uuid::Uuid::new_v4()));
    let result=(||->Result<()> {
        let mut output=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        output.write_all(&serde_json::to_vec_pretty(value)?)?;output.sync_all()?;drop(output);
        std::fs::rename(&temp,file)?;
        #[cfg(unix)]
        if let Err(e)=std::fs::File::open(parent).and_then(|f|f.sync_all()) {eprintln!("model registry directory sync failed after commit: {e}");}
        Ok(())
    })();
    if result.is_err() {let _=std::fs::remove_file(&temp);}
    result
}
// Called only with the shared motion gate held, including while ONNX warms up.
pub fn update(action: &str, body: &Value) -> Result<Value> {
    let mut next=current().lock().unwrap().clone();
    ensure!(body["revision"]==next["revision"],"模型列表已更新，请刷新后重试");
    let slot=body["slot"].as_str().context("缺少模型槽位")?;
    let creating=action=="import" && body["newAction"].is_object();
    if creating {
        ensure!(!SLOTS.contains(&slot) && !actions_in(&next).iter().any(|a|a["id"]==slot),"动作ID已存在");
        if next["actions"].is_null() {next["actions"]=json!([]);}
        next["actions"].as_array_mut().context("动作列表无效")?.push(json!({"id":slot,"label":body["newAction"]["label"],"templateSlot":body["newAction"]["templateSlot"]}));
    }
    let execution_slot=if SLOTS.contains(&slot) {slot.to_owned()} else {
        actions_in(&next).iter().find(|a|a["id"]==slot).and_then(|a|a["templateSlot"].as_str()).context("未知动作")?.to_owned()
    };
    kinds_in(&next,slot)?;
    let mut imported_dir=None;
    let result=(||->Result<()> {
        match action {
            "select"|"rollback"=>{
                let id=if action=="rollback" {next["previous"][slot].as_str().context("没有上一版本")?} else {body["id"].as_str().context("缺少模型ID")?}.to_owned();
                let model=entry(&next,&id)?;
                ensure!(model["slot"]==slot,"模型与槽位不匹配");
                inspect(model,true)?;
                select_in(&mut next,slot,&id)?;
            },
            "import"=>{
                use base64::Engine;
                ensure!(next["models"].as_array().unwrap().len()<128,"最多注册128个模型");
                let encoded=body["onnxBase64"].as_str().context("缺少ONNX文件")?;
                ensure!(encoded.len()<=MAX_MODEL_BYTES.div_ceil(3)*4,"模型超过16MiB");
                let bytes=base64::engine::general_purpose::STANDARD.decode(encoded).context("ONNX编码无效")?;
                ensure!(!bytes.is_empty() && bytes.len()<=MAX_MODEL_BYTES,"模型大小无效");
                ensure!(serde_json::to_vec(&body["metadata"])?.len()<=256*1024 && serde_json::to_vec(&body["manifest"])?.len()<=256*1024,"metadata或manifest过大");
                let metadata=import_metadata(body,&execution_slot,&bytes)?;
                let id=format!("user-{}",uuid::Uuid::new_v4());
                let model=json!({"id":id,"label":body["label"],"slot":slot,"file":"policy.onnx","imported":true,"executionSlot":execution_slot});
                next["models"].as_array_mut().unwrap().push(model.clone());
                if creating {next["selected"][slot]=json!(id);}
                validate(&next)?;
                std::fs::create_dir_all(artifacts())?;
                let dir=artifacts().join(&id);std::fs::create_dir(&dir)?;imported_dir=Some(dir.clone());
                for (name,data) in [("policy.onnx",bytes),("policy.metadata.json",serde_json::to_vec_pretty(&metadata)?) ] {
                    let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(name))?;
                    f.write_all(&data)?;f.sync_all()?;
                }
                #[cfg(unix)]
                {std::fs::File::open(&dir)?.sync_all()?;std::fs::File::open(artifacts())?.sync_all()?;}
                inspect_path(&execution_slot,&resolve(&model)?,true)?;
                // Import registers a candidate, never activates it or moves the robot.
            },
            _=>bail!("未知模型管理操作"),
        }
        next["revision"]=json!(next["revision"].as_u64().unwrap().checked_add(1).context("版本号溢出")?);
        validate(&next)?;
        let mut guard=current().lock().unwrap();
        ensure!(body["revision"]==guard["revision"],"模型列表已更新，请刷新后重试");
        persist(&state_path(),&next)?;
        *guard=next.clone();
        Ok(())
    })();
    if result.is_err() {
        if let Some(dir)=imported_dir {for name in ["policy.onnx","policy.metadata.json"] {let _=std::fs::remove_file(dir.join(name));}let _=std::fs::remove_dir(dir);}
    }
    result?;
    Ok(json!({"saved":true,"revision":next["revision"],"appliesOn":"next-model-start"}))
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn templates_keep_contracts_without_copying_acceptance_or_provenance() {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../models");
        let temp=tempfile::tempdir().unwrap();let path=temp.path().join("policy.onnx");
        let bytes=b"new candidate weights";std::fs::write(&path,bytes).unwrap();
        for model in defaults()["models"].as_array().unwrap() {
            let file=root.join(model["file"].as_str().unwrap()).with_extension("metadata.json");
            let source:Value=serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
            let metadata=bind_weights(template_metadata(&source),bytes);
            for key in ["sourceCheckpoint","sourceExperiment","simulationAcceptance","formalAcceptance","actuatorParamsSha256"] {assert!(metadata[key].is_null(),"{key}");}
            assert_eq!(metadata["hardwareValidated"],false);assert_eq!(metadata["simulationValidated"],false);
            assert_ne!(metadata["policySha256"],source["policySha256"]);
            std::fs::write(path.with_extension("metadata.json"),serde_json::to_vec(&metadata).unwrap()).unwrap();
            for kind in kinds(model["slot"].as_str().unwrap()).unwrap() {
                crate::policy::verified_metadata_at(kind,&path).unwrap_or_else(|e|panic!("{}: {e:#}",model["id"]));
            }
        }
    }
    #[test] fn official_manifest_matches_sets_and_rejects_unsupported_contracts() {
        let manifest=json!({"schema_version":2,"model_api":1,"obs_len":61,"action_len":14,
            "policies":[{"file":"kick.onnx","kind":"episodic","duration_s":0.5,"training":{"task_id":"Mjlab-BallKick-Flat-MicroDuck"}}]});
        let m=official_entry(&manifest,"kick.onnx").unwrap();
        assert_eq!(official_preset("kick_left",&m).unwrap(),"kick-left-default");
        assert!(official_preset("walk",&m).is_err());
        assert!(official_entry(&manifest,"missing.onnx").is_err());
        let mut duplicate=manifest.clone();duplicate["policies"].as_array_mut().unwrap().push(manifest["policies"][0].clone());
        assert!(official_entry(&duplicate,"kick.onnx").is_err());
        for (key,value) in [("model_api",json!(2)),("obs_len",json!(62)),("action_len",json!(12)),("action_scale",json!(0.5))] {
            let mut wrong=m.clone();wrong[key]=value;assert!(official_entry(&wrong,"policy.onnx").is_err());
        }
        let mut wrong=m.clone();wrong["command"]=json!({"encoding":"phase"});assert!(official_entry(&wrong,"policy.onnx").is_err());
        wrong=m.clone();wrong["duration_s"]=json!(2);assert!(official_preset("kick_left",&wrong).is_err());
        assert!(import_metadata(&json!({"metadata":{},"presetId":"walk-default"}),"walk",b"x").is_err());
    }
    #[test] fn explicit_selection_pairs_commands_and_preserves_rollback() {
        let mut r=defaults();validate(&r).unwrap();
        assert_eq!(slot_for("flamingo_hold").unwrap(),slot_for("flamingo_lower").unwrap());
        assert_eq!(slot_for("sitstand_sit").unwrap(),slot_for("sitstand_stand").unwrap());
        select_in(&mut r,"flamingo","flamingo-official").unwrap();
        assert_eq!(r["previous"]["flamingo"],"flamingo-right");
        assert!(select_in(&mut r,"walk","flamingo-official").is_err());
        assert_eq!(r["selected"]["walk"],"walk-default");
        select_in(&mut r,"flamingo","flamingo-right").unwrap();
        assert_eq!(r["previous"]["flamingo"],"flamingo-official");
    }
    #[test] fn registry_rejects_paths_duplicates_and_incomplete_selection() {
        for file in ["../evil.onnx","C:\\evil.onnx","/tmp/evil.onnx","a/b.onnx"] {
            let mut r=defaults();r["models"][0]["file"]=json!(file);assert!(validate(&r).is_err());
        }
        let mut r=defaults();r["models"][1]["id"]=r["models"][0]["id"].clone();assert!(validate(&r).is_err());
        let mut r=defaults();r["selected"]["walk"]=json!("flamingo-right");assert!(validate(&r).is_err());
        let mut r=defaults();r["selected"].as_object_mut().unwrap().remove("walk");assert!(validate(&r).is_err());
    }
    #[test] fn selection_survives_restart_and_missing_file_never_falls_back() {
        let temp=tempfile::tempdir().unwrap();let file=temp.path().join("registry.json");
        let mut r=defaults();select_in(&mut r,"flamingo","flamingo-official").unwrap();persist(&file,&r).unwrap();
        let loaded:Value=serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();assert_eq!(loaded,r);
        std::fs::write(temp.path().join("flamingo_right_hd1910.onnx"),b"unselected").unwrap();
        assert!(resolve_at(entry(&loaded,"flamingo-official").unwrap(),temp.path(),temp.path()).is_err());
    }
    #[test] fn mutation_requires_known_idle_state() {
        for state in ["policy","moving","preflight","disabling","calibrating",""] {assert!(!idle(state));}
        for state in ["idle","disabled","holding","failed"] {assert!(idle(state));}
    }
}

#[cfg(test)] mod roller_migration_tests {
    use super::*;
    #[test] fn existing_roller_choices_survive_additive_crouch_slot_migration() {
        let mut r=defaults();r["selected"].as_object_mut().unwrap().remove("roller_crouch");
        r["models"].as_array_mut().unwrap().retain(|m|m["slot"]!="roller_crouch");
        let previous=r.clone();add_swizzle_slot(&mut r).unwrap();validate(&r).unwrap();
        assert_eq!(r["selected"]["swizzle"],previous["selected"]["swizzle"]);
        assert_eq!(r["revision"],previous["revision"]);assert_eq!(r["previous"],previous["previous"]);
        assert!(r["selected"]["roller_crouch"].is_string());
        let migrated=r.clone();add_swizzle_slot(&mut r).unwrap();assert_eq!(r,migrated);
    }
    #[test] fn old_registry_gains_one_slot_without_replacing_user_choices() {
        let mut r=defaults();r["selected"].as_object_mut().unwrap().remove("swizzle");
        r["models"].as_array_mut().unwrap().retain(|m|m["slot"]!="swizzle");
        r["selected"]["flamingo"]=json!("flamingo-official");r["revision"]=json!(42);
        let before=r.clone();add_swizzle_slot(&mut r).unwrap();validate(&r).unwrap();
        assert_eq!(r["revision"],42);assert_eq!(r["selected"]["flamingo"],"flamingo-official");
        for (key,value) in before["selected"].as_object().unwrap(){assert_eq!(&r["selected"][key],value);}
        let migrated=r.clone();add_swizzle_slot(&mut r).unwrap();assert_eq!(migrated,r);
    }
}
