//! Android: finding, installing, starting and removing Craft Apps through the system's package
//! manager.
//!
//! Nothing here touches Java. The Android shell (`apps/artcraft-launcher-android`) implements
//! [`Bridge`] over JNI and registers it with [`set_bridge`] before the backend starts; everything
//! the backend does on Android goes through that one trait, so this crate stays free of `unsafe`
//! and builds, and is tested, on every platform.

use std::path::Path;
use std::sync::OnceLock;

use artcraft_launcher_core::version;
use artcraft_launcher_core::{CraftApp, InstallSource, Installed};

/// What the Android shell offers the backend.
pub trait Bridge: Send + Sync {
    /// The installed `versionName` of `package`, or `None` when it isn't installed.
    fn package_version(&self, package: &str) -> Result<Option<String>, String>;
    /// Whether the person has allowed this app to install packages ("Install unknown apps").
    fn can_install_packages(&self) -> bool;
    /// Sends the user to the system screen that grants [`Self::can_install_packages`].
    fn request_install_permission(&self) -> Result<(), String>;
    /// Hands `apk` to the system package installer and waits for its outcome. The system asks the
    /// person to confirm; `Err` carries what the installer said (including a dismissed prompt).
    fn install_apk(&self, apk: &Path) -> Result<(), String>;
    /// Asks the system to remove `package` (it shows its own confirmation).
    fn uninstall(&self, package: &str) -> Result<(), String>;
    /// Starts the launch activity of `package`.
    fn launch(&self, package: &str) -> Result<(), String>;
}

static BRIDGE: OnceLock<Box<dyn Bridge>> = OnceLock::new();

/// Registers the shell's bridge. Returns `false` when one is already registered.
pub fn set_bridge(bridge: Box<dyn Bridge>) -> bool {
    BRIDGE.set(bridge).is_ok()
}

fn bridge() -> Result<&'static dyn Bridge, String> {
    BRIDGE.get().map(|b| &**b).ok_or_else(|| "the Android package manager isn't available".to_string())
}

/// The installed copy of `app`, found by its package names, newest naming first.
pub fn detect(app: &CraftApp) -> Option<Installed> {
    let bridge = bridge().ok()?;
    app.bundle_ids().find_map(|package| match bridge.package_version(&package) {
        Ok(Some(name)) => Some(Installed {
            version: version::parse(&name),
            source: InstallSource::AndroidApp,
            location: package.clone(),
            package: Some(package),
            externally_managed: false,
            origin: None,
            installation: None,
        }),
        Ok(None) => None,
        Err(e) => {
            log::warn!("{}: couldn't ask the package manager about {package}: {e}", app.name);
            None
        }
    })
}

/// Installs the downloaded, checksum-verified `apk` of `app`.
pub fn install_apk(app: &CraftApp, apk: &Path, existing: Option<&Installed>) -> Result<(), String> {
    let _ = existing;
    let bridge = bridge()?;
    if !bridge.can_install_packages() {
        // Take the person to the switch; the download stays, so the next try skips the transfer.
        bridge.request_install_permission()?;
        return Err(format!("Allow ArtCraft Launcher to install apps, then try installing {} again.", app.name));
    }
    bridge.install_apk(apk)
}

/// Asks Android to remove the installed copy of `app`.
pub fn uninstall(app: &CraftApp, installed: &Installed) -> Result<(), String> {
    let package = installed.package.as_deref().unwrap_or(&installed.location);
    log::info!("{}: asking Android to remove {package}", app.name);
    bridge()?.uninstall(package)
}

/// Starts `app`.
pub fn launch(app: &CraftApp, installed: &Installed) -> Result<(), String> {
    let package = installed.package.as_deref().unwrap_or(&installed.location);
    log::info!("{}: starting {package}", app.name);
    bridge()?.launch(package)
}
