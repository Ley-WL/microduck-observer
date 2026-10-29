//! Tunable defaults for autonomous exploration. All durations are seconds, angles degrees.
use serde_json::{json,Value};
use super::behavior::Activity;
pub struct BehaviorParameters {
    /// Sit, roll, dance, look, explore. Must total 100.
    pub weights:[u8;5], pub sit_seconds:[f64;2], pub look_seconds:[f64;2],
    pub look_yaw:f64, pub look_period:f64, pub response_period:f64,
    /// Pitch (#31), yaw (#32), roll (#33); no neck (#30) override.
    pub response_degrees:[f64;3], pub transition_timeout_seconds:f64,
}
pub const BEHAVIOR:BehaviorParameters=BehaviorParameters {
    weights:[10,10,10,30,40],sit_seconds:[5.,10.],look_seconds:[15.,30.],
    look_yaw:45.,look_period:8.,response_period:2.,response_degrees:[6.,12.,5.],
    transition_timeout_seconds:20.,
};
impl BehaviorParameters {
    pub fn status(&self)->Value {json!({"weights":self.weights,"sitSeconds":self.sit_seconds,"lookSeconds":self.look_seconds,"lookYawDegrees":self.look_yaw,"lookPeriodSeconds":self.look_period,"responsePeriodSeconds":self.response_period,"responseDegrees":self.response_degrees,"transitionTimeoutSeconds":self.transition_timeout_seconds})}
    pub fn choose(&self,draw:u8)->Activity {
        let mut total=0u16;
        for (weight,activity) in self.weights.iter().zip([Activity::Sit,Activity::Roll,Activity::Dance,Activity::Look,Activity::Explore]) {
            total+=*weight as u16;if (draw as u16)<total{return activity;}
        }
        Activity::Explore
    }
}
pub const STOP_MM:f64=450.;
pub const CLEAR_MM:f64=700.;
pub const FORWARD_SPEED:f32=0.2;
pub const TURN_SPEED:f32=1.;
pub const TURN_MIN_DEGREES:f64=30.;
pub const TURN_MAX_DEGREES:f64=90.;
pub const TURN_TIMEOUT_SECONDS:f64=10.;
pub const SCAN_ANGLES:[f64;3]=[30.,60.,90.];
pub const LOOK_UP:f64=-15.;
pub const FORWARD_SECONDS:[f64;2]=[3.,10.];
pub const CHATTER_SECONDS:[u64;2]=[3,7];

pub const BLOCKED_REMINDER_SECONDS:f64=10.;

pub const OBSTACLE_CONFIRM:std::time::Duration=std::time::Duration::from_millis(500);
pub const MAX_TURN_ATTEMPTS:u8=3;
pub const RETREAT_SECONDS:[f64;2]=[0.4,0.7];
pub const SCAN_ALIGNMENT_DEGREES:f64=3.;
pub const MOTION_ALIGNMENT_DEGREES:f64=12.;
pub const SCAN_TIMEOUT_SECONDS:f64=6.;
pub const SCAN_SETTLE:std::time::Duration=std::time::Duration::from_millis(350);
pub const SCAN_SAMPLES:u8=3;

/// Forward command reaches full speed after this wall-clock interval. Stops remain immediate.
pub const FORWARD_RAMP_SECONDS:f64=0.5;

pub const U_TURN_DEGREES:f64=180.;
