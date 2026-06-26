# UI Separation Plan

## Goal

Separate XMMS Renascene's application logic from its GTK-specific user interface so the project can support multiple frontends in the future, including desktop GTK, iOS, Android, and potentially other UI toolkits.

The end state should make GTK one frontend implementation instead of the place where most application behavior lives. Core behavior such as playlist handling, playback state transitions, configuration, equalizer logic, session commands, skin/layout state, and user-command handling should be usable without importing GTK, GIO, GLib, or any concrete desktop widget type.

A future mobile frontend should be able to reuse the same core state, commands, effects, view models, playlist behavior, equalizer behavior, config behavior, and playback coordination rules while replacing only the platform-specific presentation and integration layers.

## Motivation

The current codebase has grown from a working GTK application. This is good for parity and fast iteration, but it also means a lot of application decisions are coupled to GTK event handlers, drawing areas, dialogs, popovers, window synchronization, and GTK-specific state wiring.

This coupling makes several things harder:

- Adding a second UI toolkit requires rediscovering and duplicating behavior currently embedded in `src/ui.rs`.
- Mobile ports would need to depend on desktop GTK concepts or rewrite large parts of the application.
- Unit testing core behavior is harder when behavior is tied to widgets and event controllers.
- Refactoring UI code is risky because behavior and presentation are interleaved.
- `src/ui.rs` is very large, making ownership boundaries hard to see.
- Platform-specific concerns such as dialogs, timers, clipboard, windows, CSS, gestures, and file pickers are mixed with application rules.

The goal is not to rewrite the application. The goal is to gradually create a clean architecture around the behavior that already exists.

## High-Level Approach

Use an incremental, behavior-preserving refactor.

The first priority is to extract frontend-neutral logic into new modules while leaving the GTK UI working exactly as it does today. GTK should initially call the new pure logic modules, but no visible behavior should change.

Over time, the application should move toward this shape:

```text
src/
  app/                    # frontend-neutral application orchestration
    mod.rs
    command.rs            # user/application commands
    effect.rs             # effects requested by core logic
    controller.rs         # state transitions and command handling
    preview.rs            # startup/preview option handling
    view_model.rs         # data models consumed by UIs
    panel.rs              # panel placement/visibility/shading logic
    playlist_actions.rs   # playlist menu/action mapping

  domain/ or existing core modules
    app_state.rs
    audio_model.rs
    config.rs
    equalizer.rs
    playlist.rs
    podcast.rs
    skin/
    session.rs

  playback/               # eventual backend abstraction
    mod.rs
    model.rs
    backend.rs
    gstreamer.rs

  ui/
    mod.rs                # frontend selection / shared UI boundary
    gtk/                  # GTK-specific implementation
      mod.rs
      app.rs
      main_window.rs
      playlist_window.rs
      equalizer_window.rs
      preferences.rs
      file_info_dialog.rs
      skin_browser.rs
      style.rs
      gestures.rs
      dialogs.rs

  render/                 # current Cairo skin renderer; later abstractable
```

The core rule:

> Frontend-neutral modules must not import `gtk`, `gio`, `glib`, `gdk`, or concrete widget types.

GTK-specific modules may import and adapt core/application types.

## Architectural Principles

### 1. Core logic owns behavior

The following behavior should live outside GTK code:

- playlist mutations
- playlist current-position preservation rules
- shuffle/repeat/no-advance behavior
- playback state transitions
- pause-between-songs behavior
- EOF handling
- failed-track skipping
- equalizer state and conversions
- config load/save and config updates
- session command parsing and application
- preview/startup option application
- playlist menu command mapping
- panel visibility/docking/shading decisions
- stream info formatting
- title/time formatting
- user preference state changes
- skin/layout hit-test calculations where they are toolkit-independent

### 2. UI code owns presentation and platform integration

GTK code should own:

- GTK application/window lifecycle
- widgets and drawing areas
- event controllers and gestures
- popovers and menus
- CSS/style providers
- dialogs and file choosers
- clipboard integration
- timers/main-loop scheduling
- platform window movement/resizing
- rendering surfaces into GTK drawing contexts

### 3. User actions become commands

UI event handlers should translate frontend events into command values, for example:

```rust
AppCommand::Play
AppCommand::Pause
AppCommand::SetVolume(80)
AppCommand::TogglePlaylistVisibility
AppCommand::ExecutePlaylistMenu { kind, index }
```

The command handler should update application state and return effects.

### 4. Side effects become explicit effects

Core/application code should not directly open dialogs, queue GTK redraws, or call player backend APIs. It should request effects such as:

```rust
AppEffect::StartPlayback
AppEffect::StopPlayback
AppEffect::QueueRender(RenderTarget::Main)
AppEffect::SaveConfig
AppEffect::OpenFileDialog(FileDialogRequest::AddFiles)
AppEffect::ShowError(String)
```

The active frontend decides how to perform those effects.

### 5. UIs consume view models

Frontend code should render from data-only view models rather than reaching into every part of `AppState`.

Examples:

```rust
MainPlayerViewModel
PlaylistViewModel
EqualizerViewModel
PanelViewModel
PreferencesViewModel
```

A GTK frontend can render these using Cairo/GTK widgets. A mobile frontend can render the same models using native UI components.

### 6. Keep commits small and verifiable

Each step should be small enough to review independently. Most steps should be mechanical extraction or logic relocation with tests moved or added alongside the change.

Recommended validation after each step:

```bash
cargo test --lib --bins
cargo test --test render
cargo clippy --all-targets
```

For larger UI movement steps, also run any available E2E tests.

---

# Progress Tracking Checklist

Use this checklist to track implementation. Keep the boxes unchecked until the code has been implemented, committed, and validated with the listed checks for that step.

## Phase Checklist

- [x] **Step 0: Audit current UI coupling**
  - [x] Search and classify all `gtk`, `gio`, `glib`, `gdk`, and `cairo` usages.
  - [x] Classify `src/ui.rs` functions into pure logic, view-model construction, widget construction, event handling, dialogs, rendering glue, and platform/window integration.
  - [x] Record high-value extraction candidates.
  - [x] Confirm no behavior changes were made, or run validation if notes/code changed.

- [x] **Step 1: Introduce the `app` module skeleton**
  - [x] Add `src/app/mod.rs`.
  - [x] Add `src/app/preview.rs`.
  - [x] Add `src/app/command.rs`.
  - [x] Add `src/app/effect.rs`.
  - [x] Add `src/app/controller.rs`.
  - [x] Add `src/app/view_model.rs`.
  - [x] Register `pub mod app;` in `src/lib.rs`.
  - [x] Keep placeholders/documentation only where behavior is not yet migrated.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the module skeleton.

- [x] **Step 2: Move preview/startup option logic out of `ui.rs`**
  - [x] Move `PreviewOptions` to `src/app/preview.rs`.
  - [x] Move `apply_preview_options_to_config` to `src/app/preview.rs`.
  - [x] Update `src/main.rs` imports.
  - [x] Update `src/session.rs` imports.
  - [x] Update `src/ui.rs` imports/usages.
  - [x] Update tests using `PreviewOptions`.
  - [x] Keep GTK-specific preview state construction in GTK/UI code for now if needed.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the preview extraction.

- [x] **Step 3: Define frontend-neutral commands and effects**
  - [x] Add initial `AppCommand` enum in `src/app/command.rs`.
  - [x] Include playback commands: play, pause, stop, toggle pause, previous, next, seek.
  - [x] Include audio commands: set volume, set balance, equalizer-related commands as needed.
  - [x] Include playlist commands: execute playlist menu, playlist selection/action commands as needed.
  - [x] Include panel commands: toggle visibility, shade, detach/dock, set playlist size.
  - [x] Add initial `AppEffect` enum in `src/app/effect.rs`.
  - [x] Add `RenderTarget` or equivalent render invalidation target type.
  - [x] Include playback effects: start, pause, stop, seek, update backend volume/balance/equalizer.
  - [x] Include UI/platform effects: queue render, save config, open dialogs, show errors.
  - [x] Do not migrate all handlers yet; only introduce the vocabulary.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the command/effect API.

- [x] **Step 4: Extract simple pure helpers from `ui.rs`**
  - [x] Move time formatting helpers.
  - [x] Move prompt time parsing helpers.
  - [x] Move stream info text formatting.
  - [x] Move main title text formatting.
  - [x] Move playlist footer info formatting.
  - [x] Move panel state conversion helpers.
  - [x] Move menu-index to playlist-action mapping helpers.
  - [x] Move mouse wheel delta-to-volume/seek calculations.
  - [x] Move toolkit-independent titlebar/button hit-test rules.
  - [x] Move or add unit tests in the new app modules.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo test --test render`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit each helper group or the completed helper extraction.

- [x] **Step 5: Introduce `AppController`**
  - [x] Add `AppController` holding `AppState`.
  - [x] Add `new`, `state`, `state_mut`, and `into_state` accessors.
  - [x] Add `handle_command(&mut self, AppCommand) -> Vec<AppEffect>`.
  - [x] Add `handle_playback_event(&mut self, PlaybackEvent) -> Vec<AppEffect>`.
  - [x] Keep `AppController` free of GTK/widget/backend object dependencies.
  - [x] Add focused controller tests for any migrated behavior.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the controller shell.

- [x] **Step 6: Move playback-control state logic into `AppController`**
  - [x] Migrate Play handling.
  - [x] Migrate Pause handling.
  - [x] Migrate Stop handling.
  - [x] Migrate Toggle Pause handling.
  - [x] Migrate Previous Track handling.
  - [x] Migrate Next Track handling.
  - [x] Migrate seek behavior.
  - [x] Migrate changing-to-next-track-starts-from-beginning behavior.
  - [x] Migrate play-from-stopped-preserves-selected-position behavior.
  - [x] Migrate pause-between-songs behavior.
  - [x] Migrate EOF behavior.
  - [x] Migrate stale backend position sync blocking.
  - [x] Convert GTK playback handlers to emit `AppCommand` values.
  - [x] Interpret returned playback/render/save effects in GTK.
  - [x] Move existing playback behavior tests from `ui::tests` to controller tests where possible.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the playback controller migration.

- [x] **Step 7: Move playlist action logic into `AppController`**
  - [x] Move add/remove/select/misc/list menu index mapping.
  - [x] Move sort action mapping.
  - [x] Move playlist selection changes that are currently in GTK handlers.
  - [x] Move current-position preservation rules after playlist mutations where currently UI-owned.
  - [x] Move reverse/randomize/sort command handling.
  - [x] Move toolkit-independent playlist scroll row calculations.
  - [x] Convert GTK playlist menu handlers to `AppCommand` values.
  - [x] Add/move tests for playlist menu command mapping.
  - [x] Add/move tests for sort/current-entry preservation behavior.
  - [x] Add/move tests for remove/select menu behaviors.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the playlist action migration.

- [x] **Step 8: Move panel/window state logic into app modules**
  - [x] Move main player shaded state toggling logic.
  - [x] Move playlist visible/shaded/detached state logic.
  - [x] Move equalizer visible/shaded/detached state logic.
  - [x] Move panel state mapping.
  - [x] Move docking relationship logic.
  - [x] Move toolkit-independent desired panel size calculations.
  - [x] Keep actual GTK window show/hide/resize/move/present code in GTK.
  - [x] Add/move tests for panel state mapping.
  - [x] Add/move tests for shade/close titlebar action mapping.
  - [x] Add/move tests for docked panel size behavior.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo test --test render`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the panel state migration.

- [x] **Step 9: Extract view-model builders**
  - [x] Define `MainPlayerViewModel`.
  - [x] Define `PlaylistViewModel` and row model types.
  - [x] Define `EqualizerViewModel`.
  - [x] Define panel/preferences view model types as needed.
  - [x] Add `main_player_view_model(&AppState)`.
  - [x] Add `playlist_view_model(&AppState)`.
  - [x] Add `equalizer_view_model(&AppState)`.
  - [x] Update GTK render/update code to consume view models where practical.
  - [x] Move/add tests for main render state formatting.
  - [x] Move/add tests for playlist row selected/current states.
  - [x] Move/add tests for equalizer view state.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo test --test render`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the view-model extraction.

- [x] **Step 10: Introduce a GTK runtime/effect interpreter**
  - [x] Add a GTK-side effect interpreter, initially in `src/ui.rs` or `src/ui/gtk/runtime.rs`.
  - [x] Implement playback effects by calling the current backend integration.
  - [x] Implement render invalidation effects by queueing appropriate redraws.
  - [x] Implement config-save effects.
  - [x] Implement dialog-opening effects.
  - [x] Implement error/message effects.
  - [x] Route migrated GTK handlers through `apply_effects`.
  - [x] Keep `AppController` pure and frontend-neutral.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Manually smoke test key UI actions.
  - [x] Commit the GTK effect interpreter.

- [x] **Step 11: Physically split `src/ui.rs` into GTK modules**
  - [x] Convert `src/ui.rs` into `src/ui/mod.rs` or a small compatibility wrapper.
  - [x] Create `src/ui/gtk/mod.rs`.
  - [x] Move GTK application startup/lifecycle into `src/ui/gtk/app.rs`.
  - [x] Move main window construction into `src/ui/gtk/main_window.rs`.
  - [x] Move main menu/popover code into `src/ui/gtk/main_menu.rs`.
  - [x] Move playlist window/menu code into `src/ui/gtk/playlist_window.rs` and/or `playlist_menu.rs`.
  - [x] Move equalizer window code into `src/ui/gtk/equalizer_window.rs`.
  - [x] Move preferences window code into `src/ui/gtk/preferences.rs`.
  - [x] Move file-info dialog code into `src/ui/gtk/file_info_dialog.rs` if not already separate.
  - [x] Move skin browser code into `src/ui/gtk/skin_browser.rs`.
  - [x] Move gesture/event-controller glue into `src/ui/gtk/gestures.rs`.
  - [x] Move generic dialog helpers into `src/ui/gtk/dialogs.rs`.
  - [x] Keep or move style code under `src/ui/gtk/style.rs` depending on existing layout.
  - [x] Run `cargo test --lib --bins` after each significant move.
  - [x] Run `cargo test --test render` after larger moves.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit the GTK module split.

- [x] **Step 12: Define frontend service boundaries**
  - [x] Identify file dialog service needs.
  - [x] Identify directory dialog service needs.
  - [x] Identify message/error dialog service needs.
  - [x] Identify clipboard service needs.
  - [x] Identify timer/main-loop scheduling needs.
  - [x] Identify config/storage path service needs.
  - [x] Identify skin import/export path service needs.
  - [x] Identify external URL opening needs.
  - [x] Identify platform window action needs.
  - [x] Prefer explicit `AppEffect` payloads over injecting many service traits too early.
  - [x] Document the frontend/platform surface area.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit service boundary documentation/types.

- [x] **Step 13: Separate playback backend from UI concerns**
  - [x] Split playback model types from GStreamer implementation where practical.
  - [x] Create `src/playback/model.rs` for frontend/backend-neutral playback data.
  - [x] Create `src/playback/backend.rs` for backend trait/interface.
  - [x] Move current GStreamer implementation toward `src/playback/gstreamer.rs`.
  - [x] Keep existing desktop behavior and tests green.
  - [x] Adapt `AppEffect` playback effects to the backend boundary.
  - [x] Add/update backend integration tests.
  - [x] Run `cargo test --lib --bins`.
  - [x] Run `cargo clippy --all-targets`.
  - [x] Commit playback backend separation.

- [x] **Step 14: Decide rendering abstraction strategy**
  - [x] Decide whether mobile should initially reuse the Cairo renderer as a bitmap renderer.
  - [x] Decide whether to introduce frontend-neutral draw commands later.
  - [x] Document the selected rendering strategy.
  - [x] If choosing Cairo-first, define how non-GTK frontends receive rendered images.
  - [x] If choosing draw-commands, define an initial `DrawCommand` model and migration plan.
  - [x] Preserve existing pixel/render parity tests.
  - [x] Run `cargo test --test render` if code changes are made.
  - [x] Commit rendering strategy documentation or initial abstraction.

- [x] **Step 15: Introduce feature-gated frontends**
  - [x] Confirm app/core modules compile without GTK imports.
  - [x] Confirm GTK code lives under `src/ui/gtk/`.
  - [x] Confirm playback backend effects are isolated.
  - [x] Confirm core tests do not require GTK setup.
  - [x] Add `gtk-ui` feature.
  - [x] Add `gstreamer-backend` feature.
  - [x] Consider a placeholder `mobile-ui` feature only if useful.
  - [x] Keep current defaults preserving desktop behavior.
  - [x] Add/check `cargo check --no-default-features` or a documented equivalent target once feasible.
  - [x] Run full default validation.
  - [x] Commit frontend feature gates.

## Milestone Checklist

- [x] **Milestone 1: Create app boundary**
  - [x] Complete Step 1.
  - [x] Complete Step 2.
  - [x] Validate with `cargo test --lib --bins`.
  - [x] Validate with `cargo clippy --all-targets`.

- [x] **Milestone 2: Extract pure helpers and tests**
  - [x] Complete Step 4.
  - [x] Pure helper tests live outside GTK where practical.
  - [x] Validate with `cargo test --lib --bins`.
  - [x] Validate with `cargo test --test render`.
  - [x] Validate with `cargo clippy --all-targets`.

- [x] **Milestone 3: Add commands/effects and controller shell**
  - [x] Complete Step 3.
  - [x] Complete Step 5.
  - [x] Migrate at least one command family.
  - [x] Validate with `cargo test --lib --bins`.
  - [x] Validate with `cargo clippy --all-targets`.

- [x] **Milestone 4: Migrate playlist and panel state logic**
  - [x] Complete Step 7.
  - [x] Complete Step 8.
  - [x] GTK handlers dispatch commands instead of owning the migrated behavior.
  - [x] Validate with `cargo test --lib --bins`.
  - [x] Validate with `cargo test --test render`.
  - [x] Validate with `cargo clippy --all-targets`.

- [x] **Milestone 5: Split GTK modules**
  - [x] Complete Step 10.
  - [x] Complete Step 11.
  - [x] `src/ui.rs` is removed, renamed to `src/ui/mod.rs`, or reduced to a small wrapper.
  - [x] Validate with `cargo test --lib --bins`.
  - [x] Validate with `cargo test --test render`.
  - [x] Validate with `cargo clippy --all-targets`.

# Detailed Plan

## Step 0: Audit current UI coupling

### Objective

Identify which parts of the current code are GTK-specific and which parts are application logic embedded inside GTK code.

### Work

1. Search for all imports/usages of:
   - `gtk`
   - `gio`
   - `glib`
   - `gdk`
   - `cairo`
2. Classify functions in `src/ui.rs` into categories:
   - pure application logic
   - state/view-model construction
   - GTK widget construction
   - GTK event handling
   - GTK dialog/file chooser code
   - rendering/drawing glue
   - platform/window integration
3. Create a short internal map of high-value extraction candidates.

### Audit notes

Completed audit findings:

- Dependency search found GTK/GIO/GLib/GDK/Cairo references concentrated in `src/ui.rs`, `src/ui/file_info.rs`, `src/ui/style.rs`, and the Cairo renderer modules under `src/render/`. Non-UI platform/backend usages also exist in `src/player.rs`, `src/mpris.rs`, and `src/podcast.rs`.
- `src/ui.rs` contains several distinct groups:
  - preview/startup helpers near the top of the file;
  - keyboard shortcut mapping and event handling;
  - GTK window/menu/dialog builders;
  - preferences, skin browser, and skin editor UI construction;
  - panel/window state synchronization;
  - pointer/control state and `MainWindowUiState`;
  - pure formatting, slider conversion, title formatting, and hit-test helpers near the end;
  - file chooser and GTK action glue.
- Highest-value first extraction candidates are:
  - `PreviewOptions` and preview config application;
  - pure formatting/parsing helpers such as duration/time/title formatting;
  - slider conversion helpers;
  - playlist menu/action mapping;
  - panel state mapping;
  - playback command state transitions currently tested through `ui::tests`.
- Step 0 changed only this planning document and did not change runtime behavior.

### Expected outcome

A clearer list of what should move first. No behavior changes.

### Validation

No code changes required, unless adding comments or notes. If code changes are made, run:

```bash
cargo test --lib --bins
```

---

## Step 1: Introduce the `app` module skeleton

### Objective

Create a home for frontend-neutral application orchestration without changing behavior.

### Work

Add files:

```text
src/app/mod.rs
src/app/preview.rs
src/app/command.rs
src/app/effect.rs
src/app/controller.rs
src/app/view_model.rs
```

Initially, most files can contain only type/module placeholders and documentation comments.

Register the module in `src/lib.rs`:

```rust
pub mod app;
```

### Initial structure

```rust
// src/app/mod.rs
pub mod command;
pub mod controller;
pub mod effect;
pub mod preview;
pub mod view_model;
```

### Expected outcome

The project has an explicit namespace for application logic. No behavior changes.

### Validation

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

---

## Step 2: Move preview/startup option logic out of `ui.rs`

### Objective

Remove frontend-neutral startup/preview option behavior from the GTK UI module.

### Current issue

Types/functions like these are not GTK-specific:

- `PreviewOptions`
- `apply_preview_options_to_config`
- preview/session config application rules

They should not live in `src/ui.rs`.

### Work

Move to:

```text
src/app/preview.rs
```

Likely exports:

```rust
pub struct PreviewOptions { ... }

pub fn apply_preview_options_to_config(
    config: &mut Config,
    options: &PreviewOptions,
) -> Result<(), String>;
```

Update imports in:

- `src/main.rs`
- `src/session.rs`
- `src/ui.rs`
- tests using `PreviewOptions`

### Notes

If `preview_state_from_options` or `preview_state_from_app_state` still returns GTK/UI-specific state, keep those wrappers in GTK UI for now, but make them call the extracted pure config function.

### Expected outcome

Session and CLI preview parsing no longer depend on `crate::ui` for frontend-neutral option types.

### Validation

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

---

## Step 3: Define frontend-neutral commands and effects

### Objective

Introduce a common language between future UIs and application logic.

### Work

Add command types in `src/app/command.rs`.

Initial commands should be conservative and based on existing behavior:

```rust
pub enum AppCommand {
    Play,
    Pause,
    Stop,
    TogglePause,
    PreviousTrack,
    NextTrack,
    SeekToMs(i64),
    SetVolume(i32),
    SetBalance(i32),
    ToggleShuffle,
    ToggleRepeat,
    ToggleNoPlaylistAdvance,
    ToggleMainShade,
    TogglePlaylistVisibility,
    ToggleEqualizerVisibility,
    TogglePlaylistShade,
    ToggleEqualizerShade,
    SetPlaylistSize { width: i32, height: i32 },
    ExecutePlaylistMenu { kind: PlaylistMenuKind, index: usize },
}
```

Add effect types in `src/app/effect.rs`:

```rust
pub enum AppEffect {
    StartPlayback,
    StartPlaybackFromCurrent,
    PausePlayback,
    StopPlayback,
    SeekPlayback(i64),
    SetBackendVolume(i32),
    SetBackendBalance(i32),
    SetBackendEqualizer,
    SaveConfig,
    QueueRender(RenderTarget),
    OpenAddFilesDialog,
    OpenAddDirectoryDialog,
    OpenFileInfoDialog,
    OpenSkinBrowser,
    ShowError(String),
}

pub enum RenderTarget {
    Main,
    Playlist,
    Equalizer,
    All,
}
```

Do not migrate all handlers yet. Introduce the vocabulary first.

### Expected outcome

A reviewed API for UI-independent command/effect exchange.

### Validation

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

---

## Step 4: Extract simple pure helpers from `ui.rs`

### Objective

Move low-risk pure functions first, building confidence and shrinking `ui.rs`.

### Candidate functions/logic

Move to `src/app/view_model.rs`, `src/app/panel.rs`, or small focused modules:

- time formatting helpers
- prompt time parsing helpers
- stream info text formatting
- main title text formatting
- playlist footer info formatting
- panel state conversion helpers
- menu index to playlist action mapping
- mouse wheel delta-to-volume/seek calculations
- titlebar button hit-test rules, if independent from GTK types

### Work pattern

For each helper group:

1. Read the function and related tests.
2. Move it to an `app` module.
3. Make GTK code call the new function.
4. Move or add unit tests in the app module.
5. Commit.

### Expected outcome

`src/ui.rs` starts to depend on pure application helpers instead of containing them.

### Validation

```bash
cargo test --lib --bins
cargo test --test render
cargo clippy --all-targets
```

---

## Step 5: Introduce `AppController`

### Objective

Create a frontend-neutral state-transition owner.

### Work

Add `src/app/controller.rs`:

```rust
pub struct AppController {
    state: AppState,
}

impl AppController {
    pub fn new(state: AppState) -> Self;
    pub fn state(&self) -> &AppState;
    pub fn state_mut(&mut self) -> &mut AppState;
    pub fn into_state(self) -> AppState;

    pub fn handle_command(&mut self, command: AppCommand) -> Vec<AppEffect>;
    pub fn handle_playback_event(&mut self, event: PlaybackEvent) -> Vec<AppEffect>;
}
```

At first, it can delegate to existing functions or only handle a small command subset.

### Important design choice

Avoid passing GTK objects, player backend objects, or drawing areas into `AppController`.

`AppController` should only know about:

- `AppState`
- domain models
- commands
- playback events as data
- effects as return values

### Expected outcome

A stable abstraction exists for future frontend command handling.

### Validation

Add focused tests for basic commands, then run:

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

---

## Step 6: Move playback-control state logic into `AppController`

### Objective

Make playback buttons platform-neutral.

### Candidate behavior to move

- Play
- Pause
- Stop
- Toggle pause
- Previous track
- Next track
- changing to next track starts from beginning
- play from stopped preserves selected position
- pause-between-songs handling
- EOF behavior
- stale backend position sync blocking

### Work

1. Identify GTK handlers/tests for playback controls.
2. Move the state-transition logic into `AppController::handle_command` and/or `handle_playback_event`.
3. Have GTK handlers translate button events into `AppCommand`.
4. Have GTK interpret returned `AppEffect` values by calling the existing backend/UI functions.
5. Move existing behavior tests from `ui::tests` to `app::controller` where possible.

### Example

GTK before:

```rust
// handler directly mutates state, starts backend, queues redraws
```

GTK after:

```rust
let effects = controller.borrow_mut().handle_command(AppCommand::Play);
gtk_runtime.apply_effects(effects);
```

### Expected outcome

Playback-control behavior can be tested without GTK.

### Validation

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

Run any E2E playback-related tests if available.

---

## Step 7: Move playlist action logic into `AppController`

### Objective

Make playlist menu/actions platform-neutral.

### Candidate behavior to move

- Add/remove/select/misc/list menu index mapping
- sort action mapping
- playlist selection changes
- current-position preservation after mutations
- reverse/randomize/sort commands
- scroll row calculations where they are not toolkit-specific

### Work

Create either:

```text
src/app/playlist_actions.rs
```

or keep initially in `src/app/controller.rs` if small.

Convert GTK menu handlers into `AppCommand::ExecutePlaylistMenu { kind, index }` or more specific commands.

### Expected outcome

A future UI can expose playlist actions without duplicating menu index behavior.

### Validation

Move/add tests for:

- playlist menu command maps menu indices
- sort preserves current entry
- selected-only sort behavior
- remove/select menu behaviors

Then run:

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

---

## Step 8: Move panel/window state logic into app modules

### Objective

Separate logical panel state from GTK windows.

### Candidate behavior to move

- main/player shaded state
- playlist visible/shaded/detached state
- equalizer visible/shaded/detached state
- panel state mapping
- docking relationships
- desired panel size calculations where toolkit-independent
- show/hide/toggle behavior

### Keep in GTK

- actual `gtk::ApplicationWindow` creation
- calls to resize/move/present/hide windows
- platform-specific window constraints
- drawing-area invalidation

### Possible module

```text
src/app/panel.rs
```

Possible types:

```rust
pub enum PanelKind {
    Main,
    Playlist,
    Equalizer,
}

pub struct PanelStateChange {
    pub panel: PanelKind,
    pub visible: Option<bool>,
    pub shaded: Option<bool>,
    pub detached: Option<bool>,
}
```

### Expected outcome

Panel behavior can be tested without creating GTK windows.

### Validation

Move/add tests for:

- panel state maps visibility/detach/shade flags
- shade and close titlebar buttons return window actions
- docked panel size ignores detached panels

Then run:

```bash
cargo test --lib --bins
cargo test --test render
cargo clippy --all-targets
```

---

## Step 9: Extract view-model builders

### Objective

Make UI rendering consume data-only models instead of raw state internals.

### Work

Create view models in `src/app/view_model.rs` or split by area:

```rust
pub struct MainPlayerViewModel {
    pub title: String,
    pub elapsed_text: String,
    pub bitrate_text: String,
    pub frequency_text: String,
    pub channels_text: String,
    pub player_state: PlayerState,
    pub volume: i32,
    pub balance: i32,
    pub shuffle: bool,
    pub repeat: bool,
    pub shaded: bool,
}

pub struct PlaylistViewModel {
    pub rows: Vec<PlaylistRowViewModel>,
    pub current_index: Option<usize>,
    pub selected_indices: Vec<usize>,
    pub scroll_offset: usize,
    pub visible: bool,
    pub shaded: bool,
}

pub struct EqualizerViewModel {
    pub active: bool,
    pub auto: bool,
    pub preamp_position: i32,
    pub band_positions: EqualizerBandPositions,
    pub visible: bool,
    pub shaded: bool,
}
```

Add builder functions:

```rust
pub fn main_player_view_model(state: &AppState) -> MainPlayerViewModel;
pub fn playlist_view_model(state: &AppState) -> PlaylistViewModel;
pub fn equalizer_view_model(state: &AppState) -> EqualizerViewModel;
```

### Expected outcome

GTK render/update code depends on stable view models. Future frontends can consume the same view models.

### Validation

Move/add tests for:

- main render state formats stream info like XMMS
- playlist row selected/current states
- equalizer view state follows config/runtime state

Then run:

```bash
cargo test --lib --bins
cargo test --test render
cargo clippy --all-targets
```

---

## Step 10: Introduce a GTK runtime/effect interpreter

### Objective

Keep platform side effects out of `AppController`.

### Work

Create GTK-specific effect handling, likely in:

```text
src/ui/gtk/runtime.rs
```

or initially inside `src/ui.rs`:

```rust
struct GtkAppRuntime { ... }

impl GtkAppRuntime {
    fn apply_effects(&self, effects: Vec<AppEffect>) {
        for effect in effects {
            self.apply_effect(effect);
        }
    }

    fn apply_effect(&self, effect: AppEffect) {
        match effect {
            AppEffect::StartPlayback => { ... }
            AppEffect::QueueRender(target) => { ... }
            AppEffect::SaveConfig => { ... }
            AppEffect::OpenAddFilesDialog => { ... }
            AppEffect::ShowError(message) => { ... }
            // ...
        }
    }
}
```

### Expected outcome

`AppController` remains pure. GTK handles platform work in one place.

### Validation

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

Manual smoke testing is recommended for UI actions.

---

## Step 11: Physically split `src/ui.rs` into GTK modules

### Objective

Reduce the size of `src/ui.rs` after behavior has been extracted.

### Timing

Do this after the controller/view-model extraction has reduced logic coupling. Splitting first would move a large tangled file into many tangled files.

### Proposed target layout

```text
src/ui/
  mod.rs
  gtk/
    mod.rs
    app.rs
    main_window.rs
    main_menu.rs
    playlist_window.rs
    playlist_menu.rs
    equalizer_window.rs
    preferences.rs
    file_info_dialog.rs
    skin_browser.rs
    gestures.rs
    dialogs.rs
    style.rs
```

### Work

1. Move CSS/style-specific code first, if not already separated.
2. Move file-info dialog code.
3. Move skin browser code.
4. Move preferences window code.
5. Move playlist/equalizer window code.
6. Move main window construction last.

### Expected outcome

`src/ui.rs` becomes either `src/ui/mod.rs` or a small compatibility wrapper. GTK-specific code is under `src/ui/gtk/`.

### Validation

After each module move:

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

For larger moves:

```bash
cargo test --test render
```

---

## Step 12: Define frontend service boundaries

### Objective

Prepare for non-GTK frontends by identifying platform services.

### Candidate service boundaries

Eventually define frontend/platform traits or effect payloads for:

- file dialogs
- directory dialogs
- message/error dialogs
- clipboard
- timers
- config file location
- skin import/export location
- external URL opening
- platform window actions
- notification/toast messages

### Preferred approach

Favor effects first. Avoid making `AppController` generic over many traits too early.

For example, prefer:

```rust
AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles)
```

over:

```rust
controller.handle_command(command, &dyn FileDialogService)
```

This keeps app logic deterministic and easier to test.

### Expected outcome

The required platform surface area is explicit and documented.

### Validation

Compile/test only; most changes are structural.

---

## Step 13: Separate playback backend from UI concerns

### Objective

Prepare for alternative playback backends on iOS/Android.

### Current situation

`player.rs` contains GStreamer-specific backend code and playback model/event types. This is not GTK UI, but it is platform/backend-specific.

### Work

Eventually split:

```text
src/playback/
  mod.rs
  model.rs       # PlayerState, PlaybackEvent, tags, stream info
  backend.rs     # trait/interface
  gstreamer.rs   # current implementation
```

Possible trait:

```rust
pub trait PlaybackBackend {
    fn play_uri(&self, uri: &str) -> Result<(), PlaybackError>;
    fn pause(&self) -> Result<(), PlaybackError>;
    fn stop(&self) -> Result<(), PlaybackError>;
    fn seek(&self, position_ms: i64) -> Result<(), PlaybackError>;
    fn set_volume(&self, volume: i32) -> Result<(), PlaybackError>;
    fn set_balance(&self, balance: i32) -> Result<(), PlaybackError>;
    fn set_equalizer(&self, state: EqualizerBackendState) -> Result<(), PlaybackError>;
}
```

### Recommendation

Do this after the first UI/controller split. The UI separation will make it clearer where playback effects are triggered.

### Expected outcome

Future mobile platforms can provide their own playback implementation.

### Validation

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

Backend integration tests should remain green.

---

## Step 14: Decide rendering abstraction strategy

### Objective

Prepare skin rendering for non-GTK platforms.

### Current situation

Rendering is Cairo-based. That may still be useful on mobile if we render into bitmaps, but mobile frontends may prefer native drawing APIs.

### Option A: Keep Cairo as portable skin renderer

The renderer outputs an image/surface. Each frontend displays that image.

Pros:

- preserves current pixel parity
- lowest behavior risk
- reuse render tests

Cons:

- mobile must integrate Cairo or accept bitmap rendering
- less native UI flexibility

### Option B: Produce frontend-neutral draw commands

Core renderer builds commands:

```rust
pub enum DrawCommand {
    Blit {
        pixmap: SkinPixmapKind,
        source: SkinRect,
        dest: SkinRect,
    },
    Text {
        text: String,
        position: (i32, i32),
        color: RgbaColor,
    },
    FillRect {
        rect: SkinRect,
        color: RgbaColor,
    },
}
```

GTK/Cairo consumes commands. Mobile consumes commands with native drawing.

Pros:

- cleaner abstraction
- mobile can draw natively

Cons:

- larger refactor
- higher risk to pixel parity

### Recommendation

Do not start here. First separate application logic and GTK. Preserve the Cairo renderer initially, then revisit rendering once view models and frontend boundaries exist.

---

## Step 15: Introduce feature-gated frontends

### Objective

Allow building the core without GTK and eventually select frontends by feature.

### Possible future `Cargo.toml` shape

```toml
[features]
default = ["gtk-ui", "gstreamer-backend"]
gtk-ui = ["dep:gtk", "dep:gio", "dep:glib"]
gstreamer-backend = ["dep:gstreamer"]
mobile-ui = []
```

### Important warning

Do not feature-gate too early. Feature gates are painful while dependencies are still tangled.

### Suggested timing

Only add frontend/backend feature gates after:

- `app` modules compile without GTK imports
- GTK code is under `src/ui/gtk/`
- playback backend effects are isolated
- core tests do not require GTK setup

### Expected outcome

A future check like this becomes possible:

```bash
cargo check --no-default-features --features core
```

---

# Proposed Initial Milestones

## Milestone 1: Create app boundary

Scope:

- Add `src/app/` module skeleton.
- Move `PreviewOptions` and `apply_preview_options_to_config` out of `ui.rs`.
- Update `main.rs`, `session.rs`, and `ui.rs` imports.

Expected PR/commit title:

```text
Introduce frontend-neutral app preview module
```

Validation:

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

## Milestone 2: Extract pure helpers and tests

Scope:

- Move formatting/parsing helpers out of `ui.rs`.
- Move menu-index/action mapping helpers out of `ui.rs`.
- Move existing behavior tests where possible.

Expected PR/commit title:

```text
Move pure UI helper logic into app modules
```

Validation:

```bash
cargo test --lib --bins
cargo test --test render
cargo clippy --all-targets
```

## Milestone 3: Add commands/effects and controller shell

Scope:

- Add `AppCommand`.
- Add `AppEffect`.
- Add `AppController` shell.
- Migrate one small command family, ideally playback controls.

Expected PR/commit title:

```text
Introduce app controller command/effect boundary
```

Validation:

```bash
cargo test --lib --bins
cargo clippy --all-targets
```

## Milestone 4: Migrate playlist and panel state logic

Scope:

- Move playlist menu/action logic into controller/app modules.
- Move panel state logic into app modules.
- Keep GTK as effect interpreter and renderer.

Expected PR/commit title:

```text
Move playlist and panel behavior out of GTK UI
```

Validation:

```bash
cargo test --lib --bins
cargo test --test render
cargo clippy --all-targets
```

## Milestone 5: Split GTK modules

Scope:

- Move GTK code into `src/ui/gtk/` submodules.
- Keep public behavior unchanged.
- Keep main entrypoint using GTK frontend.

Expected PR/commit title:

```text
Split GTK frontend into focused modules
```

Validation:

```bash
cargo test --lib --bins
cargo test --test render
cargo clippy --all-targets
```

---

# Success Criteria

This effort is successful when:

1. Frontend-neutral modules do not import GTK/GIO/GLib/GDK.
2. GTK code is an adapter around commands, effects, and view models.
3. Application behavior can be unit-tested without GTK widgets.
4. `src/ui.rs` is either gone or reduced to a small module wrapper.
5. Playback, playlist, panel, equalizer, config, and session behavior live outside the GTK frontend.
6. A future mobile frontend can be started by implementing:
   - command dispatch from native events
   - effect handling using mobile APIs
   - rendering from shared view models
   - platform services such as dialogs/timers/storage
7. Existing XMMS/Winamp compatibility behavior and render parity tests remain green.

---

# Review Questions

Before implementation, decide the following:

1. Should the frontend-neutral namespace be called `app`, `core`, or something else?
2. Should `session.rs` stay top-level or move under `app/session.rs` later?
3. Should command/effect types be broad from the start, or introduced only as each handler migrates?
4. Should view models mirror the existing skin-render states, or be more frontend-agnostic?
5. Should mobile frontends aim to reuse the Cairo renderer initially, or should we plan for draw commands?
6. How much GTK behavior should remain in tests versus being migrated into pure controller tests?
7. Should playback backend abstraction happen during UI separation or after the GTK split?
