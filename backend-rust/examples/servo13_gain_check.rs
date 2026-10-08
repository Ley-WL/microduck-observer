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

fn run(bus:&mut servos::Bus, report:&mut Value, home_check:bool)->Result<()> {
    let cal:Value=serde_json::from_slice(&std::fs::read("/var/lib/microduck-observer/calibration.json")?)?;
    let before=bus.read_feedback(&IDS)?;report["before"]=json!(before);report["calibration"]=cal.clone();
    for id in IDS {control::validate(id,before.get(&id),false)?;if before[&id]["torque"]!=0 {bail!("Expected all disabled");}}
    let id=13u8;let center=before[&id]["position"].as_i64().unwrap() as i32;
    let direction=cal["joints"]["directions"]["13"].as_f64().unwrap_or(-1.);
    let reference=cal["joints"]["references"]["13"].as_f64().ok_or_else(||anyhow::anyhow!("Missing #13 calibration"))?;
    let angle=(center as f64-reference)*direction*std::f64::consts::TAU/4096.;
    let (low,high,cap)=control::configuration(bus,id)?;
    let goals:Vec<_>=[-3f64,3.,0.].into_iter().map(|delta|control::target(id,center,&cal,low,high,angle+delta.to_radians(),false)).collect::<Result<_>>()?;
    let goals=if home_check {vec![control::target(id,center,&cal,low,high,policy::home_targets_for("stand")?[&13],false)?,center]} else {goals};
    report["homeCheck"]=json!(home_check);
    report["center"]=json!(center);report["plannedTargets"]=json!(goals);report["hardwareLimits"]=json!([low,high,cap]);
    let result=(||->Result<()> {
        control::goals(bus,&BTreeMap::from([(id,center)]))?;
        control::profiles(bus,&[id],&before)?;
        bus.sync_write(40,&BTreeMap::from([(id,vec![1])]))?;
        let enabled=bus.read_feedback(&IDS)?;control::validate(id,enabled.get(&id),true)?;
        control::goals(bus,&BTreeMap::from([(id,center)]))?;
        report["frames"]=json!([]);
        let mut start_goal=center;
        for (phase,kp) in [6u8,12,6].into_iter().enumerate() {
            bus.write_register(id,50,&[kp])?;if bus.read_register(id,50,1)?!=[kp] {bail!("P gain not confirmed");}
            for (segment,target) in goals.iter().enumerate() {
                let began=Instant::now();let mut clock=control::FrameClock::new(began);
                for tick in 0..150 {
                    if began.elapsed()>Duration::from_secs(4) {bail!("Segment timing expired");}
                    let feedback=bus.read_tick(&IDS)?;
                    for sid in IDS {control::validate(sid,feedback.get(&sid),sid==id)?;if sid!=id && feedback[&sid]["torque"]!=0 {bail!("Other joint enabled");}}
                    let ratio=(began.elapsed().as_secs_f64()/1.).min(1.);
                    let goal=(start_goal as f64+(*target-start_goal) as f64*ratio).round_ties_even() as i32;
                    let at=telemetry::monotonic();control::goals(bus,&BTreeMap::from([(id,goal)]))?;
                    report["frames"].as_array_mut().unwrap().push(json!({"phase":phase,"kp":kp,"segment":segment,"tick":tick,"elapsed":began.elapsed().as_secs_f64(),"target":target,"commanded":goal,"writeMono":at,"feedback":feedback[&id],"diagnostics":bus.diagnostics}));
                    clock.wait();
                }
                start_goal=*target;
            }
            println!("{}",json!({"phase":phase,"kp":kp,"completed":true}));
        }
        Ok(())
    })();
    let disabled=bus.sync_write(40,&BTreeMap::from([(id,vec![0])]));report["disableWriteOk"]=json!(disabled.is_ok());
    // Restore original SRAM profile while disabled; no EEPROM writes.
    for (address,key) in [(41,"accelerationRaw"),(44,"goalCurrentRaw"),(46,"speedLimitRaw"),(50,"kpRaw"),(51,"kdRaw")] {
        let value=before[&id][key].as_u64().ok_or_else(||anyhow::anyhow!("Missing original profile"))?;
        let bytes=if address==44||address==46 {(value as u16).to_le_bytes().to_vec()}else{vec![value as u8]};
        bus.write_register(id,address,&bytes)?;
        if bus.read_register(id,address,bytes.len() as u8)?!=bytes {bail!("Restore not confirmed #{id} address {address}");}
    }
    let after=bus.read_feedback(&IDS)?;report["after"]=json!(after);
    for sid in IDS {control::validate(sid,after.get(&sid),false)?;if after[&sid]["torque"]!=0 {bail!("Final disable not confirmed");}}
    result
}
fn main()->Result<()> {
    let args:Vec<_>=std::env::args().collect();if args.get(1).map(String::as_str)!=Some("--supported-single13") {bail!("Explicit support flag required");}
    let output=args.get(2).ok_or_else(||anyhow::anyhow!("Output path required"))?;
    let mut bus=servos::Bus::open("/dev/ttyS2")?;let mut report=json!({"test":"#13 supported +/-3deg 50Hz, temporary P6/P12/P6, D20, return center and restore SRAM", "controlledIds":[13]});
    let result=run(&mut bus,&mut report,args.iter().any(|arg|arg=="--home"));
    if let Err(e)=&result {report["error"]=json!(e.to_string());let _=bus.sync_write(40,&BTreeMap::from([(13,vec![0])]));}
    report["traceTail"]=json!(bus.trace);std::fs::write(output,serde_json::to_vec(&report)?)?;result
}
