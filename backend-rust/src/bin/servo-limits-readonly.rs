//! Run only with production stopped and all fifteen servos confirmed unloaded.
//! Uses the shared parser and only READ/configuration/feedback requests.
#[path = "../servos.rs"]
mod servos;
mod telemetry {
    pub const IDS:[u8;15]=[10,11,12,13,14,20,21,22,23,24,30,31,32,33,34];
    pub fn monotonic()->f64 {
        static START:std::sync::OnceLock<std::time::Instant>=std::sync::OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
    }
}
use anyhow::{bail,Result};
use serde_json::{json,Value};

fn parse_configuration(id:u8,c:&[u8])->Result<Value> {
    if c.len()!=40 || c[..2]!=[3,46] || c[33]!=4 {bail!("#{id} unverified firmware/mode");}
    let word=|n|u16::from_le_bytes([c[n],c[n+1]]);
    Ok(json!({"id":id,"firmware":[c[0],c[1]],"mode":c[33],"minimumEncoder":word(9),"maximumEncoder":word(11),"maximumOutput":word(16),"configurationBytes":c}))
}
fn main()->Result<()> {
    let port=std::env::args().nth(1).ok_or_else(||anyhow::anyhow!("serial path required"))?;
    let mut bus=servos::Bus::open(&port)?;
    let before=bus.read_feedback(&telemetry::IDS)?;
    if before.len()!=15 || before.values().any(|r|r["torque"]!=0 || r["fault"]!=0) {bail!("requires 15 online fault-free unloaded servos; no configuration requested");}
    let mut configs=Vec::new();
    for id in telemetry::IDS {configs.push(parse_configuration(id,&bus.read_configuration(id)?)?);}
    let after=bus.read_feedback(&telemetry::IDS)?;
    if after.len()!=15 || after.values().any(|r|r["torque"]!=0 || r["fault"]!=0) {bail!("final unloaded feedback not confirmed");}
    println!("{}",json!({"readOnly":true,"servoRegisterWrites":false,"eepromWrites":false,"configurationReadAddress":0,"configurationReadLength":40,"instructionsUsed":[2,130],"configs":configs,"torqueBefore":before.iter().map(|(i,r)|(i.to_string(),r["torque"].clone())).collect::<std::collections::BTreeMap<_,_>>(),"torqueAfter":after.iter().map(|(i,r)|(i.to_string(),r["torque"].clone())).collect::<std::collections::BTreeMap<_,_>>()}));
    Ok(())
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn config_parser_checks_mode_and_decodes_only_read_fields(){
        let mut c=[0u8;40];c[0]=3;c[1]=46;c[33]=4;c[9..11].copy_from_slice(&10u16.to_le_bytes());c[11..13].copy_from_slice(&4090u16.to_le_bytes());
        let v=parse_configuration(20,&c).unwrap();assert_eq!(v["minimumEncoder"],10);assert_eq!(v["maximumEncoder"],4090);c[33]=0;assert!(parse_configuration(20,&c).is_err());
    }
}
