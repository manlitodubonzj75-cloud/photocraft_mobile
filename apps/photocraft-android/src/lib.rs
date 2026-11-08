//! PhotoCraft Android entrypoint.
//!
//! Hosts [`photocraft_ui_egui::PhotocraftApp`] inside an Android `NativeActivity`
//! using `eframe` and `android-activity`.

pub mod services;

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android_app: AndroidApp) {
    use photocraft_engine::Session;
    use photocraft_ui_egui::PhotocraftApp;

    let recovery_dir = android_app.internal_data_path().map(|p| p.join("recovery"));
    let mut options = eframe::NativeOptions::default();
    options.android_app = Some(android_app);

    let result = eframe::run_native(
        "PhotoCraft",
        options,
        Box::new(move |cc| {
            PhotocraftApp::install_type_reset_hooks(&cc.egui_ctx);

            let picker_state = std::sync::Arc::new(std::sync::Mutex::new(None));
            let services = services::android_services(recovery_dir, picker_state.clone());

            let mut app = PhotocraftApp::new(Session::new(), services);
            app.mobile_picker = picker_state;
            app.ui.mobile.enabled = true;

            // Wire GPU initialization or CPU fallback with diagnostic logging.
            if let Some(rs) = cc.wgpu_render_state.clone() {
                let info = rs.adapter.get_info();
                log::info!("Android wgpu adapter: name='{}', backend={:?}, device_type={:?}", info.name, info.backend, info.device_type);
                app.perf.gpu_info.set_adapter(&info);
                app.perf.gpu_info.set_available(&rs.available_adapters);
                photocraft_ui_egui::texel_guard::install(&cc.egui_ctx, &info);

                let is_software = info.device_type == eframe::wgpu::DeviceType::Cpu;
                if is_software {
                    let fallback_msg = "No compatible hardware graphics adapter; falling back to software graphics.";
                    log::warn!("Android GPU fallback: {fallback_msg}");
                    app.perf.gpu_info.canvas = "cpu".into();
                    app.perf.gpu_info.fallback = Some(fallback_msg.into());
                    app.watch_window_device(&rs.device);
                } else {
                    app.set_wgpu(rs);
                    app.start_compositor(&cc.egui_ctx);
                    log::info!("Android GPU canvas compositor started successfully");
                }
            } else {
                let reason = "wgpu_render_state is unavailable; running in CPU fallback mode.";
                log::warn!("Android graphics fallback: {reason}");
                app.perf.gpu_info.canvas = "cpu".into();
                app.perf.gpu_info.fallback = Some(reason.into());
            }

            Ok(Box::new(app))
        }),
    );

    if let Err(err) = result {
        log::error!("PhotoCraft Android run_native exited with error: {err}");
    }
}
