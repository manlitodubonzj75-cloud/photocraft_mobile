//! Android platform services and crash recovery store.

use photocraft_doc::Document;
use photocraft_format::RecoveryStore;
use photocraft_ui_egui::mobile::ActivePicker;
use photocraft_ui_egui::{Recoverable, Services};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

type SharedRecovery = Rc<RefCell<Option<RecoveryStore>>>;

fn with_store<T>(store: &SharedRecovery, f: impl FnOnce(&mut RecoveryStore) -> T) -> Result<T, String> {
    let mut slot = store.try_borrow_mut().map_err(|_| "crash recovery is busy".to_string())?;
    Ok(f(slot.as_mut().ok_or("no recovery directory")?))
}

/// Create Android platform services with recovery store wired to `recovery_dir`.
pub fn android_services(recovery_dir: Option<PathBuf>, picker_state: Arc<Mutex<Option<ActivePicker>>>) -> Services {
    let recover_dir = recovery_dir.clone();
    let store: SharedRecovery = Rc::new(RefCell::new(recovery_dir.map(RecoveryStore::new)));
    let (s1, s2, s4, s5) = (store.clone(), store.clone(), store.clone(), store.clone());

    let file_dialog_fn = photocraft_ui_egui::mobile::mobile_file_dialog_service(picker_state);

    Services {
        import: Some(Box::new(|name: &str, bytes: &[u8], opts: &photocraft_io::ImportOptions| {
            photocraft_io::import_with_options(name, bytes, opts).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string())
        })),
        export: Some(Box::new(|doc: &photocraft_doc::Document, path: &str, settings: &photocraft_ui_egui::ExportSettings| {
            photocraft_io::export(doc, path, &settings.io_options()).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
        })),
        file_dialog: Some(file_dialog_fn),
        write: Some(Box::new(|path: &str, bytes: &[u8]| std::fs::write(path, bytes).map_err(|e| e.to_string()))),
        autosave: Some(Box::new(move |doc: &Arc<Document>, revision: u64, path: Option<&str>| {
            with_store(&s1, |s| s.autosave_checked(doc, revision, path.map(str::to_string)))?
        })),
        autosave_results: Some(Box::new(move || with_store(&s4, |s| s.take_completed()).unwrap_or_default())),
        flush_autosaves: Some(Box::new(move || with_store(&s5, RecoveryStore::flush).unwrap_or_default())),
        discard_autosave: Some(Box::new(move |id: u64| {
            let _ = with_store(&s2, |s| s.discard(id));
        })),
        recover: Some(Box::new(move || {
            let Some(dir) = recover_dir.as_ref() else { return Vec::new() };
            photocraft_format::list_recovery(dir)
                .into_iter()
                .map(|e| Recoverable {
                    key: e.info.key.clone(),
                    name: e.info.document_name.clone(),
                    path: e.info.original_path.clone(),
                    load: Box::new(move || photocraft_format::recover(&e).map_err(|e| e.to_string())),
                })
                .collect()
        })),
        adopt_autosave: Some(Box::new(move |id: u64, key: &str| {
            let _ = with_store(&store, |s| s.adopt(id, key));
        })),
        ..Default::default()
    }
}
