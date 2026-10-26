//! Isolated sensor worker. Observer owns its lifetime; no listener or service.
#[cfg(target_os="linux")]
mod native {
    use anyhow::{ensure,Result};
    use std::{ffi::CString,io::Write,os::fd::AsRawFd,sync::atomic::{AtomicBool,Ordering},time::{Duration,Instant}};
    static STOP:AtomicBool=AtomicBool::new(false);
    extern "C" fn stop(_:libc::c_int) {STOP.store(true,Ordering::Relaxed);}
    unsafe extern "C" {
        fn vl5_open(path:*const libc::c_char,addr:u8)->i32;
        fn vl5_close(); fn vl5_is_alive()->i32; fn vl5_init()->i32;
        fn vl5_start(hz:u8)->i32;fn vl5_stop()->i32;
        fn vl5_data_ready()->i32;fn vl5_get_frame(distance:*mut i16,status:*mut u8)->i32;
    }
    struct Sensor {started:bool}
    impl Drop for Sensor {fn drop(&mut self) {unsafe {
        if self.started && vl5_stop()!=0 {eprintln!("ToF stop-ranging failed");}
        vl5_close();
    }}}
    pub fn run()->Result<()> {
        let path=std::env::args().nth(1).unwrap_or("/dev/i2c-3".into());
        ensure!(path.strip_prefix("/dev/i2c-").is_some_and(|s|!s.is_empty() && s.bytes().all(|b|b.is_ascii_digit())),"Invalid I2C device");
        let lock=std::fs::OpenOptions::new().read(true).write(true).open(&path)?;
        // Advisory per-device exclusivity among Observer workers; codec uses its own address.
        ensure!(unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0,"ToF device already owned");
        unsafe {libc::signal(libc::SIGTERM,stop as *const () as libc::sighandler_t);libc::signal(libc::SIGINT,stop as *const () as libc::sighandler_t);}
        let c=CString::new(path)?;
        // One worker, one static ULD instance. Arrays below are exactly 64 entries.
        ensure!(unsafe{vl5_open(c.as_ptr(),0x29)}==0,"ToF open failed");
        let mut sensor=Sensor{started:false};
        ensure!(unsafe{vl5_is_alive()}==1,"VL53L5CX not found at 0x29");
        ensure!(unsafe{vl5_init()}==0,"ToF firmware initialization failed");
        if STOP.load(Ordering::Relaxed) {return Ok(());}
        ensure!(unsafe{vl5_start(15)}==0,"ToF start-ranging failed");sensor.started=true;
        let clock=Instant::now();let mut last=clock;let mut seq=0u64;
        let mut out=std::io::BufWriter::new(std::io::stdout());
        while !STOP.load(Ordering::Relaxed) {
            let ready=unsafe{vl5_data_ready()};ensure!(ready>=0,"ToF data-ready I2C failure");
            if ready==1 {
                let mut distances=[0i16;64];let mut status=[0u8;64];
                ensure!(unsafe{vl5_get_frame(distances.as_mut_ptr(),status.as_mut_ptr())}==0,"ToF frame I2C failure");
                for i in 0..64 {if distances[i]<0 {distances[i]=0;status[i]=0;}}
                seq+=1;last=Instant::now();
                let mut captured:libc::timespec=unsafe{std::mem::zeroed()};
                ensure!(unsafe{libc::clock_gettime(libc::CLOCK_MONOTONIC,&mut captured)}==0,"ToF capture clock unavailable");
                let captured_ns=captured.tv_sec as u64*1_000_000_000+captured.tv_nsec as u64;
                let frame=serde_json::json!({"captured_mono_ns":captured_ns,"rows":8,"cols":8,"seq":seq,"t_ns":clock.elapsed().as_nanos() as u64,"distance_mm":distances.as_slice(),"status":status.as_slice()});
                serde_json::to_writer(&mut out,&frame)?;writeln!(out)?;out.flush()?;
            }
            ensure!(last.elapsed()<Duration::from_secs(3),"ToF stopped producing frames");
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}
fn main() {
    #[cfg(target_os="linux")]
    if let Err(e)=native::run(){eprintln!("{e:#}");std::process::exit(1);}
    #[cfg(not(target_os="linux"))]
    {eprintln!("ToF hardware requires Linux");std::process::exit(1);}
}
