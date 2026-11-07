# PhotoCraft mobile UI: implementation plan

Status: **implementation plan**, companion to `docs/mobile-ui-mapping.md`.

The mapping document (`docs/mobile-ui-mapping.md`) establishes *what exists* and *where the seams are*.
This document fixes *how the mobile shell is built on those seams*, slice by slice, so every step
compiles and passes the repository gates (`cargo xtask layers`, the wasm gate, clippy denies).

Every slice is independently reviewable and leaves the tree green.

---

## 0. Guiding decisions

1. **The mobile UI is a wrapping host, not a fork.** The browser build already proves the pattern:
   `apps/photocraft-web/src/web.rs` defines `WebShell { app: PhotocraftApp }`, implements
   `eframe::App`, and drives the *same* `PhotocraftApp` through `logic_frame` / `ui_frame` /
   `finish_type_reset_logic` / `finish_type_reset_pass` (`crates/ui-egui/src/app/frame.rs:35`,
   `:148`). The mobile shell does the same: `MobileShell { app: PhotocraftApp }`.
   - No engine, `doc`, `ops`, `compose`, `gpu`, `io`, `format` crate is touched.
   - No editing algorithm is reimplemented.
   - No `PhotocraftApp` code is duplicated; the new code only *arranges* the existing draw calls.

2. **All behaviour routes through the existing command registry.** Any action the mobile chrome
   offers calls `PhotocraftApp`'s existing command dispatch using the **exact ids** in
   `crates/ui-egui/src/menu_catalog.rs`. No parallel command table, no invented ids.

3. **Reuse the existing panel bodies.** The Inspector sheet and the tablet dock both draw the
   *existing* panel bodies (`crates/ui-egui/src/panels/dock.rs` `dock_body`, dispatch keyed by
   `panels/panel_docking.rs` `PANELS`). The mobile shell chooses *where* they are hosted (bottom
   sheet vs side dock); it does not re-draw them.

4. **Responsive device class is a single derived value.** `DeviceClass::{Phone, Tablet}` is computed
   from the egui viewport size each frame. Nothing else in the shell hard-codes a breakpoint, so a
   foldable or a resized window reclassifies cleanly with no lost state (task: "No feature should
   silently disappear due to responsive layout").

5. **Honest feature surfacing.** Where the task asks for something the engine does not have
   (recent *projects*, favourite tools, two-finger pan), it is either (a) implemented for real as
   part of the slice that needs it, or (b) shown as unavailable with the reason — never faked.

---

## 1. Repository integration facts (verified)

| Fact | Value |
|---|---|
| Workspace members | `crates/*`, `apps/*`, `xtask` (`Cargo.toml`) |
| App pattern | `apps/photocraft` (native eframe), `apps/photocraft-cli`, `apps/photocraft-web` (wasm trunk) |
| `PhotocraftApp` public fields | `session: Session`, `ui: UiState`, `services: Services` (`crates/ui-egui/src/lib.rs:343`) |
| Host hooks | `logic_frame`, `ui_frame`, `finish_type_reset_logic`, `finish_type_reset_pass` (all `pub`) |
| Command dispatch | `Session::execute(id, json_params) -> Result<Value, EngineError>` (`crates/engine/src/lib.rs`) |
| Layers rule | apps are `Class::Exempt`; a new app crate must be added to `xtask/src/layers.rs` `TABLE` |
| wasm gate | L0–L6 crates must `cargo check --target wasm32-unknown-unknown` |

---

## 2. Slice plan

### Slice 1 — mapping (DONE)
`docs/mobile-ui-mapping.md`. No code change.

### Slice 2 — device class + responsive shell skeleton (DONE)
- New module `crates/ui-egui/src/mobile/` (library-side, so it is unit-testable with
  `egui_kittest` and reuse from *any* host app):
  - `DeviceClass` (`Phone`/`Tablet`) with an explicit, documented breakpoint and a
    `classify(size, scale)` function.
  - `MobileShellLayout`: the region model (top bar, canvas, tool rail, inspector dock, bottom nav,
    open sheet) computed from `DeviceClass` + current `UiState`, with **state preservation** across
    layout changes.
- Wire the shell as an opt-in mode (`UiState`) rather than replacing desktop layout, so the
  desktop app is byte-for-byte unaffected when the mode is off.
- Unit tests: classify boundaries; layout invariants (phone has no permanent side panels; tablet
  keeps canvas ≥ min); state preservation across a phone->tablet reclassification.

### Slice 3 — host the existing panels in the sheet/dock (DONE)
- Phone: a draggable bottom sheet that renders `dock_body` for the active tab.
- Tablet: a right-hand dock reusing the same `dock_body`.
- Reuse `panel_docking::PANELS` for the tab list; no new panel ids.

### Slice 4 — Home / New Document / Open-Import mobile arrangement (NOT STARTED)
- Reuse `home_recent_ui`, `new_doc_ui`, `file_ui` states and their commands.
- Add the project store behind a `Services`-style closure (platform-backed), keeping
  `document.recent.*` as the *preset* recents only.

### Slice 5 — input: device classification + two-finger pan (NOT STARTED)
- Classify pointer events by `PointerKind` in the canvas input path.
- Two-finger pan alongside the existing pinch-zoom / rotate; keep
  `stylus::pen_hold_is_no_right_click`.

### Slice 6+ — settings, export sheet, complex workspaces as full-screen.

Each slice: `cargo check --offline`, `cargo xtask layers`, affected crate tests, wasm gate for
L0–L6.

---

## Revision history

- 2026-11-07: initial plan; slice 2 in progress.
- 2026-11-07: slices 2-3 implemented; `--mobile [phone|tablet]` launch flag and app
  wiring added. 26 mobile tests pass (see `KNOWN_ISSUES.md`). Slice 4+ not started.
  Docs: `MOBILE_ARCHITECTURE.md`, `UPSTREAM_MAINTENANCE.md`, `AI_MAINTENANCE.md`,
  `KNOWN_ISSUES.md`.
