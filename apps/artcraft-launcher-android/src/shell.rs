//! The Android window: one eframe app around [`LauncherUi`] and the shared [`NativeBackend`].

use android_activity::AndroidApp;
use artcraft_launcher_core::PackageKind;
use artcraft_launcher_core::protocol::Backend;
use artcraft_launcher_native::{NativeBackend, android, paths};
use artcraft_launcher_ui_egui::{Host, LauncherUi, PortInfo, Settings, TitleBar};

use crate::jni_bridge::JniBridge;

/// What the interface asks of the system. Android has no tray, desktops, login items or folder
/// picker, so only quitting and the title bar differ from the defaults.
struct AndroidHost {
    quit: bool,
}

impl Host for AndroidHost {
    fn title_bar(&self) -> TitleBar {
        // The system draws the status bar; the interface adds no title bar of its own.
        TitleBar::Native
    }

    fn default_install_folder(&mut self, _kind: PackageKind, _arch: Option<artcraft_launcher_core::Arch>) -> Option<String> {
        // Android installs apps itself; there is no folder to choose or show.
        None
    }

    fn quit(&mut self) {
        self.quit = true;
    }
}

struct Shell {
    ui: LauncherUi,
    host: AndroidHost,
    /// The system bars' and cut-outs' size in points: top, bottom, left, right.
    insets: [f32; 4],
}

impl Shell {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        let backend: Box<dyn Backend> = Box::new(NativeBackend::new(move || ctx.request_repaint()));
        let mut ui = LauncherUi::new(&cc.egui_ctx, backend, Settings::default());
        ui.attach(&cc.egui_ctx);
        ui.set_port(Some(PortInfo { system: "Android".into(), report_url: android::PORT_ISSUES_URL.into() }));
        Self { ui, host: AndroidHost { quit: false }, insets: [0.0; 4] }
    }
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.cumulative_frame_nr() % 30 == 0 {
            let scale = ctx.pixels_per_point().max(0.1);
            self.insets = android::insets_px().map(|px| px / scale);
        }
        self.ui.logic(ctx, &mut self.host);
        if std::mem::take(&mut self.host.quit) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Keep the interface clear of the status bar, the navigation bar and a camera cut-out.
        let [top, bottom, left, right] = self.insets;
        let full = ui.max_rect();
        let area = egui::Rect::from_min_max(full.min + egui::vec2(left, top), full.max - egui::vec2(right, bottom));
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(area));
        self.ui.ui(&mut inner, &mut self.host);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.ui.raw_input(ctx, raw_input);
    }
}

/// Registers the package-manager bridge and the app's private folder, then runs the window until
/// the activity finishes.
pub fn run(app: AndroidApp) -> Result<(), String> {
    match app.internal_data_path() {
        Some(root) => paths::set_android_root(root),
        None => return Err("Android gave the app no private folder".into()),
    }
    // A second start of the activity in the same process keeps the first bridge: it holds the
    // application context, which outlives any one activity.
    if !android::set_bridge(Box::new(JniBridge::new(&app)?)) {
        log::debug!("the package-manager bridge was already registered");
    }
    let options = eframe::NativeOptions { android_app: Some(app), renderer: eframe::Renderer::Wgpu, ..Default::default() };
    eframe::run_native("ArtCraft Launcher", options, Box::new(|cc| Ok(Box::new(Shell::new(cc))))).map_err(|e| e.to_string())
}
