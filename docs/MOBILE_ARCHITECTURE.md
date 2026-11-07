# PhotoCraft Mobile Architecture

This document describes how the mobile-first UI in this Android fork is implemented,
how it integrates with the upstream PhotoCraft engine, and what is (and is not) verified.

Audience: maintainers of this fork. For upstream sync see `UPSTREAM_MAINTENANCE.md`;
for AI agents see `AI_MAINTENANCE.md`; for test status see `KNOWN_ISSUES.md`.
Companion design docs: `mobile-ui-mapping.md` (component mapping) and `mobile-ui-plan.md` (slice plan).

## 1. Scope and approach

The mobile UI is an **additive shell** over the existing egui/eframe desktop UI.
The image editing engine, document model, command registry, tools, filters, renderer,
history, file formats and platform abstractions are **unchanged and reused as-is**.

Principles followed:

- One engine, two front-ends. The mobile shell selects a different layout and
  interaction model but drives the same commands and view models.
- No parallel implementations. Mobile panels host existing panel bodies; the
  mobile chrome issues existing command IDs (`edit.undo`, `edit.redo`, `file.save`,
  panel-group switches, `crate::tool_memory::select`, ...).
- Feature-toggle, not fork. A single runtime flag (`MobileUiState::enabled`) switches
  the shell; the desktop path is untouched when the flag is off.

## 2. Integration point

The mobile shell is selected inside the frame body builder:

- `crates/ui-egui/src/app/frame.rs` -- `ui_body(...)` branches on
  `self.ui.mobile.enabled && !home`. When enabled (and not on the home screen) it
  calls `panels::mobile_body(...)`; otherwise the existing desktop body runs verbatim.

This is the **only** structural branch in the rendering path. Everything else mobile
lives in the new `mobile` module and calls into existing `panels`/`widgets` APIs.

Wiring of the flag happens at launch:

- `apps/photocraft/src/launch_args.rs` -- `--mobile [phone|tablet]` flag
  (`Option<String>`, `default_missing_value = "auto"`), parsed into a new
  `PerProcess::Mobile` variant.
- `apps/photocraft/src/main.rs` -- after `PhotocraftApp::new`, sets
  `app.ui.mobile.enabled` and `app.ui.mobile.force_class` from the parsed flag.

State is persisted with the rest of `UiState`:

- `crates/ui-egui/src/state.rs` -- `UiState` gains
  `pub mobile: crate::mobile::MobileUiState`, serde camelCase, defaults to disabled.

## 3. Crate / module / file inventory

### 3.1 New code (mobile-specific)

New module `crates/ui-egui/src/mobile/`:

| File | Responsibility |
|------|----------------|
| `mod.rs` | Module wiring and re-exports. |
| `device.rs` | `DeviceClass::{Phone, Tablet}`, `Orientation`, `classify()`. Short side >= 600 pt => Tablet; non-finite/non-positive => Phone. |
| `shell.rs` | `regions()` region model, `RegionId`, `RegionPane` metrics (`MIN_TOUCH_TARGET = 48`, top bar 44, bottom nav 56, rail 56, dock 320), `MobileShell`, `OpenPanels` (sheet fraction clamped 0.15-0.85). |
| `panels.rs` | Thin host that draws the **existing** `crate::panels::dock_body` on a group/tab. `HostKind::{BottomSheet, RightDock}`. Tab strip reuses `widgets::tab_strip::pill_tabs`. |
| `state.rs` | `MobileUiState { enabled, force_class, open_group, sheet_fraction }` (camelCase serde, defaults off). |
| `chrome.rs` | Mobile top bar (Home/Undo/Redo/Save/More via real commands), bottom nav (Layers/Adjustments/Color/History/More -> real panel groups), tablet `tool_rail` (reuses `panels::toolbar`), `tool_options` (reuses `panels::options_bar`), phone `tool_sheet` grid (uses `crate::tool_memory::select`). |
| `shell_tests.rs` | 6 `egui_kittest` end-to-end tests: phone/tablet arrangements, bottom-nav open/close, undo gating, class-change state preservation, toggle-off restores desktop. |

### 3.2 Modified upstream files (minimal, additive)

| File | Change |
|------|--------|
| `crates/ui-egui/src/lib.rs` | Added `pub mod mobile;`. |
| `crates/ui-egui/src/state.rs` | Added `mobile: MobileUiState` field + default. |
| `crates/ui-egui/src/app/frame.rs` | Mobile branch in `ui_body`. |
| `crates/ui-egui/src/panels.rs` | Added `pub(crate) fn mobile_body(...)`; widened `visible_sections` visibility to `pub(crate)`. |
| `crates/ui-egui/src/panels/toolbar.rs` | `visible_sections` `pub(super)` -> `pub(crate)`. |
| `crates/ui-egui/src/widgets.rs` | `tab_strip` `pub(super)` -> `pub(crate)`. |
| `apps/photocraft/src/launch_args.rs` | `--mobile` flag + `PerProcess::Mobile`. |
| `apps/photocraft/src/main.rs` | Applies mobile flag to `UiState` after construction. |

Visibility widenings are the only edits to shared code that are not strictly new
additions; they change no behavior.

### 3.3 Shared with upstream vs mobile-specific

- **Shared (unmodified behavior):** all engine crates; `ui-egui` panels, widgets,
  tools, filters, docking, i18n, rendering; document model, command registry,
  history, file formats, platform services.
- **Mobile-specific:** everything under `crates/ui-egui/src/mobile/`, the
  `--mobile` launch flag, and the `MobileUiState` persisted field.

## 4. How the mobile UI talks to the engine

The mobile shell never reimplements editing logic. It:

1. Renders a mobile arrangement (`regions()` + `MobileShell`).
2. Hosts existing panel bodies via `panels::dock_body` inside a bottom sheet (phone)
   or right dock (tablet).
3. Issues existing commands through the normal session command path --
   the real seam is `Session::execute(id, json_params)` -- for top-bar actions,
   bottom-nav group switches, and tool selection via `tool_memory::select`.

Because panels and commands are the shared upstream surfaces, mobile inherits
document state, history, dirty/saved semantics, progress and error reporting for free.

## 5. Responsive behavior

- **Phones:** portrait-first; canvas-centered; no permanently visible side panels;
  bottom sheets for tools/inspector; contextual tool options above the bottom nav.
- **Tablets:** landscape and portrait; optional left tool rail (reuses `panels::toolbar`);
  central canvas; right-side inspector dock (reuses `panels::dock_body`); desktop-like
  arrangement.

`DeviceClass` is derived by `classify()` (short side >= 600 pt => Tablet) and can be
forced with `--mobile phone` / `--mobile tablet` for testing.

## 6. Known limitations / incomplete / unverified

State these plainly when reporting status.

- **Layout slice only.** This slice delivers the mobile shell, chrome, panel hosting,
  tool rail/sheet, and bottom navigation. It does not yet reflow every individual
  panel's internals for narrow widths.
- **Home / New Document / Open-Import / Save-Export / Settings mobile screens are not
  part of this slice.** The required screen list in the task is broader than what is
  implemented; the editor shell is the delivered piece.
- **Unverified:** real-device touch/stylus behavior, on-device performance, GPU/CPU
  fallback under mobile workloads, and foldable/narrow phone widths have not been
  exercised beyond the `egui_kittest` layout tests.
- **Android packaging/run path** for this fork is not validated in this environment
  (see build/run below).
- The 26 ui-egui test failures are unrelated to the mobile module and unresolved;
  see `KNOWN_ISSUES.md`. No mobile test fails.

## 7. Build, run, test

Environment notes: external git deps are already vendored under
`~/.cargo/git/checkouts`, so use `--offline`. Rust 1.99.1.

```sh
# From the repository root (pcm/photocraft)

# Compile checks
cargo check --offline -p photocraft-engine
cargo check --offline -p photocraft-ui-egui
cargo check --offline -p photocraft

# Layering rule check
cargo xtask layers

# Lint the mobile module
cargo clippy --offline -p photocraft-ui-egui

# Mobile unit + shell tests (fast, targeted)
cargo test --offline -p photocraft-ui-egui --lib mobile::
```

Run the app in mobile mode:

```sh
cargo run --offline -p photocraft -- --mobile        # auto-classify by window
cargo run --offline -p photocraft -- --mobile phone  # force phone layout
cargo run --offline -p photocraft -- --mobile tablet # force tablet layout
```

Do not run the full suite by default; it is slow and currently red for unrelated
reasons (see `KNOWN_ISSUES.md`). Use the targeted `mobile::` filter above.

Note: the wasm target is not installed in this environment, so the wasm gate cannot
be run locally. The `photocraft` **test** binary does not compile here due to a
pre-existing test-only issue (`apps/photocraft/src/external_svg.rs:1391`, `mkfifoat`);
non-test `cargo check -p photocraft` passes.
