//! ArtCraft Launcher for Android.
//!
//! The same egui interface and backend as the desktop launcher, running in a `NativeActivity`.
//! What differs is only where the platform comes in: apps are found, installed, started and
//! removed through the system package manager (`jni_bridge`, the Java side is
//! `android/java/.../PackageHelper.java`), and there is no tray, window chrome or folder picker.
//!
//! The crate is empty on other targets so a whole-workspace build on a desktop still works.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![cfg(target_os = "android")]

mod jni_bridge;
mod shell;

use android_activity::AndroidApp;

/// Where Android starts the app (`android.app.lib_name` in the manifest names this library).
#[allow(unsafe_code)] // `android_main` must be exported by name for android-activity's glue.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("ArtCraftLauncher"));
    log::info!("ArtCraft Launcher {} starting", env!("CARGO_PKG_VERSION"));
    if let Err(e) = shell::run(app) {
        log::error!("ArtCraft Launcher stopped: {e}");
    }
}
