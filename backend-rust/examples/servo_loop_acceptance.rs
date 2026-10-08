//! Hardware acceptance executable: reuses production Bus, FrameClock and FeedbackCoast.
#[path="../src/calibration.rs"] mod calibration;
#[path="../src/control.rs"] mod control;
#[path="../src/guided.rs"] mod guided;
#[path="../src/imu.rs"] mod imu;
#[path="../src/orientation.rs"] mod orientation;
#[path="../src/policy.rs"] mod policy;
#[path="../src/servos.rs"] mod servos;
#[path="../src/telemetry.rs"] mod telemetry;
use anyhow::{bail,Result};
use serde_json::{json,Value};
use std::{collections::BTreeMap,time::{Duration,Instant}};
use telemetry::IDS;

fn run(bus:&mut servos::Bus, report:&mut Value, ids:&[u8])->Result<()> {
    let calibration:Value=serde_json::from_slice(&std::fs::read("/var/lib/microduck-observer/calibration.json")?)?;
    report["calibration"]=calibration.clone();
    let baseline=bus.read_feedback(&IDS)?;
    report["baseline"]=json!(baseline);
    for id in IDS {
        control::validate(id,baseline.get(&id),false)?;
        if baseline[&id]["torque"]!=0 {bail!("Expected all servos disabled before test");}
    }
    let mut centers=BTreeMap::new();
    let mut angles=BTreeMap::new();
    let mut configurations=BTreeMap::new();
    // Validate every joint before the first enabling/position write.
    for id in ids {
        let center=baseline[id]["position"].as_i64().unwrap() as i32;
        let key=id.to_string();
        let direction=calibration["joints"]["directions"][&key].as_f64().unwrap_or(-1.);
        let reference=calibration["joints"]["references"][&key].as_f64().ok_or_else(||anyhow::anyhow!("Missing calibration #{id}"))?;
        let angle=(center as f64-reference)*direction*std::f64::consts::TAU/4096.;
        let (low,high,cap)=control::configuration(bus,*id)?;
        if cap==0 || cap>1000 {bail!("Invalid output cap #{id}");}
        for delta in [-1f64,1.] {control::target(*id,center,&calibration,low,high,angle+delta.to_radians(),false)?;}
        centers.insert(*id,center);angles.insert(*id,angle);configurations.insert(*id,(low,high,cap));
    }
    report["centers"]=json!(centers);report["centerAnglesRad"]=json!(angles);
    report["configurations"]=json!(configurations);report["controlledIds"]=json!(ids);
    // Align all joints, set only necessary profile fields, enable once, then
    // issue the final goals last, exactly as the native standing path does.
    control::goals(bus,&centers)?;
    control::profiles(bus,ids,&baseline)?;
    for id in ids {
        if baseline[id]["torqueLimitRaw"]!=configurations[id].2 {
            let cap=configurations[id].2.to_le_bytes();
            bus.write_register(*id,48,&cap)?;
            if bus.read_register(*id,48,2)?!=cap {bail!("Output cap readback mismatch #{id}");}
        }
    }
    bus.sync_write(40,&ids.iter().map(|id|(*id,vec![1])).collect())?;
    let enabled=bus.read_feedback(&IDS)?;
    for id in IDS {control::validate(id,enabled.get(&id),ids.contains(&id))?;}
    report["enabled"]=json!(enabled);
    control::goals(bus,&centers)?;
    let mut coast=control::FeedbackCoast::default();
    coast.sample(&enabled,&IDS,Instant::now());
    let mut clock=control::FrameClock::new(Instant::now());
    let start=Instant::now();
    let mut phase=0usize;
    report["frames"]=json!([]);
    for round in 1..=3 {
        for index in 0..500 {
            if start.elapsed()>Duration::from_secs(40) {bail!("Acceptance deadline exceeded");}
            clock.wait();
            let feedback=bus.read_tick(&IDS)?;
            let diagnostic=bus.diagnostics.clone();
            for (id,row) in &feedback {control::validate(*id,Some(row),ids.contains(id))?;}
            let observation=coast.sample(&feedback,&IDS,Instant::now());
            let holding=observation.is_none();
            let goals=if holding {
                ids.iter().map(|id|(*id,coast.last.as_ref().unwrap()[id]["position"].as_i64().unwrap() as i32)).collect()
            } else {
                let delta=(std::f64::consts::TAU*phase as f64/50.).sin().to_radians();
                phase+=1;
                ids.iter().map(|id| {
                    let (low,high,_)=configurations[id];
                    Ok((*id,control::target(*id,centers[id],&calibration,low,high,angles[id]+delta,false)?))
                }).collect::<Result<BTreeMap<_,_>>>()?
            };
            let at=telemetry::monotonic();
            control::goals(bus,&goals)?;
            report["frames"].as_array_mut().unwrap().push(json!({"round":round,"index":index,"writeMono":at,"goals":goals,
                "holding":holding,"coasted":coast.misses>0&&!holding,"feedback":feedback,"diagnostics":diagnostic}));
        }
        println!("{}",json!({"round":round,"frames":500,"elapsedSeconds":start.elapsed().as_secs_f64()}));
    }
    report["skippedTicks"]=json!(clock.skipped);
    control::goals(bus,&centers)?;
    std::thread::sleep(Duration::from_millis(300));
    let restored=bus.read_feedback(&IDS)?;
    report["restore"]=json!(restored);
    for id in ids {
        control::validate(*id,restored.get(id),true)?;
        if (restored[id]["position"].as_i64().unwrap()-centers[id] as i64).abs() as f64>5.*4096./360. {bail!("Center restoration not confirmed #{id}");}
    }
    Ok(())
}
fn main()->Result<()> {
    let args:Vec<_>=std::env::args().collect();
    let ids:&[u8]=match args.get(1).map(String::as_str) {
        Some("--supported-single23")=>&[23],Some("--supported-all14")=>&IDS[..14],
        _=>bail!("Explicit supported-single23 or supported-all14 flag required"),
    };
    let output=args.get(2).ok_or_else(||anyhow::anyhow!("Output path required"))?;
    let mut bus=servos::Bus::open("/dev/ttyS2")?;
    let mut report=json!({"test":"native supported joints 50Hz +/-1deg, three 500-frame rounds, read then write"});
    let result=run(&mut bus,&mut report,ids);
    if let Err(error)=&result {report["error"]=json!(error.to_string());}
    // Always unload on completion or failure; no resumed old targets afterwards.
    match control::unload(&mut bus) {Ok(value)=>report["disable"]=value,Err(error)=>report["disableError"]=json!(error.to_string())};
    match bus.read_feedback(&IDS) {Ok(value)=>report["final"]=json!(value),Err(error)=>report["finalReadError"]=json!(error.to_string())};
    report["traceTail"]=json!(bus.trace);
    std::fs::write(output,serde_json::to_vec(&report)?)?;
    result?;
    if report["disable"]["state"]!="disabled" {bail!("Final disable not confirmed");}
    Ok(())
}
