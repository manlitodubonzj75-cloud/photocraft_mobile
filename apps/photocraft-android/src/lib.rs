//! PhotoCraft Android entrypoint.
//!
//! Hosts [`photocraft_ui_egui::PhotocraftApp`] inside an Android `NativeActivity`
//! using `eframe` and `android-activity`.

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    use photocraft_engine::Session;
    use photocraft_ui_egui::{PhotocraftApp, Services};

    let mut options = eframe::NativeOptions::default();
    options.android_app = Some(app);

    let _ = eframe::run_native(
        "PhotoCraft",
        options,
        Box::new(|cc| {
            PhotocraftApp::install_type_reset_hooks(&cc.egui_ctx);
            let mut app = PhotocraftApp::new(Session::new(), Services::default());
            app.ui.mobile.enabled = true;
            Ok(Box::new(app))
        }),
    );
}
