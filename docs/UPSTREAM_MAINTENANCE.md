# Upstream Maintenance Guide (PhotoCraft Android fork)

How to pull and integrate future updates from the original PhotoCraft repository
without breaking or rewriting the mobile UI. Preserve the fork; adapt to upstream.

For AI agents, use the leaner `AI_MAINTENANCE.md` instead.

## 1. Repository remotes

One-time setup (only if `upstream` is missing):

```sh
git remote -v
git remote add upstream https://github.com/storytold/photocraft
git fetch upstream
```

Convention used below: `upstream` = original repo, `origin` = this fork,
`main` = the fork's default branch (adjust if different).

## 2. Files most likely to conflict

These are the fork's touch points on upstream code. Conflicts cluster here.

| File | Why it conflicts |
|------|------------------|
| `crates/ui-egui/src/lib.rs` | One added line (`pub mod mobile;`). Trivial conflict, keep both sides. |
| `crates/ui-egui/src/state.rs` | Added `mobile: MobileUiState` field + default. Keep the field. |
| `crates/ui-egui/src/app/frame.rs` | Mobile branch inside `ui_body`. Keep the branch, re-apply on top of upstream body. |
| `crates/ui-egui/src/panels.rs` | Added `mobile_body(...)` and a visibility widening. Keep both; re-add if upstream rewrote the region. |
| `crates/ui-egui/src/panels/toolbar.rs` | `visible_sections` visibility widening. Keep `pub(crate)`. |
| `crates/ui-egui/src/widgets.rs` | `tab_strip` visibility widening. Keep `pub(crate)`. |
| `apps/photocraft/src/launch_args.rs` | `--mobile` flag + `PerProcess::Mobile`; the `PerProcess::FLAGS` array length is asserted in a test. |
| `apps/photocraft/src/main.rs` | Applies the mobile flag after app construction. |

Everything under `crates/ui-egui/src/mobile/` is fork-only and never conflicts
(unless upstream happens to add a `mobile` module -- unlikely).

## 3. Breaking-change signals to watch for

When reviewing the upstream diff before merging, treat these as high risk:

- Signature or visibility change of `panels::dock_body`, `panels::toolbar`,
  `panels::options_bar`, `panels::visible_sections`, or `widgets::tab_strip`.
  The mobile shell calls these directly.
- Renamed/removed command IDs: `edit.undo`, `edit.redo`, `file.save`, panel-group
  switch commands, or `tool_memory::select`.
- `UiState` serialization changes (field renames, serde attributes) that affect the
  persisted `mobile` object.
- `egui`/`winit` fork bumps under `~/.cargo/git/checkouts` -- interface churn can
  break the mobile chrome's widget calls.
- `launch_args` flag parsing changes, especially the `PerProcess::FLAGS` length
  assertion test.

## 4. Step-by-step update workflow

```sh
# From the repository root (pcm/photocraft). Work on a branch, never on main.

# 0. Clean starting point
git --no-optional-locks status

# 1. Fetch upstream
git fetch upstream

# 2. See what is coming before merging
git --no-pager log --oneline HEAD..upstream/main
git --no-pager diff --stat HEAD..upstream/main

# 3. Create a work branch
git switch -c sync/upstream-YYYYMMDD

# 4. Merge (or rebase; merge is safer for a fork with local commits)
GIT_EDITOR=true git merge upstream/main

# 5. Resolve conflicts (see section 5), then validate (section 6).

# 6. Commit the merge
GIT_EDITOR=true git commit
```

Prefer `git merge` over `git rebase` for upstream sync: it preserves the fork's
history and makes rollback a single `git reset --hard` / `git revert -m 1`.

## 5. Resolving typical conflicts

Rule: **keep upstream code, re-apply the small mobile addition on top.** Never
rewrite a working upstream region wholesale to match the fork.

- `lib.rs`: accept both -- upstream lines plus `pub mod mobile;`.
- `state.rs`: keep the upstream struct, re-add the `mobile` field and its default.
- `frame.rs`: keep upstream `ui_body` body, re-insert the
  `if self.ui.mobile.enabled && !home { panels::mobile_body(...) } else { ... }` branch.
- `panels.rs` / `toolbar.rs` / `widgets.rs`: keep upstream implementations, re-apply
  the added `mobile_body` function and the `pub(crate)` visibility widenings.
- `launch_args.rs`: keep the `--mobile` flag and `PerProcess::Mobile`; if upstream
  changed `FLAGS` length, adjust the array and the length assertion test to match
  the new total (do not drop `--mobile`).
- `main.rs`: keep the post-construction block that sets
  `app.ui.mobile.enabled` / `force_class`.

If a conflict is not in the list above, it is probably an unrelated upstream change.
Resolve it by preferring upstream, and do not refactor.

## 6. Validation after an update

Run the cheap, targeted checks first (see `AI_MAINTENANCE.md` section 5):

```sh
cargo check --offline -p photocraft-ui-egui
cargo test  --offline -p photocraft-ui-egui --lib mobile::
cargo xtask layers
```

Only if the mobile tests pass and something still looks wrong, widen the net.
Do not run the full suite by default (it is slow and currently red for unrelated
reasons; see `KNOWN_ISSUES.md`).

Acceptance criteria for the update: mobile compiles, `mobile::` tests pass,
`xtask layers` passes, and no new failures attributable to the merge.

## 7. Safe rollback

If the merge goes wrong, abandon the branch instead of unpicking changes:

```sh
# On the sync branch
git merge --abort            # if conflict resolution is still in progress

# Or, if already committed on the branch:
git switch main
git branch -D sync/upstream-YYYYMMDD

# Or revert the merge commit on main (if pushed):
git revert -m 1 <merge-commit-sha>
```

Because all mobile work lives in isolated new files plus a handful of small
additions, deleting or reverting the sync branch returns to the known-good fork
state without collateral damage.

## 8. Maintenance principles

- Preserve the existing mobile UI and its behavior.
- Minimize modifications to upstream code; keep edits additive and isolated.
- No unrelated refactoring.
- Prefer small compatibility fixes over rewrites.
- Do not rescan the whole repository; use the diffs and this doc.
- Never run the full test suite by default.
- Stop and report when a problem is unrelated to the requested update.
