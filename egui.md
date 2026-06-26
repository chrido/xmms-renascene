# egui Frontend Plan

## Goal

Add an `egui` frontend alongside the existing GTK frontend, step by step, without making the egui build depend on GTK, GIO, GLib, GDK, or GTK-only UI modules.

The first egui milestone should support the main user-facing XMMS windows:

- Main player window
- Playlist window/panel
- Equalizer window/panel
- Preferences dialog/window

The skin editor is explicitly out of scope for the first egui frontend and should remain GTK-only for now.

## Motivation

The project is moving toward a clean separation between application logic and concrete UI toolkits. GTK is currently the mature desktop frontend, but future frontends should be able to reuse the same app/controller/view-model/rendering logic without linking GTK.

`egui` is a good next frontend because it is Rust-native, portable, immediate-mode, and can run on multiple desktop/windowing backends through `eframe`. It can also provide a stepping stone toward more portable UI architecture before native mobile frontends are attempted.

The key architectural rule for this work:

> The egui frontend must compile without depending on GTK-related crates or modules.

That means an egui-only build should eventually pass something like:

```bash
cargo check --no-default-features --features egui-ui
```

without compiling `gtk`, `gio`, `glib`, `gdk`, GTK file dialogs, GTK CSS, or GTK skin-editor code.

## Source Reference

`egui` project:

- https://github.com/emilk/egui

Expected crate choices:

- `egui` for immediate-mode UI widgets and drawing primitives.
- `eframe` for a desktop app runner/window integration.
- Optional later: `egui_extras`, `rfd`, or platform-specific file dialog crates, but avoid adding them until needed.

## High-Level Approach

Work incrementally:

1. Make frontend dependencies feature-gated so GTK is not compiled for egui-only builds.
2. Keep the existing GTK frontend working as the default.
3. Add explicit frontend selection via CLI, e.g. `--frontend gtk` or `--frontend egui`; if unspecified, default to GTK.
4. Add a minimal egui app shell.
5. Reuse app-layer commands, effects, panel models, view models, config, playlist, equalizer, and playback abstractions.
6. If egui needs behavior currently implemented inside GTK modules, extract that behavior into a shared frontend-neutral file first; do not import GTK code from egui.
7. Initially render skinned UI via the existing Cairo renderer converted to egui textures, rather than rewriting all skin drawing directly in egui.
8. Add native egui controls around or over the rendered skin where appropriate.
9. Implement the main player, playlist, equalizer, and preferences UI.
10. Compare GTK and egui screenshots regularly to catch layout/rendering regressions.
11. Leave the skin editor behind `gtk-ui` only.

## Non-Goals for the First Milestone

- Do not port the skin editor to egui.
- Do not remove GTK.
- Do not rewrite the Cairo renderer unless necessary.
- Do not implement mobile packaging yet.
- Do not make egui pixel-perfect before the basic frontend is functional.
- Do not duplicate core playlist/playback/equalizer logic in egui.
- Do not copy-paste logic from GTK modules into egui modules. Extract shared logic into `app`, `render`, `skin`, or another frontend-neutral module first.

---

# Architecture Target

## Current desired frontend split

```text
src/
  app/                 # frontend-neutral commands/effects/controller/view models
  playback/            # backend abstraction + GStreamer adapter
  render/              # skin/Cairo renderer and future draw-command model
  ui/
    gtk/               # GTK frontend only
    egui/              # egui frontend only
```

## Feature target

```toml
[features]
default = ["gtk-ui", "gstreamer-backend"]
gtk-ui = ["dep:gtk", ...]
egui-ui = ["dep:egui", "dep:eframe"]
gstreamer-backend = ["dep:gstreamer", "dep:gstreamer-pbutils"]
mobile-ui = []
```

Eventually:

```bash
cargo check --no-default-features --features egui-ui
cargo check --no-default-features --features gtk-ui,gstreamer-backend
cargo test --all-features
```

## Module target

```text
src/ui/egui/
  mod.rs
  app.rs              # eframe app implementation and lifecycle
  runtime.rs          # AppEffect interpreter for egui
  main_player.rs      # main player panel/window
  playlist.rs         # playlist UI
  equalizer.rs        # equalizer UI
  preferences.rs      # preferences dialog/window
  skin_texture.rs     # Cairo/image conversion into egui textures
  layout.rs           # egui window placement, scaling, panel docking helpers
  input.rs            # pointer/key/wheel translation into AppCommand
  screenshots.rs      # egui screenshot capture helpers, if frontend-specific helpers are needed
```

Shared logic extracted from GTK must live outside `src/ui/gtk/`, for example:

```text
src/app/              # commands, controller, effects, view models, preference mappings
src/render/           # renderer, screenshot helpers, draw commands, pixel comparison helpers
src/skin/             # skin geometry/layout primitives
src/ui/shared/        # only if a helper is UI-frontend-neutral but does not fit app/render/skin
```

GTK-only modules should remain under:

```text
src/ui/gtk/
src/ui/file_info.rs       # until migrated/gated
src/ui/style.rs           # GTK CSS; must not compile in egui-only builds
src/skineditor.rs         # GTK-only for now
```

---

# Progress Checklist

## Phase 0: Audit GTK dependency leakage

- [x] Search all GTK-related imports/usages:
  - [x] `gtk`
  - [x] `gio`
  - [x] `glib`
  - [x] `gdk`
  - [x] `gtk::prelude`
- [x] Identify all modules that currently force GTK compilation.
- [x] Identify public APIs that expose GTK types.
- [x] Identify tests that require GTK initialization.
- [x] Document which modules must be gated behind `gtk-ui`.
- [x] Confirm `src/skineditor.rs` is GTK-only and should stay gated behind `gtk-ui`.
- [x] Validation: no behavior changes.
- [x] Commit audit notes if code/docs changed.

### Audit notes

Current GTK-related references are concentrated in `src/ui.rs`, `src/ui/file_info.rs`, `src/ui/style.rs`, and `src/ui/gtk/runtime.rs`. The skin editor is imported by `src/ui.rs` and remains GTK-only for the egui milestone. Non-UI platform/backend references also exist in `src/player.rs`, `src/mpris.rs`, and `src/podcast.rs`; those are not GTK UI modules but must still be considered when compiling no-default-feature targets. Public GTK-facing UI APIs currently flow through `src/ui.rs` and `src/e2e.rs`, so egui-only builds must gate those modules or provide frontend-neutral alternatives. Tests under `ui::tests` initialize GTK and remain behind the GTK feature path.

## Phase 1: Make frontend dependencies feature-gated

### Objective

Make it possible to compile non-GTK frontends without GTK-related dependencies.

### Tasks

- [x] Make GTK-related dependencies optional in `Cargo.toml`:
  - [x] `gtk`
  - [x] any direct `gio` dependency if added later
  - [x] any direct `glib` dependency if added later
- [x] Update `gtk-ui` feature to include GTK dependencies.
- [x] Add optional egui dependencies:
  - [x] `egui`
  - [x] `eframe`
- [x] Add `egui-ui` feature using egui dependencies.
- [x] Keep default features as GTK desktop:
  - [x] `gtk-ui`
  - [x] `gstreamer-backend`
- [x] Gate GTK-only modules with `#[cfg(feature = "gtk-ui")]`.
- [x] Gate `src/skineditor.rs` with `#[cfg(feature = "gtk-ui")]` or move it under GTK-only frontend structure.
- [x] Ensure the library can still compile with default features.
- [x] Add a temporary compile check:

```bash
cargo check --no-default-features --features egui-ui
```

- [x] Validation:

```bash
cargo check
cargo check --no-default-features --features egui-ui
cargo test --lib --bins
cargo clippy --all-targets
```

- [x] Commit: `Gate GTK dependencies behind gtk-ui feature`

## Phase 2: Extend repo tool for frontend screenshot diffs

### Objective

Add repo-tool support to capture screenshots from both GTK and egui frontends and generate a visual diff image. This should be one of the first implementation steps so every later egui UI task can use the same comparison workflow.

### Tasks

- [x] Locate the existing repo tool entrypoint and command structure.
- [x] Add a frontend screenshot-diff command, for example:

```bash
repo-tool frontend-screenshot-diff \
  --scenario main-player-default \
  --gtk-output target/screenshots/gtk-main.png \
  --egui-output target/screenshots/egui-main.png \
  --diff-output target/screenshots/diff-main.png
```

- [x] Support named screenshot scenarios:
  - [x] `main-player-default`
  - [x] `main-player-shaded`
  - [x] `playlist-default`
  - [x] `playlist-with-selection`
  - [x] `equalizer-default`
  - [x] `equalizer-non-default`
  - [x] `preferences-default`
- [x] Add options for:
  - [x] output directory;
  - [x] image tolerance;
  - [x] fail-on-diff threshold;
  - [x] keeping intermediate GTK/egui screenshots;
  - [x] updating reference images intentionally.
- [x] Reuse existing GTK screenshot code where possible, but if that code lives in GTK-only modules and egui needs the same setup logic, extract the shared state/scenario setup into frontend-neutral code first.
- [x] Add shared screenshot scenario builders outside GTK-only modules, for example:

```text
src/app/screenshot_scenarios.rs
src/render/screenshot_compare.rs
```

- [x] Add image diff generation:
  - [x] load GTK/reference screenshot;
  - [x] load egui screenshot;
  - [x] compare dimensions;
  - [x] compute per-pixel difference;
  - [x] write a diff heatmap/image;
  - [x] print summary statistics such as changed pixels and max delta.
- [x] Make the command work even before egui rendering is complete by allowing missing-egui output to produce a clear actionable error.
- [x] Add tests for the image comparison/diff helper using small synthetic images.
- [x] Document the command in this plan and any repo-tool help text.
- [x] Validation:

```bash
cargo test --lib --bins
cargo test --test render
# plus the repo-tool command once available:
# repo-tool frontend-screenshot-diff --scenario main-player-default --output-dir target/screenshots
```

- [x] Commit: `Add repo tool frontend screenshot diff command`

## Phase 3: Add egui module skeleton

### Objective

Create a dedicated egui frontend namespace without implementing UI behavior yet.

### Tasks

- [ ] Add `src/ui/egui/mod.rs`.
- [ ] Add `src/ui/egui/app.rs`.
- [ ] Add `src/ui/egui/runtime.rs`.
- [ ] Add `src/ui/egui/main_player.rs`.
- [ ] Add `src/ui/egui/playlist.rs`.
- [ ] Add `src/ui/egui/equalizer.rs`.
- [ ] Add `src/ui/egui/preferences.rs`.
- [ ] Add `src/ui/egui/skin_texture.rs`.
- [ ] Add `src/ui/egui/layout.rs`.
- [ ] Add `src/ui/egui/input.rs`.
- [ ] Export egui module only when `egui-ui` is enabled.
- [ ] Ensure no egui module imports GTK.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo check
```

- [ ] Commit: `Add egui frontend module skeleton`

## Phase 4: Add frontend selection CLI and an egui executable entrypoint

### Objective

Allow launching the egui frontend without disturbing the existing GTK default. Users should be able to choose the frontend explicitly from the CLI.

### Options

Preferred first option:

```text
src/bin/xmms-egui.rs
```

Also add frontend selection to the main CLI:

```bash
xmms-rs --frontend gtk
xmms-rs --frontend egui
```

If `--frontend` is unspecified, default to GTK for compatibility.

### Tasks

- [ ] Add a frontend selection enum in a frontend-neutral module, for example `app::preview::FrontendKind` or `app::frontend::FrontendKind`.
- [ ] Add CLI parsing for `--frontend gtk`.
- [ ] Add CLI parsing for `--frontend egui`.
- [ ] Reject unknown frontend values with a clear error.
- [ ] Default to GTK when `--frontend` is unspecified.
- [ ] Gate `--frontend egui` execution behind `egui-ui`; return a clear error if the binary was built without egui support.
- [ ] Add tests for CLI frontend parsing:
  - [ ] unspecified frontend defaults to GTK;
  - [ ] `--frontend gtk` selects GTK;
  - [ ] `--frontend egui` selects egui;
  - [ ] invalid frontend is rejected.
- [ ] Add `src/bin/xmms-egui.rs` gated by `egui-ui` if a separate binary remains useful.
- [ ] Parse the same preview/session options where practical, reusing `app::preview::PreviewOptions`.
- [ ] Create an `eframe` native app runner.
- [ ] Instantiate egui app state from `AppState::default()` or loaded config.
- [ ] Keep GTK behavior unchanged when no frontend is specified.
- [ ] Ensure `xmms-egui` is only built when `egui-ui` is enabled if necessary.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo run --no-default-features --features egui-ui --bin xmms-egui -- --help
cargo run --features gtk-ui --bin xmms-rs -- --frontend gtk --gtk-smoke
cargo run --features egui-ui --bin xmms-rs -- --frontend egui --help
```

- [ ] Commit: `Add frontend CLI selection and egui binary`

## Phase 5: Create egui app state and runtime

### Objective

Wire egui to the frontend-neutral app layer.

### Tasks

- [ ] Define `EguiApp` in `src/ui/egui/app.rs`.
- [ ] Store `AppController` inside `EguiApp`.
- [ ] Store frontend-only egui state:
  - [ ] open/closed preferences dialog
  - [ ] selected preferences page
  - [ ] texture cache
  - [ ] scale factor
  - [ ] panel layout preferences
- [ ] Implement `eframe::App` for `EguiApp`.
- [ ] Add `EguiRuntime` or effect interpreter in `runtime.rs`.
- [ ] Interpret basic `AppEffect` values:
  - [ ] `QueueRender`
  - [ ] `ShowError`
  - [ ] `ShowMessage`
  - [ ] playback effects as no-op initially if backend not wired
  - [ ] dialog effects as no-op/log initially
- [ ] Add a helper to dispatch commands:

```rust
fn dispatch(&mut self, command: impl Into<AppCommand>)
```

- [ ] Ensure no GTK imports.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo test --lib
```

- [ ] Commit: `Wire egui app to app controller`

## Phase 6: Reuse skin rendering through egui textures

### Objective

Render the existing skinned UI into egui without duplicating all skin drawing code, and establish screenshot comparison as a recurring safety check.

### Strategy

Use the current Cairo renderer to render into an image buffer, then upload that buffer as an `egui::TextureHandle`.

### Tasks

- [ ] Add `src/ui/egui/skin_texture.rs` helpers.
- [ ] Render main player state with existing render functions into a Cairo image surface.
- [ ] Convert Cairo image data into `egui::ColorImage`.
- [ ] Upload/update `egui::TextureHandle`.
- [ ] Add texture invalidation when relevant `AppEffect::QueueRender` is received.
- [ ] Handle scale factor cleanly.
- [ ] Preserve render parity by not changing renderer behavior.
- [ ] Add shared screenshot helpers outside GTK-only modules if egui needs behavior currently available only through GTK preview/screenshot code.
- [ ] Add a deterministic GTK reference screenshot path for main player, playlist, equalizer, and preferences states where practical.
- [ ] Add an egui screenshot capture path for the same states.
- [ ] Add a pixel/image comparison helper with documented tolerance.
- [ ] Compare egui screenshots against GTK/reference screenshots after each visible UI milestone.
- [ ] Store/update reference images only intentionally.
- [ ] Add tests for conversion helpers if practical without requiring a window.
- [ ] Validation:

```bash
cargo test --test render
cargo check --no-default-features --features egui-ui
# when UI screenshot capture is implemented:
# cargo test --test egui_screenshots
```

- [ ] Commit: `Render skin surfaces as egui textures`

## Phase 7: Implement main player egui UI

### Objective

Show and interact with the main XMMS player in egui.

### Tasks

- [ ] Add `main_player.rs` UI function, e.g.:

```rust
pub fn show_main_player(ui: &mut egui::Ui, app: &mut EguiApp)
```

- [ ] Build from `main_player_view_model(&AppState)`.
- [ ] Display the rendered main player skin texture.
- [ ] Add egui hit testing for main player controls:
  - [ ] Play
  - [ ] Pause
  - [ ] Stop
  - [ ] Previous
  - [ ] Next
  - [ ] Eject/open file effect
  - [ ] Shuffle
  - [ ] Repeat
  - [ ] Volume slider
  - [ ] Balance slider
  - [ ] Seek slider
- [ ] Translate interactions into hierarchical commands:
  - [ ] `PlayerCommand`
  - [ ] `AudioCommand`
  - [ ] `PlaylistCommand`
  - [ ] `PanelCommand`
- [ ] Handle keyboard shortcuts for main controls where straightforward.
- [ ] Add smoke tests for command translation helpers.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo test --lib --bins
```

- [ ] Commit: `Implement egui main player UI`

## Phase 8: Implement playlist egui UI

### Objective

Show and interact with the playlist in egui.

### Tasks

- [ ] Add `playlist.rs` UI function.
- [ ] Build from `playlist_view_model(&AppState)`.
- [ ] Display playlist rows.
- [ ] Show current row and selected rows.
- [ ] Support row click selection.
- [ ] Support double-click or enter to play selected/current row.
- [ ] Support scroll wheel behavior.
- [ ] Support playlist menu actions via `PlaylistCommand`.
- [ ] Support sort/reverse/randomize commands.
- [ ] Implement open/add/load/save effects as no-op/log initially or use a non-GTK file dialog only if already chosen.
- [ ] Avoid GTK file chooser APIs.
- [ ] Add unit tests for playlist command translation.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo test --lib --bins
```

- [ ] Commit: `Implement egui playlist UI`

## Phase 9: Implement equalizer egui UI

### Objective

Show and interact with the equalizer in egui.

### Tasks

- [ ] Add `equalizer.rs` UI function.
- [ ] Build from `equalizer_view_model(&AppState)`.
- [ ] Display equalizer skin texture or egui-native controls.
- [ ] Support active toggle.
- [ ] Support auto toggle.
- [ ] Support preamp slider.
- [ ] Support ten band sliders.
- [ ] Dispatch `EqualizerCommand` values.
- [ ] Interpret resulting effects to update backend equalizer once backend wiring is active.
- [ ] Add tests for equalizer slider command translation.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo test --lib --bins
cargo test --test render
```

- [ ] Commit: `Implement egui equalizer UI`

## Phase 10: Implement preferences egui dialog

### Objective

Port the main preferences dialog to egui while leaving skin editor GTK-only.

### Scope

Include preferences pages currently useful for the main app:

- Audio/output settings where backend-neutral or GStreamer-backed.
- Options.
- Fonts/title formatting.
- Visualization settings.
- Playlist behavior settings.
- Window/panel/docking/shading settings.

Exclude:

- Skin editor UI.
- GTK CSS/style-specific preferences.
- GTK-only file chooser details.

### Tasks

- [ ] Add `preferences.rs` dialog/window function.
- [ ] Define egui-only `PreferencesUiState` if needed.
- [ ] Use `AppState.config` as source of truth.
- [ ] Update config through `AppController` commands where commands exist.
- [ ] Add missing app commands for preferences that are not yet covered.
- [ ] Support reset/apply/save behavior consistently with GTK.
- [ ] Reuse pure helpers from `app::view_model` for title formatting preview.
- [ ] Keep GTK-specific controls out.
- [ ] Add tests for any newly extracted preference mapping logic.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo test --lib --bins
```

- [ ] Commit: `Implement egui preferences dialog`

## Phase 11: Backend and effects integration for egui

### Objective

Make egui use the same effect model for playback and state updates.

### Tasks

- [ ] Decide whether egui first milestone requires live playback.
- [ ] If yes, instantiate the `PlaybackBackend` implementation under `gstreamer-backend`.
- [ ] Interpret playback effects:
  - [ ] `StartPlaybackUri`
  - [ ] `ResumePlayback`
  - [ ] `PausePlayback`
  - [ ] `StopPlayback`
  - [ ] `SeekPlayback`
  - [ ] `SetBackendVolume`
  - [ ] `SetBackendBalance`
  - [ ] `SetBackendEqualizer`
- [ ] Poll backend events in egui update loop.
- [ ] Dispatch playback events into `AppController::handle_playback_event`.
- [ ] Ensure egui-only builds can still compile without `gstreamer-backend` if desired.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo check --no-default-features --features egui-ui,gstreamer-backend
cargo test --lib --bins
```

- [ ] Commit: `Wire egui runtime playback effects`

## Phase 12: File dialogs without GTK

### Objective

Handle file/directory open/save effects without GTK.

### Tasks

- [ ] Decide whether to use a cross-platform non-GTK dialog crate such as `rfd`.
- [ ] Add it as an optional dependency only under `egui-ui` if chosen.
- [ ] Implement add files.
- [ ] Implement add directory.
- [ ] Implement playlist load/save.
- [ ] Implement equalizer preset load/save if needed.
- [ ] Implement skin import selection if needed.
- [ ] Ensure no GTK file chooser code is referenced.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo test --lib --bins
```

- [ ] Commit: `Implement non-GTK file dialogs for egui`

## Phase 13: Screenshot comparison against GTK/reference output

### Objective

Continuously compare egui output with GTK/reference screenshots to catch visual regressions in the main player, playlist, equalizer, and preferences UI.

### Tasks

- [ ] Define the screenshot scenarios that must be compared:
  - [ ] default main player;
  - [ ] shaded main player;
  - [ ] playlist visible with selected/current rows;
  - [ ] equalizer visible with non-default sliders;
  - [ ] preferences dialog on each implemented egui preferences page.
- [ ] Extract any GTK-only screenshot setup logic into shared frontend-neutral helpers before egui uses it.
- [ ] Add a GTK/reference screenshot capture path for the selected scenarios.
- [ ] Add an egui screenshot capture path for the selected scenarios.
- [ ] Add an image comparison helper with configurable tolerance.
- [ ] Add a command or test target that runs screenshot comparisons.
- [ ] Document how to intentionally refresh reference screenshots.
- [ ] Run screenshot comparison after each visible egui UI phase:
  - [ ] after main player;
  - [ ] after playlist;
  - [ ] after equalizer;
  - [ ] after preferences.
- [ ] Validation:

```bash
cargo test --test render
# once implemented:
# cargo test --test egui_screenshots
```

- [ ] Commit: `Add egui screenshot comparison checks`

## Phase 14: Egui smoke tests and CI checks

### Objective

Prevent regressions where egui starts depending on GTK accidentally.

### Tasks

- [ ] Add a documented local check:

```bash
cargo check --no-default-features --features egui-ui
```

- [ ] Add a stricter check if feasible:

```bash
cargo tree --no-default-features --features egui-ui | grep -E 'gtk|gio|glib|gdk'
```

and verify no GTK stack appears.

- [ ] Add CI job or script for egui-only compile.
- [ ] Add smoke test for constructing `EguiApp` if possible without a native window.
- [ ] Add smoke test for main player command translation.
- [ ] Add smoke test for preferences config mutation.
- [ ] Validation:

```bash
cargo check --no-default-features --features egui-ui
cargo test --lib --bins
cargo clippy --all-targets
```

- [ ] Commit: `Add egui frontend smoke checks`

---

# Acceptance Criteria for First egui Milestone

The first egui milestone is complete when:

- [ ] Existing GTK frontend remains the default and still works.
- [ ] Main CLI supports `--frontend gtk` and `--frontend egui`.
- [ ] Main CLI defaults to GTK when `--frontend` is unspecified.
- [ ] Repo tool can capture GTK and egui screenshots for named scenarios and write a diff image.
- [ ] `cargo check --no-default-features --features egui-ui` succeeds.
- [ ] The egui build does not compile or depend on GTK/GIO/GLib/GDK.
- [ ] An egui binary or frontend entrypoint launches.
- [ ] Any logic needed by egui from GTK has been extracted into shared frontend-neutral modules first.
- [ ] Main player UI is visible.
- [ ] Playlist UI is visible.
- [ ] Equalizer UI is visible.
- [ ] Preferences dialog/window is visible and can update config.
- [ ] Skin editor remains GTK-only and is not part of egui.
- [ ] Shared app commands/effects/view models are used instead of duplicating logic.
- [ ] GTK/reference screenshots and egui screenshots are compared for the implemented UI states.
- [ ] Screenshot comparison differences are reviewed and either fixed or intentionally accepted with updated references.
- [ ] Render parity tests still pass.
- [ ] Existing GTK tests still pass.

---

# Recommended Initial PR Sequence

## PR 1: Dependency and module gating

- [ ] Optionalize GTK dependencies.
- [ ] Add egui/eframe optional dependencies.
- [ ] Add `egui-ui` feature.
- [ ] Gate GTK-only modules.
- [ ] Verify egui-only compile does not pull GTK.

## PR 2: repo-tool screenshot diff infrastructure

- [ ] Add repo-tool command for frontend screenshot diffing.
- [ ] Add named screenshot scenarios.
- [ ] Add shared screenshot scenario builders outside GTK-only modules.
- [ ] Add image comparison and diff image generation.
- [ ] Add synthetic tests for diff helper.

## PR 3: frontend CLI, egui skeleton, and binary

- [ ] Add `--frontend gtk|egui` parsing.
- [ ] Default unspecified frontend to GTK.
- [ ] Add egui module skeleton.
- [ ] Add `xmms-egui` binary if still useful.
- [ ] Launch a blank `eframe` window.
- [ ] Store `AppController` in `EguiApp`.

## PR 4: egui texture rendering and screenshot baseline

- [ ] Convert Cairo-rendered surfaces to egui textures.
- [ ] Display main player skin.
- [ ] Extract shared screenshot helpers if GTK-only logic is needed.
- [ ] Add first GTK/reference vs egui screenshot comparison.
- [ ] Keep render tests green.

## PR 5: main player interactions

- [ ] Add hit testing/controls.
- [ ] Dispatch player/audio/panel commands.
- [ ] Interpret basic effects.

## PR 6: playlist and equalizer

- [ ] Add playlist UI.
- [ ] Add equalizer UI.
- [ ] Dispatch playlist/equalizer commands.

## PR 7: preferences dialog

- [ ] Add egui preferences UI.
- [ ] Wire config mutation through app commands/effects.
- [ ] Leave skin editor GTK-only.

## PR 8: playback, dialogs, screenshots, and CI

- [ ] Wire playback backend effects if desired for first milestone.
- [ ] Add non-GTK file dialogs.
- [ ] Add screenshot comparisons for all first-milestone UI states.
- [ ] Add egui-only CI/smoke checks.
