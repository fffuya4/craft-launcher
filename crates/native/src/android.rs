//! Android: finding, installing, starting and removing Craft Apps through the system's package
//! manager.
//!
//! Nothing here touches Java. The Android shell (`apps/artcraft-launcher-android`) implements
//! [`Bridge`] over JNI and registers it with [`set_bridge`] before the backend starts; everything
//! the backend does on Android goes through that one trait, so this crate stays free of `unsafe`
//! and builds, and is tested, on every platform.

use std::path::Path;
use std::sync::OnceLock;

use artcraft_launcher_core::release::Release;
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
    /// Asks the system to remove `package`: shows its confirmation and waits for the answer, so
    /// `Err` also carries a dismissed prompt.
    fn uninstall(&self, package: &str) -> Result<(), String>;
    /// Starts the launch activity of `package`.
    fn launch(&self, package: &str) -> Result<(), String>;
    /// The system bars' and cut-outs' size in pixels (top, bottom, left, right).
    fn insets_px(&self) -> [f32; 4] {
        [0.0; 4]
    }
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

/// Where this Android port's own releases are published. Its updates are checked there, apart from
/// the original launcher's (`artcraft_launcher_core::catalog::LAUNCHER_REPO`).
pub const PORT_REPO: &str = "fffuya4/craft-launcher-android";

/// Where to tell the port's developer that the original launcher has moved on.
pub const PORT_ISSUES_URL: &str = "https://github.com/fffuya4/craft-launcher-android/issues/new";

/// The port's own revision: it counts up with each port release made on the same original
/// version (`android/PORT_REVISION`, read when the app is built).
pub fn port_revision() -> u32 {
    include_str!("../../../android/PORT_REVISION").trim().parse().unwrap_or(1)
}

/// The tag a port release carries: `android-v<original version>-r<revision>`.
pub fn port_tag(base: &str, revision: u32) -> String {
    format!("android-v{base}-r{revision}")
}

/// The original version and revision a port release tag names.
pub fn parse_port_tag(tag: &str) -> Option<(version::Version, u32)> {
    let rest = tag.strip_prefix("android-v")?;
    let (base, revision) = rest.rsplit_once("-r")?;
    Some((version::parse(base)?, revision.parse().ok()?))
}

/// Whether the port release `tag` is newer than this running copy: a later original version, or
/// the same one with a higher revision.
pub fn port_is_newer(tag: &str) -> bool {
    let (Some((base, revision)), Some(current)) = (parse_port_tag(tag), version::parse(env!("CARGO_PKG_VERSION"))) else { return false };
    match base.cmp_precedence(&current) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Equal => revision > port_revision(),
        std::cmp::Ordering::Less => false,
    }
}

/// The Android package of a port release.
pub fn port_asset(release: &Release) -> Option<&artcraft_launcher_core::release::Asset> {
    release.assets.iter().find(|a| a.name.starts_with("artcraft-launcher-") && a.name.ends_with("-android-arm64.apk"))
}

/// Installs the already downloaded and verified port package over this app.
pub fn install_file(apk: &Path) -> Result<(), String> {
    let bridge = bridge()?;
    if !bridge.can_install_packages() {
        bridge.request_install_permission()?;
        return Err("Allow ArtCraft Launcher to install apps, then try the update again.".into());
    }
    bridge.install_apk(apk)
}

/// The system bars' and cut-outs' size in pixels (top, bottom, left, right).
pub fn insets_px() -> [f32; 4] {
    bridge().map_or([0.0; 4], |b| b.insets_px())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_tags_round_trip_and_order() {
        assert_eq!(port_tag("0.16.1", 3), "android-v0.16.1-r3");
        let (base, rev) = parse_port_tag("android-v0.16.1-r3").unwrap();
        assert_eq!((base.to_string(), rev), ("0.16.1".to_string(), 3));
        assert!(parse_port_tag("v0.16.1").is_none());
        assert!(parse_port_tag("android-v0.16.1-rx").is_none());
        // A later original version is newer whatever the revision; this build is 0.16.1 r1.
        assert!(port_is_newer("android-v99.0.0-r1"));
        assert!(!port_is_newer("android-v0.0.1-r9"));
        assert!(!port_is_newer(&port_tag(env!("CARGO_PKG_VERSION"), port_revision())));
        assert!(port_is_newer(&port_tag(env!("CARGO_PKG_VERSION"), port_revision() + 1)));
    }
}
