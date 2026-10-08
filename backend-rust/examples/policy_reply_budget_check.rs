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
use serde_json::json;
use std::time::Instant;
use telemetry::IDS;
fn main()->Result<()> {
    let output=std::env::args().nth(1).ok_or_else(||anyhow::anyhow!("report path required"))?;
    let mut bus=servos::Bus::open("/dev/ttyS2")?;
    let before=bus.read_feedback(&IDS)?;
    for id in IDS { control::validate(id,before.get(&id),false)?;if before[&id]["torque"]!=0 { bail!("Expected all disabled"); } }
    let start=Instant::now();let mut clock=control::FrameClock::with_period(start,std::time::Duration::from_millis(10));
    let mut frames=vec![];
    for n in 0..1000 {
        let at=Instant::now();let mode=if n%2==0 {"policy6"} else {"normal30"};
        let feedback=if n%2==0 {bus.read_policy_tick(&IDS)?} else {bus.read_tick(&IDS)?};
        for row in feedback.values() {if row["torque"]!=0 {bail!("Unexpected torque enable");}}
        frames.push(json!({"mode":mode,"elapsedMs":at.duration_since(start).as_secs_f64()*1000.,"workMs":at.elapsed().as_secs_f64()*1000.,"diagnostics":bus.diagnostics}));clock.wait();
    }
    let after=bus.read_feedback(&IDS)?;
    std::fs::write(output,serde_json::to_vec(&json!({"before":before,"after":after,"frames":frames,"durationSeconds":start.elapsed().as_secs_f64(),"skippedTicks":clock.skipped,"hardwareWrites":0}))?)?;
    Ok(())
}
