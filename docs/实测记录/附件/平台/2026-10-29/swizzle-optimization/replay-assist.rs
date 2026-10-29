type Result<T> = std::result::Result<T, String>;
macro_rules! ensure { ($cond:expr, $message:expr) => { if !$cond { return Err($message.into()); } }; }
const STATIONARY_TURN_RATE:f64=0.9;
const GLIDING_TURN_GAIN:f64=2.;
const DRIVE_LIMIT:f64=0.4;
const DRIVE_SLEW:f64=0.8; // zero to full drive in 0.5 s
const TURN_GLIDE_START:f64=0.15_f32 as f64; // match the f32 wire boundary exactly
const TURN_RATE_BLEND_START:f64=0.2;
const STALL_ERROR:f64=0.4;
const STALL_YAW_RATE:f64=0.08;
const STALL_DWELL:f64=0.5;
const TURN_ASSIST_SECONDS:f64=1.2;
const TURN_ASSIST_SETTLE:f64=0.2;
fn smoothstep(value:f64)->f64 {let x=value.clamp(0.,1.);x*x*(3.-2.*x)}
fn turning_throttle(throttle:f64)->f64 {
    throttle.signum()*DRIVE_LIMIT*smoothstep((throttle.abs()-TURN_GLIDE_START)/(DRIVE_LIMIT-TURN_GLIDE_START))
}
fn heading_rate(throttle:f64,turn:f64)->f64 {
    if turn.abs()<=1e-6 {return 0.;}
    let blend=smoothstep((throttle.abs()-TURN_RATE_BLEND_START)/(DRIVE_LIMIT-TURN_RATE_BLEND_START));
    turn.signum()*(STATIONARY_TURN_RATE*(1.-blend)+turn.abs()*GLIDING_TURN_GAIN*blend)
}
/// Low drive plus steering prioritizes turning; larger drive blends into gliding.
/// The actor still receives +/-0.5 rad heading error, never a yaw-rate target.
#[derive(Default)]
pub struct Heading {
    pub target: Option<f64>,
    turning: bool,
    stale: bool,
    drive: f64,
    last_yaw: Option<f64>,
    last_turn: f64,
    stalled: f64,
    assist_remaining: f64,
}
fn wrap(angle: f64) -> f64 {
    (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}
impl Heading {
    pub fn assist_remaining_seconds(&self)->f64 {self.assist_remaining}
    pub fn step(&mut self, yaw: f64, input: [f32;3], fresh: bool, dt: f64) -> Result<[f32;3]> {
        ensure!(yaw.is_finite() && dt.is_finite() && dt>=0. && input.iter().all(|v|v.is_finite()), "轮滑航向输入无效");
        let turn=if fresh {input[2].clamp(-0.25,0.25) as f64}else{0.};
        let turning=turn.abs()>1e-6;
        let dt=dt.min(0.05);
        let changed=!turning || self.last_turn*turn<0.;
        if changed {self.assist_remaining=0.;}
        let still=dt>0. && self.last_yaw.is_some_and(|last|wrap(yaw-last).abs()<STALL_YAW_RATE*dt);
        let error=self.target.map_or(0.,|target|wrap(target-yaw).abs());
        self.stalled=if turning && still && error>=STALL_ERROR && self.assist_remaining<=0. && !changed {
            self.stalled+dt
        }else{0.};
        if self.stalled>=STALL_DWELL {
            self.assist_remaining=TURN_ASSIST_SECONDS;
            self.target=Some(yaw);
            self.stalled=0.;
        }
        self.last_yaw=Some(yaw);self.last_turn=turn;
        let assisting=self.assist_remaining>0.;
        let settling=self.assist_remaining>TURN_ASSIST_SECONDS-TURN_ASSIST_SETTLE;
        let input_throttle=if fresh {input[0].clamp(-0.4,0.4) as f64}else{0.};
        let desired=if assisting {0.}else if turning {turning_throttle(input_throttle)}else{input_throttle};
        // Release/stale drive is zero immediately; fresh acceleration and reversal
        // are bounded. A stalled turn briefly removes drive and retries from yaw.
        self.drive=if desired==0. {0.}else {
            self.drive+(desired-self.drive).clamp(-DRIVE_SLEW*dt,DRIVE_SLEW*dt)
        };
        let rate=if settling {0.}else if assisting {turn.signum()*STATIONARY_TURN_RATE}else{heading_rate(self.drive,turn)};
        if settling {self.target=Some(yaw);}
        self.assist_remaining=(self.assist_remaining-dt).max(0.);
        let target=self.target.get_or_insert(yaw);
        // Releasing a turn holds the current bearing, without chasing a residual
        // target. Stale input latches only once so subsequent drift is corrected.
        if (self.turning && !turning) || (!fresh && !self.stale) {*target=yaw;}
        *target=wrap(*target+rate*dt);
        let error=wrap(*target-yaw).clamp(-0.5,0.5);
        *target=wrap(yaw+error); // anti-windup during a blocked/slow turn
        self.turning=turning;self.stale=!fresh;
        Ok([self.drive as f32,0.,error as f32])
    }
}
fn main() {
    let csv=std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let mut heading=Heading::default();let mut n=0;let mut max_drive=0f64;let mut max_error=0f64;
    for line in csv.lines() {
        let row:Vec<f64>=line.split(',').map(|v|v.parse().unwrap()).collect();
        if row[0]==1. {heading=Heading::default();}
        let out=heading.step(row[1],[row[2] as f32,0.,row[3] as f32],true,0.02).unwrap();
        max_drive=max_drive.max((out[0] as f64-row[4]).abs());
        max_error=max_error.max((out[2] as f64-row[5]).abs());n+=1;
    }
    println!("samples={n}, max_drive_error={max_drive}, max_heading_error_rad={max_error}");
    assert!(max_drive<1e-6 && max_error<2e-4);
}
