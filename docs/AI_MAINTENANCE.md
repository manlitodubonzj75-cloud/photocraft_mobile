# AI Maintenance Guide (token-efficient)

For future AI coding agents working on this PhotoCraft Android fork. Goal: minimum
token spend, minimum repository exploration, minimum repeated testing.

Read `MOBILE_ARCHITECTURE.md` for the full picture and `UPSTREAM_MAINTENANCE.md` for
the human workflow. This file is the quick-start for agents.

## 1. Architecture in one screen

- The mobile UI is an **additive egui shell** over the unchanged PhotoCraft engine.
- New code lives in ONE place: `crates/ui-egui/src/mobile/`
  (`mod.rs`, `device.rs`, `shell.rs`, `panels.rs`, `state.rs`, `chrome.rs`,
  `shell_tests.rs`).
- The shell is selected in `crates/ui-egui/src/app/frame.rs` `ui_body`: if
  `self.ui.mobile.enabled && !home`, call `panels::mobile_body(...)`; else desktop.
- Panels are hosted from existing code: `panels::dock_body`,
  `panels::toolbar`, `panels::options_bar`, `widgets::tab_strip::pill_tabs`.
- Commands go through the real seam `Session::execute(id, json_params)` -- IDs like
  `edit.undo`, `edit.redo`, `file.save`, plus `tool_memory::select` for tool choice.
- Runtime switch: `--mobile [phone|tablet]` in `apps/photocraft/src/launch_args.rs`,
  applied in `apps/photocraft/src/main.rs` to `UiState.mobile`.

That is the entire fork surface. You do NOT need to read the repository to understand it.

## 2. Minimum files to inspect before an upstream update

Only these, and only the parts that the diffs touch:

1. `git --no-pager diff --stat HEAD..upstream/main` (what changed).
2. The fork's touch points (see `UPSTREAM_MAINTENANCE.md` section 2):
   `crates/ui-egui/src/lib.rs`, `state.rs`, `app/frame.rs`, `panels.rs`,
   `panels/toolbar.rs`, `widgets.rs`, `apps/photocraft/src/launch_args.rs`, `main.rs`.
3. This doc and `MOBILE_ARCHITECTURE.md`.

Do not open the `mobile/` module unless a conflict directly involves it.

## 3. Recommended integration sequence

1. `git fetch upstream`.
2. `git --no-pager diff --stat HEAD..upstream/main` -- scan for touch points and for
   signature/command-ID changes (section 3 of `UPSTREAM_MAINTENANCE.md`).
3. `git switch -c sync/upstream-YYYYMMDD`.
4. `GIT_EDITOR=true git merge upstream/main`.
5. Resolve conflicts: keep upstream, re-apply the small mobile addition.
6. Validate (section 5). Commit. Stop.

## 4. Avoid unrelated failures and dead ends

- The suite currently has **26 failing ui-egui tests**, unrelated to mobile
  (i18n/font-render cluster). Do NOT investigate, fix, or debug them.
- The `photocraft` **test** binary does not compile here due to a pre-existing
  test-only issue (`apps/photocraft/src/external_svg.rs:1391`, `mkfifoat`). Do not
  try to fix it; use non-test `cargo check`.
- The wasm target is not installed -- do not attempt wasm gates.
- Always use `--offline`; deps are vendored under `~/.cargo/git/checkouts`.
- If a failure is clearly pre-existing or unrelated to your change, **stop and
  report it**; do not chase it.

## 5. Lightweight validation strategy

Run in this order and stop at the first meaningful failure:

```sh
cargo check --offline -p photocraft-ui-egui      # must pass
cargo test  --offline -p photocraft-ui-egui --lib mobile::   # must pass (26 tests)
cargo xtask layers                               # must pass
```

Only widen to broader tests if a targeted check fails or the merge touched shared
code in an unexpected way. Never run the full suite as a first step.

## 6. Completion criteria for an upstream update

The update is complete when ALL hold:

- `cargo check --offline -p photocraft-ui-egui` passes.
- `cargo test --offline -p photocraft-ui-egui --lib mobile::` passes (26/26).
- `cargo xtask layers` passes.
- No new failures are attributable to the merge; any failure observed is confirmed
  pre-existing via `KNOWN_ISSUES.md` or a diff against the pre-merge state.
- The mobile touch points still compile against upstream APIs (no visibility/command
  regressions).

If any criterion fails after one focused fix attempt, stop and report.

## 7. Reusable upstream-sync prompt

Copy-paste for the next agent:

```
Task: sync this PhotoCraft Android fork with upstream.

Read docs/AI_MAINTENANCE.md, docs/MOBILE_ARCHITECTURE.md, and
docs/UPSTREAM_MAINTENANCE.md first. Do NOT read the whole repository.

Steps:
1. git fetch upstream
2. git --no-pager diff --stat HEAD..upstream/main  (report what changed)
3. Branch: git switch -c sync/upstream-YYYYMMDD
4. GIT_EDITOR=true git merge upstream/main
5. Resolve conflicts by keeping upstream and re-applying the small mobile
   additions listed in docs/UPSTREAM_MAINTENANCE.md section 2/5.
6. Validate ONLY: cargo check --offline -p photocraft-ui-egui;
   cargo test --offline -p photocraft-ui-egui --lib mobile::;
   cargo xtask layers
7. Do not investigate the 26 pre-existing failing tests or the mkfifoat test-only
   compile error (see docs/KNOWN_ISSUES.md). Stop and report if a problem is unrelated.
8. Report: what changed upstream, conflicts resolved, validation output, remaining risk.
```
