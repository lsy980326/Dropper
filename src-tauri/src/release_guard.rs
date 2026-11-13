
#[cfg(all(feature = "desktop", dev, not(debug_assertions)))]
compile_error!("Desktop release builds must embed frontend assets; use the Tauri build command.");
