//! Select the device-backed code for this package: `gpu` with the `wgpu` device.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(gpu)");
    if std::env::var_os("CARGO_FEATURE_WGPU").is_some() {
        println!("cargo:rustc-cfg=gpu");
    }
}
