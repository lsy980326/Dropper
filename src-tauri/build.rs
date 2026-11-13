fn main() {
    println!("cargo:rustc-check-cfg=cfg(dev)");
    #[cfg(feature = "desktop")]
    tauri_build::build()
}
