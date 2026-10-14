#[path="/mnt/d/Users/wl/Desktop/MicroDuck/microduck-observer/backend-rust/src/roll_exit.rs"] mod roll_exit;
use std::io::{self,BufRead};
fn main(){let mut guard=roll_exit::RollExit::default();let mut name=String::new();let mut ended=false;let mut t=0.;
for line in io::stdin().lock().lines(){let line=line.unwrap();let p:Vec<_>=line.split('\t').collect();if name!=p[0]{if !name.is_empty() && !ended{println!("{}\tincomplete-record\t{}\t{}",name,t,guard.rotation_degrees);}guard.reset();ended=false;name=p[0].into();}if ended{continue;}
t=p[1].parse().unwrap();let obs:Vec<f32>=p[2..].iter().map(|v|v.parse().unwrap()).collect();guard.sample(&obs);
if guard.complete(t){println!("{}\tcomplete\t{}\t{}",name,t,guard.rotation_degrees);ended=true;}
else if t>=roll_exit::RollExit::TIMEOUT-1e-9{println!("{}\ttimeout\t{}\t{}",name,t,guard.rotation_degrees);ended=true;}
}if !name.is_empty() && !ended{println!("{}\tincomplete-record\t{}\t{}",name,t,guard.rotation_degrees);}}