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

            let picker_state = std::sync::Arc::new(std::sync::Mutex::new(None));
            let file_dialog_fn = photocraft_ui_egui::mobile::mobile_file_dialog_service(picker_state.clone());

            let services = Services {
                import: Some(Box::new(|name: &str, bytes: &[u8], opts: &photocraft_io::ImportOptions| {
                    photocraft_io::import_with_options(name, bytes, opts).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string())
                })),
                export: Some(Box::new(|doc: &photocraft_doc::Document, path: &str, settings: &photocraft_ui_egui::ExportSettings| {
                    photocraft_io::export(doc, path, &settings.io_options()).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
                })),
                file_dialog: Some(file_dialog_fn),
                write: Some(Box::new(|path: &str, bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| e.to_string()))),
                ..Default::default()
            };

            let mut app = PhotocraftApp::new(Session::new(), services);
            app.mobile_picker = picker_state;
            app.ui.mobile.enabled = true;
            Ok(Box::new(app))
        }),
    );
}
