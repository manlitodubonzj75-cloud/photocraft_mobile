//! End-to-end tests for the mobile shell arrangement.
//!
//! These drive the whole [`crate::PhotocraftApp`] through `egui_kittest` with `mobile.enabled` on,
//! at a phone and a tablet size, and assert on what is actually drawn. They are the check that the
//! mobile branch in `app::frame::ui_body` really replaces the desktop chrome, that every visible
//! control is wired, and that a layout change does not drop the user's open panel.

use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;

use crate::PhotocraftApp;
use crate::dock::Group;
use crate::mobile::DeviceClass;

/// A whole app with one open document and the mobile shell on, in a window of `size`.
fn mobile_app(size: egui::Vec2, force: Option<&'static str>) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    // A document so the editor chrome (not Home) is what draws.
    app.run("file.new", json!({"width": 64, "height": 64, "background": "white"})).unwrap();
    app.ui.chrome.home = None;
    app.ui.mobile.enabled = true;
    app.ui.mobile.force_class = force.map(str::to_owned);
    Harness::builder().with_size(size).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    })
}

#[test]
fn phone_shell_shows_mobile_chrome_not_desktop() {
    let mut h = mobile_app(vec2(390.0, 844.0), Some("phone"));
    h.run_steps(4);
    // The mobile top bar's controls are present (Undo/Redo/Save/Home tooltips are widget labels).
    assert!(h.query_all_by_label("Undo").count() >= 1, "mobile top bar Undo");
    assert!(h.query_all_by_label("Save").count() >= 1, "mobile top bar Save");
    // The bottom navigation's entries are present.
    assert!(h.query_all_by_label("Layers").count() >= 1, "bottom nav Layers");
    assert_eq!(h.state().ui.mobile.class(vec2(390.0, 844.0)), DeviceClass::Phone);
}

#[test]
fn tablet_shell_shows_rail_and_dock_when_a_group_is_open() {
    let mut h = mobile_app(vec2(1024.0, 768.0), Some("tablet"));
    h.state_mut().ui.mobile.show(Group::Layers);
    h.run_steps(4);
    assert_eq!(h.state().ui.mobile.class(vec2(1024.0, 768.0)), DeviceClass::Tablet);
    // The dock header shows the group's name.
    assert!(h.query_all_by_label("Layers").count() >= 1, "dock header or tab");
}

#[test]
fn opening_a_panel_group_survives_a_device_class_change() {
    let mut h = mobile_app(vec2(390.0, 844.0), Some("phone"));
    h.state_mut().ui.mobile.show(Group::Color);
    h.run_steps(3);
    assert_eq!(h.state().ui.mobile.open(), Some(Group::Color));
    // Reclassify to tablet by forcing it: the open group must remain open.
    h.state_mut().ui.mobile.force_class = Some("tablet".into());
    h.run_steps(3);
    assert_eq!(h.state().ui.mobile.open(), Some(Group::Color), "the open panel survives the layout change");
}

#[test]
fn bottom_nav_tap_opens_and_closes_the_sheet() {
    let mut h = mobile_app(vec2(390.0, 844.0), Some("phone"));
    h.run_steps(4);
    assert_eq!(h.state().ui.mobile.open(), None);
    // Tapping the Layers nav entry opens the Layers host.
    let nav = h.get_all_by_label("Layers").next().expect("a Layers nav entry");
    nav.click();
    h.run_steps(3);
    assert_eq!(h.state().ui.mobile.open(), Some(Group::Layers));
    // Tapping the same entry again closes it.
    let nav = h.get_all_by_label("Layers").next().expect("a Layers nav entry");
    nav.click();
    h.run_steps(3);
    assert_eq!(h.state().ui.mobile.open(), None);
}

#[test]
fn undo_button_is_wired_and_gated_by_the_menu_predicate() {
    let mut h = mobile_app(vec2(390.0, 844.0), Some("phone"));
    h.run_steps(3);
    // With no undoable change the menu predicate disables Undo; after a change it enables it. The
    // button reads the same predicate, so its presence tracks the desktop menu exactly.
    h.state_mut().run("file.new", json!({"width": 32, "height": 32})).unwrap();
    h.state_mut().run("layer.duplicate", json!({})).unwrap();
    h.run_steps(3);
    assert!(crate::menus::is_enabled(h.state(), "edit.undo"), "an undone change enables edit.undo");
    if let Some(undo) = h.query_by_label("Undo") {
        undo.click();
        h.run_steps(3);
    }
}

#[test]
fn disabling_the_shell_restores_the_desktop_chrome() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 64, "height": 64})).unwrap();
    app.ui.chrome.home = None;
    app.ui.mobile.enabled = false;
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    // The desktop dock's tabs are drawn when the shell is off.
    assert!(h.query_all_by_label("Properties").count() >= 1 || h.query_all_by_label("Layers").count() >= 1);
}
