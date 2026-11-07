# PhotoCraft mobile UI: component -> architecture mapping

Status: **mapping / reconnaissance deliverable**. This document answers the first requirement of the
mobile-UI task: *"Before changing the implementation, inspect the repository and prepare a clear
mapping of existing components, commands, panels and platform capabilities to the proposed mobile
architecture."*

It is based on a read-only inspection of this repository at commit `f68e22702` (`main`). Every claim
below cites a real file and line. Where the task assumes something that does **not** exist in the
codebase, it is called out explicitly rather than invented.

---

## 0. Headline findings that change the plan

| Task assumption | Reality in the code | Consequence |
|---|---|---|
| "Reuse the existing `Session` (`dispatch`, `snapshot`, `events`, `view_models`)" | `docs/architecture.md` section 4.1 shows an **aspirational** async seam (`dispatch`/`snapshot`/`events`/`view_models`, `Services` injected). The **real** `Session` is a synchronous, multi-document struct defined in `crates/engine/src/lib.rs:202` with a single command entry point `Session::execute(id, json_params)`. | Build against `Session::execute` + `CommandSpec`, not the design-doc API. |
| "Use existing view models (`LayersModel`, `ViewModels`)" | **No typed view-model structs exist.** They appear only in `docs/architecture.md` prose. Panels read the live `Session`/`DocState`/`Document` directly. The nearest existing abstraction is `crates/engine/src/inspect.rs`, which emits `serde_json::Value`. | A non-egui mobile shell needs a **new extraction layer**; `inspect.rs` JSON is the starting point. |
| "Reuse existing tool groups and flyout structure" | Real and reusable: `crates/ui-egui/src/panels/toolbar.rs:9` `TOOL_SECTIONS` (sections -> slots -> tools) is the authoritative grouping, with existing long-press (>=0.35 s) and right-click flyouts. | Reuse `TOOL_SECTIONS`, `visible_sections`, `slot_tool`, `toolbar_position`. |
| "`document.recent.*`" as a recent-project store | `document.recent.*` exists (`crates/engine/src/cmd/document_preset_cmds.rs:331`) but is the **recent document-creation settings** list (`PresetState.recent_documents`, cap 20), *not* recent files. There is **no recent-projects concept at all**. | Do not use it as a project DB (the task already forbids this). Recent *files* are `FileHandling.recent_files` in prefs. |
| "Reuse existing egui/eframe and WASM infrastructure" | Yes - but the Windows/macOS/Linux desktop shell is native egui/eframe on wgpu; the web build is the same Rust via trunk/wasm-bindgen. No webview, no Tauri (forbidden by `AGENTS.md`). | A mobile shell is either (a) a new egui view inside this workspace, or (b) WASM. Both reuse the engine. |
| "Tool options schema" | **No `options_schema` exists.** Options are a flat `ToolOptions` struct (`crates/ui-egui/src/state/tool_options.rs:7`) plus hand-written per-tool renderers. `option_keys(tool)` (`tools/tool_reset.rs:156`) is the closest to a per-tool schema. | "Dynamic parameter editors" must be built on command param **JSON schemas** (which *do* exist for commands), not tool options. |
| "Two-finger pan and zoom" | Pinch-zoom and rotation exist (egui `zoom_delta`/`rotation_delta`, consumers in `canvas/wheel_nav.rs`, `canvas/rotate_view.rs`). **Two-finger pan does not exist** - pan is Hand/space/middle-drag/flick only. | Two-finger pan is new work in `canvas.rs`. |

---

## 1. Engine seam (what a mobile shell must call)

### 1.1 The single entry point

```rust
// crates/engine/src/lib.rs:202  (real definition)
#[derive(Default)]
pub struct Session { docs: Vec<DocState>, active: Option<usize>, tools: ToolState, /* ... */ }

// The command entry point used by *every* frontend (egui, CLI, control channel, MCP):
//   crates/engine/src/lib.rs:1-6  doc comment: "Every frontend goes through the same
//   Session::execute entry point."
```

- Commands are `CommandSpec` in `crates/engine/src/cmd/*` (e.g.
  `crates/engine/src/cmd/file_cmds.rs`, `document_preset_cmds.rs`), each with `id`, `label`,
  `menu_path`, `default_shortcut`, JSON `params` schema, `enabled`, `run`.
- `Session::execute(id, params)` is synchronous and returns `Result<Value, EngineError>`.
- The registry (`crates/engine/src/commands.rs`, `command_specs()`) is the catalogue a mobile shell
  enumerates to build menus, a searchable palette, and the tool selector.
- Layer/dialog metadata generation and "add a command" rules: `AGENTS.md` section 3 rule 1.

### 1.2 Platform capabilities (the mobile platform-services seam)

The abstraction is **not** a trait hierarchy - it is a struct of boxed closures/function pointers:

```rust
// crates/ui-egui/src/services.rs:414
#[derive(Default)]
pub struct Services { /* Option<...> fields */ }
```

Relevant typedefs (all in `crates/ui-egui/src/services.rs` unless noted):

| Capability | Type | Line |
|---|---|---|
| OS file picker | `FileDialogFn` (`crates/ui-egui/src/file_dialog.rs:70`) | 428 |
| Screen/photo capture picker | `screen_picker::Service` | 419 |
| Import bytes -> Document | `ImportFn` | 75 |
| Export Document -> bytes | `ExportFn` | 219 |
| Write bytes to path | `WriteFn` | 221 |
| Save document (background) | `SaveFileFn` | 240 |
| Open URL / system share | `OpenUrlFn` | 273 |
| Clipboard (image/svg) | `ClipboardSetFn` / `ClipboardGetFn` / `ClipboardGetSvgFn` | 278-288 |
| Prefs load/save | `LoadTextFn` / `SaveTextFn` | 343/346 |
| Autosave / recovery | `AutosaveFn` ... `RecoverFn` | 352-370 |
| OS open-documents events | `OsEventsFn` (`file_open.rs:15` `OsEvent`) | 386 |
| Pen tablet polling | `TabletPollFn` | 535 |
| Print spooler (on Session, not Services) | `print_cmds::Spooler` (`lib.rs:281`) | - |

Injection seam: `apps/photocraft/src/services.rs::native()` (~L701-850) builds the concrete
`Services`; `apps/photocraft/src/main.rs` (~L503-526) does `PhotocraftApp::new(session, services)`.
The real file-picker call is `show_file_dialog` (`apps/photocraft/src/services.rs:~204`, `rfd`).

**Mobile implication:** the OS file picker, photo library, share sheet and stylus feed all slot in
here as new `Services` implementations - no engine change required. Requirement 4.3 ("mobile system
file picker", "photo library integration") maps to `file_dialog` + `screen_pick`/`import`.

---

## 2. Screen-by-screen mapping

### 2.1 Home

| Requirement | Existing component | Gap |
|---|---|---|
| Recent project thumbnails | `crates/ui-egui/src/home_recent_ui.rs` (thumbnail cache, grid/list cards) | Built for *files*, not *projects*; card actions limited. |
| Create / Open / Import | `file.new`, `file.open`, `file.place` commands | None (wire buttons). |
| Recovery entry point | Recovery services `Services::recover` (`services.rs:370`), UI `prefs/prefs_ui/startup.rs::recovery` (L120) | Missing a dedicated Home surface; today it is a launch-time prompt. |
| Rename / duplicate / remove / delete project | Recent **files** helpers: `app/files.rs::push_recent`/`remove_recent`/`clear_recent` (`crates/ui-egui/src/app/files.rs:11-57`) | Duplicate/delete-a-file are **new** (no command). |

**Recent projects vs recent presets (requirement).** Two distinct stores already exist and must stay
separate:
- Recent **files**: `FileHandling.recent_files` (`crates/engine/src/prefs.rs:506`), cap
  `recent_file_count` (default 20) - persisted in `preferences.json` under `fileHandling.recentFiles`.
- Recent **document-creation settings**: `PresetState.recent_documents`
  (`crates/engine/src/presets.rs:85`), commands `document.recent.list|add`, `document.presets.*`
  (`crates/engine/src/cmd/document_preset_cmds.rs:291-350`), `MAX_RECENT = 20` (L19).

There is **no project database**. A mobile "recent projects" list should be a new platform-backed
store (platform services), *not* `document.recent.list`.

### 2.2 New Document

| Requirement | Existing component |
|---|---|
| Preset categories (Recent/Photo/Print/.../Saved) | `crates/ui-egui/src/dialogs/new_doc_ui` + presets in `crates/engine/src/cmd/document_preset_cmds.rs` |
| Select preset / custom W,H | `file.new` params validated by `validate_new_params` (`document_preset_cmds.rs:55`) |
| Units, PPI | `file.new` (`resolution`) |
| RGB/Gray/CMYK/Lab | `color_mode()` (`document_preset_cmds.rs:22`): `rgb`/`gray|grayscale`/`cmyk`/`lab` |
| 8/16/32-bit | `sample_type()` (L32): 8/16/32 |
| White/Transparent/custom bg | `background_color()` (L42) |
| Save custom preset | `document.presets.save` (`document_preset_cmds.rs:310`) |
| Fixed Create action | `file.new` |

`MAX_DIMENSION = 300_000`, `MAX_RESOLUTION = 30_000`, `MAX_PRESETS = 256`
(`document_preset_cmds.rs:12-19`). All mobile size/PPI controls must respect these.

### 2.3 Open / Import

- System picker -> `FileDialogFn`; photo library -> `screen_pick` service.
- Format filtering / read-only handling -> `crates/engine/src/cmd/file_cmds.rs` (`open`), import
  warnings via `file_open` notices; HEIC read-only is documented format behaviour (README).
- **Open as Document vs Place as Layer**: `file.open` vs `file.place` (`file_cmds.rs:1`, `place_bytes`
  at L50; `PLACE_EMBEDDED`).

### 2.4 Main editor

- Canvas: `crates/ui-egui/src/canvas.rs` + `canvas/` (`tool_input.rs`, `wheel_nav.rs`,
  `scrollbars.rs`, `gpu_canvas.rs`, `rotate_view.rs`). View state: `View` (`state.rs:139-169`):
  `zoom`, `center`, `rotation`, `fit_pending`, `doc_size`.
- Top toolbar (Back/title/Undo/Redo/Save/More): maps to existing shell commands; Undo/Redo are
  history commands (`cmd/history_cmds.rs`); Save is `file.save`.
- **Preserving editor state across panel open/close**: panel state is serde in
  `UiState` (`state.rs:445-475`: `panels`, `dock_tabs`, `dock`, `docking`, ...); `dock.rs::snapshot/apply`
  (L341/L415) persist it. Reuse this so state survives sheet/dock changes.

### 2.5 Tools

- Groups/flyouts: `TOOL_SECTIONS` (`panels/toolbar.rs:9-36`), 5 sections / 21 slots / 68 tools
  (`state/tool.rs:4-79`). Existing long-press (>=0.35 s) flyout at `toolbar.rs:293-324`.
- Tool ids: `crates/ui-egui/src/input/tool_shortcuts.rs::id` -> `tools.select.<name>` (L6); engine
  mirror `shell_commands::TOOLS` (`crates/engine/src/shell_commands.rs:108`).
- Contextual options: `panels/options_bar.rs::options_bar` (L42) + `tool_specific_options` (L596);
  per-tool option field list `option_keys` (`tools/tool_reset.rs:156`).
- **Searchable selector / recent / favorite tools**: the engine registry supports search; tool
  recency/favorites do **not** exist (only `UiState.group_tools`, `state.rs:228`, last-tool-per-key).

### 2.6 Inspector panels

Registry: `crates/ui-egui/src/panels/panel_docking.rs:25` `PANELS`; groups `dock.rs:25`:
`Color | Properties | Character | Navigator | History | Layers | Brushes`. Draw dispatch:
`panels/dock.rs:98` `dock_body` (match on `(Group, tab)`). Panel render fns (all take
`&mut PhotocraftApp`):

| Panel | Function | File |
|---|---|---|
| Layers | `layers::layers` | `panels/layers.rs:109` |
| Channels | `layers::channels` | `panels/layers.rs:620` |
| Paths | `vector_ui::paths_panel` | `tools/vector_ui/paths_panel.rs:156` |
| Properties | `properties::properties_body` | `panels/properties.rs:138` |
| Adjustments | `properties::adjustments_grid` | `panels/properties.rs:215` |
| History | `history::history` | `panels/history.rs:4` |
| Actions | `actions::panel` | `panels/actions.rs:1734` |
| Layer Comps | `comps_ui::panel` | `panels/comps_ui.rs:81` |
| Navigator | `navigator::navigator` | `panels/navigator.rs:3` |
| Histogram | `histogram_panel::show` | `panels/histogram_panel.rs:735` |
| Info | `info::info_panel` | `panels/info.rs:33` |
| Color | `color_panel_ui::panel` | `panels/color_panel_ui.rs` |
| Swatches | `swatches_ui::panel` | `panels/swatches_ui.rs:63` |
| Character/Paragraph | `type_tool::character_panel` | `tools/type_tool/panel.rs:209` |

**Properties depends on layer type** (requirement 2.6): `properties_body_inner`
(`panels/properties.rs:143-212`) picks by mask/vector-mask target, then document-vs-layer, then
`match &layer.content` (`LayerContent`: Raster/Group/Adjustment/Fill/Text/Shape/Smart;
`crates/doc/src/lib.rs:610` `kind_name`). Layer groups, masks, blend modes, opacity, clipping,
effects, Smart Objects and Smart Filters are all modelled in `crates/doc` and editable via existing
commands - reuse, do not reimplement.

Docking note: the layout engine (`craft_ui::docking::Workspace`) is an **external git dependency**
(`craft-libs`); only the registry, `Panels`/`DockTabs`/`DockLayout` state and content dispatch are in
this repo. A mobile bottom-sheet/dock must reuse the registry + content fns, not the craft-ui layout.

### 2.7 Color

- Panel: `panels/color_panel_ui.rs`; swatches `panels/swatches_ui.rs`; recent colours and
  `Fg`/`Bg` live in `ToolState` (`crates/engine/src/doc_state.rs:163`, `foreground`/`background`).
- **Colour management is real**: `photocraft-cms` (`crates/cms`), `Transform`/`transform::cached`;
  proofing state on `Session`. Never assume sRGB (`AGENTS.md` section 3 rule 2).
- Eyedropper / screen sampling: `screen_picker` service + `cmd/pick_cmds.rs`, `sampler_cmds.rs`.
- Colour models for pickers: `color_cmds.rs`, `color_channels.rs`.

### 2.8 Complex workspaces (Camera Raw, Liquify, Content-Aware Fill, Filter Gallery, Adaptive Wide Angle)

- These already exist as dialog/full-window states on `PhotocraftApp`:
  `pub(crate) camera_raw` (`ui-egui/src/lib.rs:597`), `wide_angle` (L605), `distort` (L593);
  filter preview plumbing `filter_preview`/`filter_preview_worker` (L522/L525) gives live preview.
- Commands: `cmd/gallery_cmds.rs` (Filter Gallery), `cmd/lens_cmds.rs` (Adaptive Wide Angle),
  `cmd/distort_cmds.rs` (Liquify), `cmd/retouch_cmds.rs` (Content-Aware), `cmd/photo_cmds.rs`.
- Progress/errors: `crates/ui-egui/src/jobs_ui.rs` + `crates/engine/src/jobs*` (background jobs,
  progress, cancellation) - reuse for "operation progress".

### 2.9 Save and Export

- Native save vs flattened export are already separate: `file.save`/`file.saveAs` (native
  `.pcraft`/PSD) vs Export As (`crates/engine/src/cmd/file_cmds/export.rs`,
  `crates/ui-egui/src/dialogs.rs` export UI; `ExportSettings` in `services.rs`).
- Format options & warnings: `FormatChoices` (`crates/ui-egui/src/services.rs:15`) with TGA bits, DDS,
  JPEG quality, TIFF options; size estimate + preview in the Export As dialog.
- Dirty/saved semantics preserved by engine `DocState`; keep them (requirement 2.9).

### 2.10 Settings

Prefs are one JSON store: `Preferences` (`crates/engine/src/prefs.rs:1225`) with **18 sections**
(`prefs/validate.rs:10`), persisted to `config_dir()/preferences.json`. Relevant categories map to
fields, not separate sections:

| Requirement | Pref location |
|---|---|
| Interface / appearance | `interface.*` (`theme`, `appearance_mode`, `language`, `ui_scale`) |
| Performance | `performance.*` (`use_gpu`, `gpu_backend`) |
| Colour management | `colorSettings` + `interface` |
| Autosave | `fileHandling.autosave` / `autosave_minutes` / `recover_on_launch` |
| File handling | `fileHandling.*` (`recent_files`, `recent_file_count`) |
| Gestures / touch | `enhancedControls.touch_gestures` / `zoom_with_trackpad_pinch` / `rotate_view_with_trackpad` (`prefs.rs:992`) |
| Stylus | `tools.use_tablet_pressure` / `tablet_input` / `pressure_curve` |
| Language | `interface.language` |

Read/write via `prefs.get`/`prefs.set`/`prefs.reset`; shell debounces + 3-way-merges
(`prefs/prefs_ui/save.rs`).

---

## 3. Responsive behaviour & input

- **No responsive/mobile/tablet view code exists** (grep for `responsive|mobile|tablet_mode|is_phone`
  -> none). This is entirely new.
- **Touch already reaches tools**: touch arrives as generic pointer input -> `ToolEvent`
  (`canvas/tool_input.rs:11`); pressure is 1.0 unless `StylusFeed` has samples. There is **no
  `PointerKind`** device classification in the live path (only an unrelated phase enum in
  `crates/automation`).
- **Pinch-zoom & rotate** exist (egui `zoom_delta`/`rotation_delta`; `wheel_nav.rs:602`,
  `rotate_view.rs:142`), gated by `enhanced_controls.*`. **Two-finger pan** is missing.
- **Long-press** already opens canvas context menus (egui press-and-hold -> secondary click); pen is
  excluded via `pen_hold_is_no_right_click` (`input/stylus.rs:479`). Any new long-press must respect
  this.
- **Stylus pressure/tilt**: `crates/tablet` (`Sample { pressure, tilt_x, tilt_y, rotation, eraser }`,
  `tablet/src/lib.rs:51`) -> `StylusFeed` -> `Stylus::pressure()`. Platform backends: macOS AppKit,
  Windows pointer + WinTab, X11 XI2. **Wayland not supported** (opens through Xwayland when a pen
  exists). Tilt/rotation reach `PenSample`; verify propagation into brush dynamics before claiming
  tilt support.
- **Keyboard shortcuts** remain supplementary: `input/shortcuts.rs::handle` (L469) ->
  `shortcut_dispatch::dispatch_pressed` (L238).

---

## 4. Proposed mobile architecture (mapping to the above)

```
  apps/photocraft-mobile  (new, L7)
    - New eframe/egui view (reuses wasm + native targets, no webview)
    - Responsive shell:                                  (new code)
        phone  -> full-screen canvas + bottom-sheet inspector + top bar
        tablet -> left tool rail + canvas + right inspector dock
    - All actions dispatch Session::execute(id, params)
  ---------------------------------------------------------------
  crates/ui-egui  (reused as a *library*, not forked)
    - TOOL_SECTIONS / visible_sections / slot_tool       (toolbar.rs)
    - PANELS + dock_body dispatch                        (panel_docking.rs / dock.rs)
    - options_bar / option_keys                          (panels, tools)
    - View / UiState / Panels / DockTabs serde state     (state.rs, dock.rs)
    - Services seam                                      (services.rs)
  ---------------------------------------------------------------
  crates/engine        Session::execute + CommandSpec registry
  crates/doc           Document/Layer model (unchanged)
  crates/tablet        pen input (unchanged)
  crates/{ops,compose,gpu,io,format,...}  (unchanged)
```

### Component -> mobile mapping

| Mobile surface | Reuses | New work |
|---|---|---|
| Home | `home_recent_ui`, recent-files prefs, recovery services | Project store (platform-backed); rename/duplicate/delete |
| New Document | `file.new`, `document.presets.*`, `document.recent.*`, `new_doc_ui` state | Mobile layout of the dialog |
| Open/Import | `FileDialogFn`, `screen_pick`, `ImportFn`, `file.open`/`file.place` | Format-filter sheet, photo-library bridge |
| Editor shell (top bar) | `View` state, history/`file.save` commands | Responsive bar; state preservation |
| Tools | `TOOL_SECTIONS`, `options_bar`, tool ids | Search UI; recents/favorites (new); touch flyout |
| Inspector | `PANELS`, `dock_body`, `properties_body` | Bottom-sheet (phone) / dock (tablet) host |
| Color | `color_panel_ui`, `swatches_ui`, `ToolState`, `cms` | Mobile picker layout |
| Complex workspaces | existing dialog states + `jobs` progress | Full-screen mobile shells |
| Save/Export | `file.save*`, Export As, `FormatChoices` | Mobile option sheet |
| Settings | `prefs.get/set`, 18 sections | Mobile settings list |
| Responsive | - | All of it (device-class detection, rail/dock vs sheet) |
| Input | `ToolEvent`, `zoom_delta`/`rotation_delta`, `tablet`, long-press | Two-finger pan; device classification |

### Non-negotiables carried from `AGENTS.md`

1. Everything is a command; use exact ids from `crates/ui-egui/src/menu_catalog.rs`.
2. No panics in non-test code; `unsafe` only in `crates/tablet`.
3. Layering enforced (`cargo xtask layers`); nothing below `ui-egui` may use egui/winit/rfd.
4. wasm gate: L0-L6 must build for `wasm32-unknown-unknown`.
5. UI is thin and data-driven; colours/radii from `theme::Tokens`.
6. No new dependencies that pull in a webview/JS UI framework.

---

## 5. Explicit scope note and gaps

**Honest scoping.** This document is the mapping deliverable. A *complete, production-oriented*
mobile shell for a 68-tool, 500+-command, 24-crate editor is a large multi-part effort that cannot be
fully implemented and **validated** in one pass here, because the workspace pins **external git
dependencies** (`craft-libs`, a forked `egui`/`winit`) that must be fetched and compiled, and because
touch/device behaviour cannot be exercised in this environment. Delivering it in verifiable slices -
each compiling and passing the repo's gates - is the only responsible path, starting with:

1. This mapping (done).
2. A responsive device-class + shell module that reuses `View`/`UiState` and the registry, behind a
   new app target, compiling clean.
3. Bottom-sheet/dock hosts over the existing `PANELS` + `dock_body`.
4. Two-finger pan + device classification in the canvas input path, keeping `pen_hold_is_no_right_click`.

**Confirmed non-existent (do not fake):** typed view models; `options_schema`; recent/favorite tools;
two-finger pan; recent-projects store; responsive/tablet layout code; tool toolbar reorder (the pref
`ToolbarCustomization.order` is persisted but never read - `panels/toolbar.rs:156`).

## Revision history

- 2026-11-07: initial mapping from read-only inspection of `f68e22702`.
