//! In-app photo and file picker for mobile viewports.
//!
//! Provides a touch-friendly modal dialog for selecting image files
//! on Android and other mobile environments where desktop file pickers are unavailable.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::{Color32, CornerRadius, Stroke, vec2};

use crate::file_dialog::{FileDialogAnswer, FileDialogReply};
use crate::theme::Tokens;

/// Common image extensions supported for opening.
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "psd", "psb", "pcraft", "gif", "bmp", "tif", "tiff", "heic", "heif", "svg", "qoi", "tga"];

/// Known mobile photo locations on Android / Linux filesystems.
fn default_locations() -> Vec<(&'static str, PathBuf)> {
    let mut locs = Vec::new();

    // Android standard external paths
    for base in &["/storage/emulated/0", "/sdcard"] {
        let p = Path::new(base);
        if p.exists() {
            let camera = p.join("DCIM/Camera");
            if camera.exists() {
                locs.push(("Камера", camera));
            } else {
                let dcim = p.join("DCIM");
                if dcim.exists() {
                    locs.push(("DCIM", dcim));
                }
            }
            let pictures = p.join("Pictures");
            if pictures.exists() {
                locs.push(("Галерея", pictures));
            }
            let download = p.join("Загрузки");
            let download_en = p.join("Download");
            if download_en.exists() {
                locs.push(("Загрузки", download_en));
            } else if download.exists() {
                locs.push(("Загрузки", download));
            }
            locs.push(("Память", p.to_path_buf()));
            break;
        }
    }

    // Fallbacks for desktop / sandbox testing
    if locs.is_empty() {
        if let Ok(home) = std::env::var("HOME") {
            let p = PathBuf::from(home);
            let pics = p.join("Pictures");
            if pics.exists() {
                locs.push(("Pictures", pics));
            }
            let dl = p.join("Downloads");
            if dl.exists() {
                locs.push(("Downloads", dl));
            }
            locs.push(("Home", p));
        } else {
            locs.push(("Root", PathBuf::from(".")));
        }
    }

    locs
}

/// Active state of the mobile file picker modal.
pub struct ActivePicker {
    pub current_dir: PathBuf,
    pub reply: Option<FileDialogReply>,
    pub error_msg: Option<String>,
}

impl std::fmt::Debug for ActivePicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivePicker")
            .field("current_dir", &self.current_dir)
            .field("has_reply", &self.reply.is_some())
            .field("error_msg", &self.error_msg)
            .finish()
    }
}

pub type SharedPicker = Arc<Mutex<Option<ActivePicker>>>;

/// Creates a `Services::file_dialog` closure backed by the shared mobile picker.
pub fn mobile_file_dialog_service(shared: SharedPicker) -> crate::file_dialog::FileDialogFn {
    Box::new(move |_request, _frame, reply| {
        let locs = default_locations();
        let initial = locs.first().map(|(_, p)| p.clone()).unwrap_or_else(|| PathBuf::from("/"));
        let mut slot = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = Some(ActivePicker { current_dir: initial, reply: Some(reply), error_msg: None });
    })
}

/// Render the mobile photo/file picker modal if active.
pub fn render(shared: &SharedPicker, ctx: &egui::Context) {
    let mut close = false;
    let mut chosen_file: Option<PathBuf> = None;

    {
        let mut slot = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(active) = slot.as_mut() else { return };

        let t = Tokens::get(ctx);
        let screen_rect = ctx.content_rect();

        // Modal backdrop overlay
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("picker_backdrop"))).rect_filled(
            screen_rect,
            CornerRadius::ZERO,
            Color32::from_black_alpha(180),
        );

        let dialog_rect = screen_rect.shrink2(vec2(8.0, 24.0));

        egui::Area::new(egui::Id::unique("picker_modal_area")).fixed_pos(dialog_rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
            ui.set_min_size(dialog_rect.size());
            ui.set_max_size(dialog_rect.size());

            ui.painter().rect_filled(dialog_rect, CornerRadius::same(12), t.dock);
            ui.painter().rect_stroke(dialog_rect, CornerRadius::same(12), Stroke::new(1.0, t.separator), egui::StrokeKind::Inside);

            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(dialog_rect.shrink(12.0)));

            // Header: Title and Close button
            inner.horizontal(|ui| {
                ui.heading(tl!("Open Image"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("  ✕  ").clicked() {
                        close = true;
                    }
                });
            });

            inner.separator();

            // Location chips (Camera, Pictures, Downloads, Storage)
            let locs = default_locations();
            inner.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for (label, path) in &locs {
                    let is_current = &active.current_dir == path;
                    let btn = egui::Button::new(*label).fill(if is_current { t.accent } else { t.hover }).min_size(vec2(0.0, 36.0));
                    if ui.add(btn).clicked() {
                        active.current_dir = path.clone();
                        active.error_msg = None;
                    }
                }
            });

            inner.add_space(8.0);

            // Path bar and Up button
            inner.horizontal(|ui| {
                if let Some(parent) = active.current_dir.parent() {
                    if ui.button(" ⬆ Наверх ").clicked() {
                        active.current_dir = parent.to_path_buf();
                        active.error_msg = None;
                    }
                }
                ui.label(egui::RichText::new(active.current_dir.to_string_lossy()).size(11.0).color(t.text_dim));
            });

            inner.separator();

            if let Some(err) = &active.error_msg {
                inner.colored_label(Color32::from_rgb(255, 100, 100), err);
            }

            // List entries
            match fs::read_dir(&active.current_dir) {
                Ok(entries) => {
                    let mut dirs = Vec::new();
                    let mut files = Vec::new();

                    for entry in entries.flatten() {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') {
                            continue;
                        }
                        if path.is_dir() {
                            dirs.push((name, path));
                        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if IMAGE_EXTS.contains(&ext.to_lowercase().as_str()) {
                                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                                files.push((name, path, size));
                            }
                        }
                    }

                    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut inner, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;

                        if dirs.is_empty() && files.is_empty() {
                            ui.add_space(20.0);
                            ui.label("В этой папке нет изображений.");
                        }

                        // Subdirectories
                        for (name, path) in dirs {
                            let resp = ui.add_sized(vec2(ui.available_width(), 44.0), egui::Button::new(format!("📁  {name}")).fill(Color32::TRANSPARENT));
                            if resp.clicked() {
                                active.current_dir = path;
                                active.error_msg = None;
                            }
                        }

                        // Image files
                        for (name, path, size) in files {
                            let size_str =
                                if size > 1024 * 1024 { format!("{:.1} MB", size as f32 / (1024.0 * 1024.0)) } else { format!("{} KB", size / 1024) };

                            let btn_text = format!("🖼  {name}  ({size_str})");
                            let resp = ui.add_sized(vec2(ui.available_width(), 48.0), egui::Button::new(btn_text).fill(t.hover));
                            if resp.clicked() {
                                chosen_file = Some(path);
                            }
                        }
                    });
                }
                Err(e) => {
                    inner.label(format!("Не удалось открыть папку: {e}"));
                }
            }
        });
    }

    if close {
        let mut slot = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(mut active) = slot.take() {
            if let Some(reply) = active.reply.take() {
                reply.send(None);
            }
        }
    } else if let Some(path) = chosen_file {
        let mut slot = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(mut active) = slot.take() {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "image.png".to_string());
            match fs::read(&path) {
                Ok(bytes) => {
                    if let Some(reply) = active.reply.take() {
                        reply.send(Some(FileDialogAnswer::Contents(name, bytes)));
                    }
                }
                Err(e) => {
                    log::error!("Failed to read photo {path:?}: {e}");
                    if let Some(reply) = active.reply.take() {
                        reply.send(None);
                    }
                }
            }
        }
    }
}
