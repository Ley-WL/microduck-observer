use crate::telemetry::{monotonic, Shared};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

fn cpu_ticks(text: &str) -> Option<(u64, u64)> {
    let mut fields = text.lines().next()?.split_whitespace();
    if fields.next()? != "cpu" { return None; }
    // Guest time is already included in user/nice; count only the first 8 fields.
    let values: Vec<u64> = fields.take(8).map(str::parse).collect::<Result<_, _>>().ok()?;
    if values.len() < 4 { return None; }
    let total = values.iter().try_fold(0u64, |a,b| a.checked_add(*b))?;
    Some((total, values[3].checked_add(*values.get(4).unwrap_or(&0))?))
}
fn cpu_usage(before: (u64,u64), after: (u64,u64)) -> Option<f64> {
    let total = after.0.checked_sub(before.0)?;
    let idle = after.1.checked_sub(before.1)?;
    if total == 0 || idle > total { return None; }
    Some((total-idle) as f64 * 100. / total as f64)
}
fn memory(text: &str) -> Option<Value> {
    let get = |name: &str| text.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        if words.next()? != name { return None; }
        let kb: u64 = words.next()?.parse().ok()?;
        if words.next()? != "kB" { return None; }
        kb.checked_mul(1024)
    });
    let total = get("MemTotal:")?;
    let available = get("MemAvailable:")?;
    let used = total.checked_sub(available)?;
    if total == 0 { return None; }
    Some(json!({"totalBytes":total,"availableBytes":available,"usedBytes":used,
        "usagePercent":used as f64 * 100. / total as f64}))
}
fn temperature(root: &Path) -> Option<Value> {
    let mut sensors: Vec<_> = fs::read_dir(root).ok()?.filter_map(Result::ok).filter_map(|entry| {
        let kind = fs::read_to_string(entry.path().join("type")).ok()?.trim().to_owned();
        let lower = kind.to_lowercase();
        if !lower.contains("cpu") && !lower.contains("soc") { return None; }
        let c: f64 = fs::read_to_string(entry.path().join("temp")).ok()?.trim().parse::<f64>().ok()? / 1000.;
        if !c.is_finite() { return None; }
        Some((if lower.contains("soc") {0} else {1}, kind, c))
    }).collect();
    sensors.sort_by(|a,b|a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    sensors.first().map(|(_,kind,c)|json!({"celsius":c,"sensor":kind}))
}
fn cpu_frequency(root: &Path) -> Option<Value> {
    let mut cores: Vec<_> = fs::read_dir(root).ok()?.filter_map(Result::ok).filter_map(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        let index: u32 = name.strip_prefix("cpu")?.parse().ok()?;
        let dir = entry.path().join("cpufreq");
        let khz: u64 = fs::read_to_string(dir.join("scaling_cur_freq")).ok()?.trim().parse().ok()?;
        if khz == 0 { return None; }
        let governor = fs::read_to_string(dir.join("scaling_governor")).ok().map(|s| s.trim().to_owned());
        let max_khz = fs::read_to_string(dir.join("scaling_max_freq")).ok().and_then(|s| s.trim().parse::<u64>().ok());
        Some((index, json!({"core":index,"mhz":khz as f64/1000.,"governor":governor,"maxMHz":max_khz.map(|v|v as f64/1000.)})))
    }).collect();
    cores.sort_by_key(|(index,_)| *index);
    if cores.is_empty() { return None; }
    let governor = cores.first().and_then(|(_,v)| v["governor"].as_str()).map(str::to_owned);
    Some(json!({"cores":cores.into_iter().map(|(_,v)|v).collect::<Vec<_>>(),"governor":governor}))
}
pub fn spawn(shared: Shared, stop: Arc<AtomicBool>) {
    std::thread::Builder::new().name("board-resources".into()).spawn(move || {
        let board = board_info();
        let mut previous = None;
        while !stop.load(Ordering::Acquire) {
            // File reads happen outside the telemetry lock and outside the UART loop.
            let ticks = fs::read_to_string("/proc/stat").ok().and_then(|s|cpu_ticks(&s));
            let usage = previous.zip(ticks).and_then(|(a,b)|cpu_usage(a,b));
            previous = ticks;
            let mem = fs::read_to_string("/proc/meminfo").ok().and_then(|s|memory(&s));
            let temp = temperature(Path::new("/sys/class/thermal"));
            let frequency = cpu_frequency(Path::new("/sys/devices/system/cpu"));
            let uptime = fs::read_to_string("/proc/uptime").ok().and_then(|v|v.split_whitespace().next()?.parse::<f64>().ok());
            shared.write().unwrap().resources = json!({"cpuUsagePercent":usage,"cpuFrequency":frequency,"memory":mem,
                "temperature":temp,"sampleMonoMs":monotonic()*1000.,"intervalMs":2000,
                "board":board,"boardUptimeSeconds":uptime});
            std::thread::sleep(Duration::from_secs(2));
        }
    }).expect("board resource monitor");
}
fn board_info() -> Value {
    let read = |path: &str| fs::read_to_string(path).ok().map(|s|s.trim_matches(['\0','\n','\r',' ']).to_owned());
    let os = fs::read_to_string("/etc/os-release").ok().and_then(|s|s.lines().find_map(|line|line.strip_prefix("PRETTY_NAME=").map(|s|s.trim_matches('"').to_owned())));
    let release = std::env::current_exe().ok().and_then(|p| {
        let parent = p.parent()?;
        if parent.file_name()? != "bin" { return None; }
        Some(parent.parent()?.file_name()?.to_string_lossy().into_owned())
    });
    json!({"model":read("/proc/device-tree/model"),"hostname":read("/etc/hostname"),
        "os":os,"kernel":read("/proc/sys/kernel/osrelease"),"architecture":std::env::consts::ARCH,
        "serviceVersion":env!("CARGO_PKG_VERSION"),"release":release})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_uses_interval_delta_and_does_not_double_count_guest() {
        assert_eq!(cpu_ticks("cpu 100 20 30 400 50 6 7 8 80 10\ncpu0 0"),Some((621,450)));
        assert_eq!(cpu_usage((100,60),(200,80)),Some(80.));
        assert_eq!(cpu_usage((100,60),(100,60)),None);
        assert_eq!(cpu_usage((200,80),(100,60)),None);
        assert_eq!(cpu_ticks("cpu bad 0 0 0"),None);
    }
    #[test]
    fn memory_excludes_reclaimable_cache_using_available() {
        let m=memory("MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 700 kB\n").unwrap();
        assert_eq!(m["usedBytes"],300*1024);
        assert_eq!(m["usagePercent"],30.);
        assert!(memory("MemTotal: 100 kB\nMemAvailable: 200 kB").is_none());
        assert!(memory("MemTotal: 100 kB").is_none());
    }
    #[test]
    fn reads_each_cpu_frequency_and_governor_from_sysfs() {
        let root=std::env::temp_dir().join(format!("microduck-cpufreq-{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        for (cpu,freq) in [(0,"816000"),(1,"408000")] {
            let dir=root.join(format!("cpu{cpu}/cpufreq")); fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("scaling_cur_freq"),freq).unwrap();
            fs::write(dir.join("scaling_governor"),"ondemand\n").unwrap();
            fs::write(dir.join("scaling_max_freq"),"1608000\n").unwrap();
        }
        let value=cpu_frequency(&root).unwrap();
        assert_eq!(value["governor"],"ondemand");
        assert_eq!(value["cores"][0]["mhz"],816.);
        assert_eq!(value["cores"][1]["mhz"],408.);
        assert_eq!(value["cores"][0]["maxMHz"],1608.);
        fs::remove_dir_all(root).unwrap();
    }
}
