fn main() {
    println!("cargo:rerun-if-changed=vendor/tof");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref()!=Ok("linux") {return;}
    cc::Build::new().include("vendor/tof/vl53l5cx")
        .define("TOF_PLATFORM","VL53L5CX_Platform")
        .file("vendor/tof/vl53l5cx/vl53l5cx_api.c")
        .file("vendor/tof/vl53l5cx/shim.c").file("vendor/tof/platform.c")
        .warnings(false).opt_level(2).compile("observer_tof");
}
