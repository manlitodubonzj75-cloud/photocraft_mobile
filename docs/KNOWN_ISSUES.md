# Known Issues and Test Baseline

Recorded status of the test suite for this fork. Do not treat any failure here as
caused by the mobile UI without evidence; equally, do not claim a failure is
pre-existing unless the evidence below says so.

## 1. Baseline

Full ui-egui lib suite (`cargo test --offline -p photocraft-ui-egui --lib`):

- **5,414 passed / 26 failed / 22 ignored** (5,440 total).
- The 26 failures are in an i18n / font-rendering cluster and are **not** in the
  mobile module.

**Verification status of the 26:** they are **unverified as pre-existing at the
commit level**. They were observed during this session and are very likely
environment-related (font/i18n rendering), but no baseline run on a pristine
upstream checkout has been done. Do not assert "pre-existing" for these 26 without
that evidence. The only failure confirmed pre-existing is the `mkfifoat` item in
section 3 (its file is unmodified by this fork).

Full error messages were not captured (output truncated). Only names (below) and
the section 3 case are documented.

## 2. The 26 failing tests (names)

```
i18n::live_tests::a_control_request_changes_the_drawing_language_in_the_same_frame
i18n::live_tests::an_explicit_language_survives_restart_on_a_different_system_language
i18n::live_tests::arabic_labels_are_laid_out_right_to_left
i18n::live_tests::arabic_names_are_laid_out_right_to_left
i18n::live_tests::dynamic_brush_sections_draw_in_the_selected_language
i18n::live_tests::first_launch_renders_the_system_language_without_overwriting_auto
i18n::live_tests::korean_glyphs_remain_available_after_repeated_language_switches
i18n::live_tests::preferences_preview_does_not_commit_until_applied_and_survives_reload
input::hold_keys::tests::space_while_editing_type_is_not_the_hand_tool
panels::brush_panel::tests::brush_sections_paint_simplified_chinese_labels
panels::layer_row_ui::edit_tests::text_thumbnail_opens_inline_editing_and_typing_is_one_undo_step
panels::layer_row_ui::tests::collapse_all_groups_from_the_panel_menu_and_it_is_saved
panels::layer_row_ui::tests::long_names_never_run_under_the_indicators
panels::layer_row_ui::tests::new_group_items_in_the_panel_menu_open_the_dialog
panels::layer_row_ui::tests::the_fx_triangle_hides_and_shows_the_effects_rows
panels::panel_docking::tests::floating_group_titles_are_opaque_in_studio_themes
tools::type_tool::canvas_tests::empty_ime_events_never_delete_the_selection
tools::type_tool::font_commit_tests::selected_fonts_commit_pattern_0
tools::type_tool::font_commit_tests::selected_fonts_commit_pattern_2
tools::type_tool::font_commit_tests::selected_fonts_commit_pattern_3
tools::type_tool::font_picker_tests::localized_font_names_label_the_menu_but_pick_the_family
tools::type_tool::ime_tests::actual_font_search_owns_stock_input_without_a_speculative_custom_request
tools::type_tool::ime_tests::actual_screen_picker_modal_retires_the_type_fence_and_rearms_it
tools::vector_ui::path_selection_tests::path_overlay_drawn_for_non_vector_tools
widgets::swap_arrow::tests::rendered_glyph_has_a_left_head_and_a_lower_right_head
widgets::tests::value_field_digits_ink_is_centred_and_level_with_the_unit
```

Cluster: i18n live rendering, brush/type font glyphs and centering, layer-row UI
layout, panel docking opacity, vector overlay, swap-arrow glyph. All consistent
with a font/rendering environment difference rather than mobile code.

## 3. Confirmed pre-existing (with evidence)

- **`apps/photocraft/src/external_svg.rs:1391` -- `cannot find function mkfifoat in
  module rustix::fs`.** This is a test-only compile failure: it sits in a
  `#[cfg(test)]` block, and `external_svg.rs` is unmodified by this fork (evidence:
  the file is untouched in the session's diff). It blocks compiling the
  `photocraft` test binary, so `cargo test -p photocraft --bin photocraft` cannot run
  here. Non-test `cargo check -p photocraft` passes.

## 4. Mobile tests (all passing, 26/26)

Targeted run: `cargo test --offline -p photocraft-ui-egui --lib mobile::`
-> **26 passed, 0 failed**.

Coverage:

- `mobile::device` (6 tests): phone/tablet classification, orientation, invalid sizes.
- `mobile::shell` (9 tests): region model, `RegionPane` metrics, `OpenPanels` clamp.
- `mobile::state` (5 tests): defaults, serde round-trip.
- `mobile::shell_tests` (6 egui_kittest tests): phone/tablet arrangements,
  bottom-nav open/close, undo gating, class-change state preservation,
  toggle-off restores desktop.

## 5. Validation commands already executed (this session)

| Command | Result |
|---------|--------|
| `cargo check --offline -p photocraft-engine` | pass |
| `cargo check --offline -p photocraft-ui-egui` | pass |
| `cargo check --offline -p photocraft` | pass |
| `cargo clippy --offline -p photocraft-ui-egui` | pass (mobile module clean) |
| `cargo xtask layers` | pass (31 crates, no violations) |
| `cargo test --offline -p photocraft-ui-egui --lib mobile::` | pass (26/26) |
| `cargo test --offline -p photocraft-ui-egui --lib` | 5,414 pass / 26 fail / 22 ignored |
| `cargo test -p photocraft --bin photocraft launch_args` | blocked by section 3 compile error |

Environment: Rust 1.99.1; builds run with `--offline` (deps vendored under
`~/.cargo/git/checkouts`); wasm target not installed.

## 6. Policy going forward

- Never run the full suite by default; use the targeted `mobile::` filter.
- Do not attempt to fix the items above unless explicitly tasked.
- If a new failure appears after a change, diff against this baseline and check
  whether it matches a known pre-existing entry before investigating further.
