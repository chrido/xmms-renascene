# Rust refactoring backlog

This document captures the Rust reuse/refactoring opportunities identified during the review of `src/` on 2026-06-25. It is intentionally written as an actionable TODO list with enough context to resume work without re-discovering the same issues.

## Review context

- Project: `xmms-renascene-rs`
- Rust code reviewed under: `src/` and selected tests under `tests/`
- Largest file by far: `src/ui.rs` (~12k lines)
- Helpful baseline command used during review:

```bash
cargo clippy --all-targets -- \
  -W clippy::too_many_lines \
  -W clippy::cognitive_complexity \
  -W clippy::type_complexity \
  -W clippy::large_enum_variant \
  -W clippy::large_stack_arrays \
  -W clippy::too_many_arguments
```

The command surfaced many refactoring-oriented warnings and one currently blocking Clippy error in tests:

- `src/render.rs:242` has `titlebar_stride * 0`, triggering `clippy::erasing_op`.
- Several large functions in `src/ui.rs`, `src/config.rs`, `src/render/main.rs`, `src/render/playlist.rs`, and `src/ui/file_info.rs` were flagged.
- `PlaybackEvent::Spectrum([f32; SPECTRUM_BANDS])` in `src/player.rs:70` was flagged as a large enum variant.

Recommended validation after each refactor:

```bash
cargo test
cargo clippy --all-targets
```

For refactoring-specific progress, rerun the stricter Clippy command above.

---

## Suggested order of attack

- [x] Fix low-risk quick wins first so diagnostics are cleaner.
- [x] Centralize shared constants/conversions and small duplicated helpers.
- [x] Refactor narrow modules (`playlist`, `skin`, `render`) before major UI splitting.
- [x] Split `src/ui.rs` in vertical slices only after smaller shared abstractions exist.
- [x] Keep behavior-preserving commits small; this project has many E2E tests that should remain green.

---

## TODO 1: Fix quick wins and diagnostic noise

### Context

These changes are low risk and make later checks easier.

Important locations:

- `src/render.rs:242` — `titlebar_stride * 0` triggers `clippy::erasing_op`.
- `src/main.rs:31` — `parse_preview_options` has duplicated flag branches.
- `src/player.rs:759` — test helper takes `&PathBuf` instead of `&Path`.
- `tests/e2e.rs:1276` — `&[skins.clone()]` can use `std::slice::from_ref(&skins)`.
- `src/playlist.rs:359` — `Playlist::next` can be confused with `Iterator::next`.

### How to do it

- [x] Replace `titlebar_stride * 0 + 27 * 4` with `27 * 4`.
- [x] Collapse duplicate CLI aliases in `parse_preview_options`:
  - `--playlist-shaded` and `--shade-playlist`
  - `--equalizer-shaded` and `--shade-equalizer`
- [x] Change the test helper signature in `src/player.rs` from `fn path_to_uri(path: &PathBuf)` to `fn path_to_uri(path: &Path)` and import/use `Path` as needed.
- [x] Replace the cloned slice in `tests/e2e.rs` with `std::slice::from_ref(&skins)`.
- [x] Consider renaming `Playlist::next` to `advance` or `next_track` in a focused follow-up; use LSP rename if available.

### Validation

- [x] `cargo test`
- [x] `cargo clippy --all-targets`
- [x] Rerun the stricter refactoring Clippy command and confirm these findings are gone.

---

## TODO 2: Centralize equalizer constants and position/dB conversion

### Context

Equalizer position-to-dB logic is duplicated in multiple files:

- `src/player.rs:628` — public `equalizer_position_to_db`
- `src/config.rs:502` — private `equalizer_position_to_db`
- `src/equalizer.rs:306` — private `position_to_db`
- `src/render/equalizer.rs:326` — private `position_to_db`

The project also repeats numeric array sizes:

- `SPECTRUM_BANDS = 75` in `src/player.rs:5`, but `[f32; 75]` appears in `src/render/main.rs` and `src/skin/widget.rs`.
- Equalizer band count `10` appears in several modules.

### How to do it

- [x] Create a small shared module, e.g. `src/audio_model.rs` or extend `src/equalizer.rs`, with:

```rust
pub const SPECTRUM_BANDS: usize = 75;
pub const EQUALIZER_BANDS: usize = 10;
pub type SpectrumData = [f32; SPECTRUM_BANDS];
pub type EqualizerBandPositions = [i32; EQUALIZER_BANDS];
pub type EqualizerBandDb = [f64; EQUALIZER_BANDS];

pub fn equalizer_position_to_db(position: i32) -> f64 { ... }
pub fn db_to_equalizer_position(db: f64) -> i32 { ... }
```

- [x] Replace local conversions in `config`, `equalizer`, and `render/equalizer` with the shared helpers.
- [x] Replace literal `[f32; 75]` with `SpectrumData` in:
  - `src/player.rs`
  - `src/render/main.rs`
  - `src/skin/widget.rs`
  - relevant tests
- [x] Replace `[f64; 10]` / `[i32; 10]` where appropriate with named aliases.

### Notes / risks

- Keep public API compatibility in mind. `player::equalizer_position_to_db` may be used by other modules; either re-export it or update imports carefully.
- Do not change rounding behavior. Current math intentionally matches XMMS/Winamp-like behavior.

### Validation

- [x] Existing equalizer tests in `src/equalizer.rs`
- [x] Existing visualization tests in `src/skin/widget.rs`
- [x] `cargo test`

---

## TODO 3: Unify rectangle/geometry types and reduce coordinate-heavy render APIs

### Context

There are several rectangle-like types with overlapping methods:

- `src/skin/layout.rs:16` — `SkinRect`
- `src/skin/widget.rs:7` — `WidgetRect`
- `src/skineditor.rs:119` — `ElementSlot`
- `src/skineditor.rs:193` — `PackRect`

There are also APIs with many raw coordinate arguments:

- `src/render/core.rs:77` — `blit_surface_rect` has 8 arguments.
- `src/render/core.rs:369` — `blit_skin_rect` has 9 arguments.
- `src/render/main.rs:700` — `draw_vis_pixel` has 8 arguments.

### How to do it

- [x] Extend `SkinRect` with common geometry helpers:
  - `right()`
  - `bottom()`
  - `intersects()`
  - `translate(dx, dy)`
  - `clamp_to(width, height)` or similar
- [x] Decide whether to replace `WidgetRect` with `SkinRect`, or implement `From<SkinRect>` / `From<WidgetRect>` conversions.
- [x] Consider replacing `ElementSlot` and `PackRect` internals with `SkinRect` plus metadata:

```rust
struct ElementSlot {
    kind: SkinPixmapKind,
    rect: SkinRect,
}
```

- [x] Refactor `blit_surface_rect` toward:

```rust
fn blit_surface_rect(
    cr: &Context,
    source: &ImageSurface,
    src: SkinRect,
    dest: (i32, i32),
) -> Result<bool, RenderError>
```

- [x] Refactor `blit_skin_rect` to accept a `SpriteSpec` or `SkinRect` source/destination where possible.
- [x] Update call sites gradually. Start with internal render modules before changing public exports.

### Notes / risks

- Be careful with clipping behavior in `blit_surface_rect`; tests cover negative source coordinates in `src/render.rs`.
- Keep XMMS-style exclusive-edge hit testing unchanged.

### Validation

- [x] `cargo test --test render`
- [x] `cargo test`
- [x] Pay close attention to render pixel tests in `src/render.rs` and `tests/render.rs`.

---

## TODO 4: Collapse duplicated playlist menu representations

### Context

Playlist menu concepts are represented three ways:

- `src/ui.rs:5384` — `PlaylistMenuKind`
- `src/skin/layout.rs:134` — `PlaylistMenuButton`
- `src/render/playlist.rs:39` — `PlaylistMenuRenderKind`

Mappings and metadata are spread across:

- `src/ui.rs:5544` — `PlaylistMenuKind::render_kind`
- `src/ui.rs:10947` — `playlist_menu_from_button`
- `src/ui.rs:10957` — `playlist_menu_button_from_kind`
- `src/skin/layout.rs:142` — `PlaylistMenuButton::item_count`
- `src/render/playlist.rs:47` — `PlaylistMenuRenderKind::item_count`
- `src/render/playlist.rs:471` — `playlist_menu_items`
- `src/render/playlist.rs:582` — `playlist_menu_border_source`

### How to do it

- [x] Choose one canonical enum, preferably a public `PlaylistMenuKind` in a lower-level module that does not force `session` to depend on `ui`.
  - Candidate home: `src/playlist.rs` or a new `src/playlist/menu.rs`.
- [x] Move all static metadata to one table:

```rust
struct PlaylistMenuDef {
    kind: PlaylistMenuKind,
    button_x: fn(width: i32) -> i32,
    button_width: i32,
    item_count: usize,
    border_source: (i32, i32),
    item_sources: &'static [PlaylistMenuItemSource],
}
```

- [x] Make layout hit-testing, popup rect calculation, render item lookup, hover default, and command mapping read from this table.
- [x] Remove conversion helpers once all callers use the canonical enum.
- [x] Update `src/session.rs` so it imports `PlaylistMenuKind` from the new canonical location, not `crate::ui`.

### Notes / risks

- E2E tests use `PlaylistMenuKind` directly through `crate::ui`; re-export from `ui` temporarily if needed to avoid a large test update.
- Preserve item order exactly. Command mapping depends on item indexes.

### Validation

- [x] Existing playlist menu tests in `src/ui.rs` near the bottom.
- [x] E2E tests involving playlist menus.
- [x] `cargo test`

---

## TODO 5: Share CLI/session option parsing and option application

### Context

Startup/session flags are parsed and applied in multiple places:

- `src/main.rs:31` — `parse_preview_options`
- `src/session.rs:45` — `parse_session_command`
- `src/ui.rs:691` — `preview_state_from_app_state`

Overlapping options include:

- show/hide playlist/equalizer
- dock/undock playlist/equalizer
- shade/unshade main/playlist/equalizer
- skin path
- reset
- playlist menu flags

### How to do it

- [x] Create `src/cli.rs` or `src/session/args.rs` with table-driven parsing helpers.
- [x] Represent aliases in one place, e.g.:

```rust
enum ParsedArgAction {
    ShowPlaylist,
    ShowEqualizer,
    SetMainShaded(bool),
    SetPlaylistShaded(bool),
    SetEqualizerShaded(bool),
    SetPlaylistDetached(bool),
    SetEqualizerDetached(bool),
    SetSkinPath,
    SetScreenshotPath,
    SetScaleFactor,
    SetPlaylistSize,
    OpenPreferences,
    OpenSkinEditor,
    Reset,
    OpenPlaylistMenu(PlaylistMenuKind),
}
```

- [x] Share a single `apply_preview_options(config: &mut Config, options: &PreviewOptions)` function.
- [x] Use that shared function in:
  - preview startup
  - session command application
  - tests that construct states from options
- [x] Keep `PreviewOptions` as the common data structure unless a more domain-specific name becomes useful.

### Notes / risks

- Preserve ignored unknown `--flags` behavior in session parsing if existing behavior relies on it.
- `parse_preview_options` currently supports screenshot/scale/playlist-size flags that session parsing does not; keep feature differences explicit.

### Validation

- [x] `src/main.rs` parser tests
- [x] `src/session.rs` tests
- [x] E2E startup/session tests
- [x] `cargo test`

---

## TODO 6: Turn `Config` load/save into grouped helpers or a schema

### Context

`Config` serialization/deserialization is long and manually mirrored:

- `src/config.rs:155` — `Config::from_key_file_str`
- `src/config.rs:306` — `Config::to_key_file_string`

Patterns repeat many times:

- parse with default fallback
- clamp values
- support legacy aliases
- write current key and legacy key
- write grouped visualization/equalizer/podcast settings

### How to do it

Option A: grouped helper functions while keeping `Config` flat.

- [x] Split loading into private helpers:
  - `read_window_config(&keys, &mut cfg)`
  - `read_playback_config(&keys, &mut cfg)`
  - `read_playlist_config(&keys, &mut cfg)`
  - `read_equalizer_config(&keys, &mut cfg)`
  - `read_visualization_config(&keys, &mut cfg)`
  - `read_podcast_config(&keys, &mut cfg)`
- [x] Split writing into matching helpers:
  - `write_window_config(&mut out, self)`
  - etc.

Option B: actual nested config structs.

- [x] Introduce `PlaybackConfig`, `PlaylistConfig`, `EqualizerConfig`, `VisualizationConfig`, `PodcastConfig`.
- [x] Migrate `Config` fields gradually, re-exporting or adapter methods if needed.

Recommended first step: Option A. It is lower risk and keeps external callers stable.

### Helper ideas

```rust
fn read_i32_clamped(keys: &BTreeMap<String, String>, key: &str, current: i32, min: i32, max: i32) -> i32;
fn read_bool_or(keys: &BTreeMap<String, String>, key: &str, current: bool) -> bool;
fn read_i32_alias(keys: &BTreeMap<String, String>, keys: &[&str], fallback: i32) -> i32;
```

### Notes / risks

- Preserve legacy key support:
  - `doublesize`
  - visualization aliases like `vis_type`, `analyzer_mode`, `analyzer_type`, `scope_mode`, `analyzer_peaks`, `vis_refresh`
  - legacy equalizer band dB keys
- Preserve output ordering if tests assert serialized contents.

### Validation

- [x] `src/config.rs` tests, especially config round-trip and legacy-key tests.
- [x] `cargo test`

---

## TODO 7: Extract playlist mutation helpers that preserve current position

### Context

Many playlist methods repeat this pattern:

1. Capture current position/current entry.
2. Mutate `entries`.
3. Refresh current position.
4. Fall back to old position when needed.

Examples:

- `src/playlist.rs:247` — `remove_selected_or_current`
- `src/playlist.rs:271` — `remove_selected`
- `src/playlist.rs:283` — `crop_to_selected_or_current`
- `src/playlist.rs:306` — `remove_dead_files`
- `src/playlist.rs:323` — `physically_delete_selected`
- `src/playlist.rs:402` — `sort_by`
- `src/playlist.rs:411` — `sort_selected_by`
- `src/playlist.rs:433` — `reverse`
- `src/playlist.rs:441` — `randomize`
- `src/playlist.rs:449` — `move_entry`

### How to do it

- [x] Add helpers for current entry capture:

```rust
fn current_entry_snapshot(&self) -> Option<PlaylistEntry>;
```

- [x] Add a mutation wrapper:

```rust
fn mutate_entries_preserving_current<F>(&mut self, f: F) -> bool
where
    F: FnOnce(&mut Vec<PlaylistEntry>) -> bool;
```

- [x] Add a variant for removals that need old-position fallback:

```rust
fn mutate_entries_after_remove<F>(&mut self, f: F) -> bool
where
    F: FnOnce(&mut Vec<PlaylistEntry>, Option<usize>) -> bool;
```

- [x] Extract selection predicates:
  - `has_selected_entries()`
  - `selected_or_current_predicate(old_position)`
- [x] Refactor one method at a time and keep tests green.

### Notes / risks

- `PlaylistEntry` equality is used to relocate the current item. Preserve that behavior.
- Removing duplicate entries may have subtle current-position behavior; add tests before changing logic if unsure.

### Validation

- [x] Existing playlist tests.
- [x] Add focused tests around current position after sort/remove/move with duplicate entries.
- [x] `cargo test`

---

## TODO 8: Add a render skin surface cache

### Context

Rendering repeatedly converts `XpmImage` to Cairo `ImageSurface`:

- `src/render/core.rs:48` — `surface_from_xpm`
- `src/render/core.rs:369` — `blit_skin_rect` converts each time.
- Additional repeated conversions in:
  - `src/render/main.rs`
  - `src/render/playlist.rs`
  - `src/render/equalizer.rs`

Some functions cache the surface locally for a single render call, but there is no shared cache across a full render pass/window.

### How to do it

- [x] Introduce a render-facing wrapper, e.g.:

```rust
struct SkinSurfaceCache {
    surfaces: BTreeMap<SkinPixmapKind, ImageSurface>,
}
```

or:

```rust
struct RenderSkin<'a> {
    skin: &'a DefaultSkin,
    surfaces: RefCell<BTreeMap<SkinPixmapKind, ImageSurface>>,
}
```

- [x] Add a method:

```rust
fn surface(&self, kind: SkinPixmapKind) -> Result<Option<ImageSurface>, RenderError>;
```

- [x] Update render functions to use cached surfaces instead of calling `surface_from_xpm` repeatedly.
- [x] Decide invalidation strategy:
  - Rebuild the cache when active skin changes.
  - Rebuild affected pixmap surfaces when the skin editor mutates a pixmap.
- [x] Keep the existing `surface_from_xpm` as the low-level conversion helper.

### Notes / risks

- Cairo `ImageSurface` ownership/clone semantics should be checked before storing/reusing.
- Skin editor mutation makes stale cache risk real; add explicit invalidation hooks.
- This is a performance refactor; keep rendering behavior byte-identical.

### Validation

- [x] Existing render pixel tests.
- [x] Screenshot generation through `write_player_screenshot`.
- [x] `cargo test --test render`
- [x] `cargo test`

---

## TODO 9: Refactor `render_playlist_frame` and main render functions into smaller helpers

### Context

Clippy flagged large render functions:

- `src/render/main.rs:157` — `render_main_player_state`
- `src/render/playlist.rs:56` — `render_playlist_frame`
- `src/render/playlist.rs:471` — `playlist_menu_items`

`render_playlist_frame` mixes shaded frame, normal titlebar, side tiling, footer tiling, and footer text. `render_main_player_state` mixes titlebar buttons, shaded state, normal playback controls, sliders, text, digits, visualization, and indicators.

### How to do it

- [x] Split `render_playlist_frame` into:
  - `render_playlist_shaded_frame`
  - `render_playlist_titlebar`
  - `render_playlist_side_borders`
  - `render_playlist_footer`
  - `render_playlist_footer_text`
- [x] Split `render_main_player_state` into:
  - `render_main_titlebar_buttons`
  - `render_main_transport_buttons`
  - `render_main_toggle_buttons`
  - `render_main_sliders`
  - `render_main_time_digits`
  - `render_main_text_fields`
  - `render_main_indicators`
- [x] For `playlist_menu_items`, replace the long match with static arrays/table constants per TODO 4.

### Notes / risks

- Keep draw order identical. Pixel tests may fail if layering changes.
- Favor private helper functions first; avoid changing public API while splitting internals.

### Validation

- [x] `cargo test --test render`
- [x] `cargo test`

---

## TODO 10: Abstract skin loading over directory/archive sources

### Context

Directory and archive loading are logically parallel:

- `src/skin/mod.rs:239` — `DefaultSkin::load_from_dir`
- `src/skin/mod.rs:279` — `DefaultSkin::load_from_archive`

Duplicated metadata loaders:

- `src/skin/mod.rs:641` — `load_vis_colors_from_dir`
- `src/skin/mod.rs:649` — `load_vis_colors_from_archive`
- `src/skin/mod.rs:664` — `load_playlist_colors_from_dir`
- `src/skin/mod.rs:672` — `load_playlist_colors_from_archive`
- `src/skin/mod.rs:687` — `load_region_masks_from_dir`
- `src/skin/mod.rs:695` — `load_region_masks_from_archive`

### How to do it

- [x] Define a small source abstraction:

```rust
trait SkinAssetSource {
    fn find_pixmap(&self, stem: &str) -> io::Result<Option<SkinAsset>>;
    fn read_text_file(&self, name: &str) -> io::Result<Option<String>>;
}
```

- [x] Implement for:
  - directory source
  - archive-entry source
- [x] Write one shared loader:

```rust
fn load_from_source(source: &impl SkinAssetSource) -> io::Result<DefaultSkin>;
```

- [x] Move fallback logic into the shared path:
  - bundled skin default
  - `numbers` fallback for `nums_ex`
  - balance fallback
  - vis colors
  - playlist colors
  - region masks
  - text colors

### Notes / risks

- Keep archive format support unchanged: `.zip`, `.wsz`, `.tar`, `.tar.gz`, `.tgz`, `.tar.bz2`, `.tbz2`.
- Preserve case-insensitive lookup behavior.
- Preserve recursive directory search behavior.

### Validation

- [x] Existing default skin tests.
- [x] Skin archive/directory tests if present.
- [x] `cargo test`

---

## TODO 11: Decide whether to adopt or remove generic widget abstractions

### Context

`src/skin/widget.rs` defines reusable UI-state abstractions:

- `WidgetList`
- `PushButton`
- `ToggleButton`
- `HorizontalSlider`
- `SimpleButton`
- indicators/displays

However, production interaction logic in `src/ui.rs` mostly uses custom pointer enums and hit-testing instead:

- `MainPointer`
- `EqualizerPointer`
- `PlaylistPointer`
- `MainControl`
- `equalizer_press` / `equalizer_release`
- `press` / `motion` / `release`

The generic widgets are heavily unit-tested but appear underused in production.

### How to do it

Option A: adopt the generic widgets.

- [x] Build main controls from `PushButton`, `ToggleButton`, and `HorizontalSlider`.
- [x] Build equalizer controls from the same primitives or a vertical slider variant.
- [x] Use widget state as the source of pressed/inside/render information.
- [x] Gradually replace `MainPointer`/`EqualizerPointer` where possible.

Option B: remove unused abstractions.

- [x] Confirm with `grep`/LSP references which types are production-used vs test-only.
- [x] Delete or narrow unused types.
- [x] Keep genuinely useful types like `Visualization`, `NumberDisplay`, and enums used by config/rendering.

Recommended approach: first decide based on roadmap. If the UI is expected to grow, adoption may pay off. If not, deletion reduces maintenance.

### Notes / risks

- Adoption is a larger behavior-preserving refactor and should be done after geometry cleanup.
- Deletion may require test cleanup but is simpler if these abstractions are not part of public API.

### Validation

- [x] `cargo test`
- [x] E2E click/drag/keyboard tests.

---

## TODO 12: Split `src/ui.rs` into focused modules

### Context

`src/ui.rs` is the main maintenance bottleneck. It contains:

- GTK application startup and window construction
- panel windows and sync logic
- preferences UI
- skin browser UI
- skin editor UI
- input controllers
- keyboard shortcut dispatch
- playback state/transitions
- main UI state and interaction methods
- playlist menu logic
- file/open/save dialogs
- MPRIS integration hooks

Clippy flagged many `too_many_lines` functions in this file. Splitting should be behavior-preserving and done after extracting shared helpers.

### Proposed module layout

- [x] `src/ui/mod.rs`
  - public exports and top-level `run_default_skin_preview`, screenshot entry points
- [x] `src/ui/app.rs`
  - GTK application startup and `build_preview_window`
- [x] `src/ui/state.rs`
  - `MainWindowUiState`, `DialogVisibility`, `SkinBrowserState`
- [x] `src/ui/panels.rs`
  - `PanelWindows`, `PanelKind`, `PanelPlacement`, panel sync/build functions
- [x] `src/ui/input.rs`
  - click/motion/scroll/key controllers and coordinate scaling helpers
- [x] `src/ui/actions.rs`
  - `UiAction`, `PanelAction`, action dispatch helpers
- [x] `src/ui/preferences.rs`
  - preferences window and pages
- [x] `src/ui/playlist_menu.rs`
  - playlist menu state and commands, if not moved lower-level by TODO 4
- [x] `src/ui/skin_browser.rs`
  - skin browser window and scanning UI
- [x] `src/ui/skin_editor.rs`
  - GTK skin editor window/tool controls
- [x] Keep existing:
  - `src/ui/file_info.rs`
  - `src/ui/style.rs`

### How to do it safely

- [x] First convert `src/ui.rs` into `src/ui/mod.rs` only if necessary, or add sibling modules under existing `src/ui/` and declare them from `src/ui.rs`.
- [x] Move one cohesive chunk at a time.
- [x] Prefer `pub(super)` within `ui` modules instead of broad `pub(crate)`.
- [x] After each move, run:

```bash
cargo test
cargo clippy --all-targets
```

### Notes / risks

- `src/ui.rs` currently has nested dependencies between state, rendering, GTK windows, and tests. Expect to adjust visibility carefully.
- Start with low-dependency helpers such as preferences or panel window sync before moving `MainWindowUiState`.
- Keep E2E-facing public API stable: `MainWindowUiState`, `PanelAction`, `PanelKind`, `PlaylistMenuKind`, `PlaylistSortAction`, etc., are used by `src/e2e.rs` and tests.

---

## TODO 13: Refactor panel action handling and panel window sync

### Context

Panel action dispatch and panel sync logic are duplicated between docked main window and detached panel windows.

Relevant locations:

- `src/ui.rs:6287` — `handle_panel_action_for_main_window`
- `src/ui.rs:6343` — `sync_single_panel_window_from_state`
- `src/ui.rs:6383` — `sync_single_panel_window_values`
- `src/ui.rs:6428` — `sync_panel_windows`
- `src/ui.rs:5575` — `add_panel_click_controller`

### How to do it

- [x] Introduce a shared action environment/context:

```rust
struct UiActionContext<'a> {
    app/window references,
    drawing areas,
    popovers,
    main_state,
}
```

- [x] Use one function for handling `PanelAction` regardless of docked/detached source.
- [x] Use `panel_window_values` + `sync_single_panel_window_values` for both equalizer and playlist in `sync_panel_windows`.
- [x] Extract common “queue redraw/sync main/detached panels” helpers.
- [x] Consider a `PanelWindowSpec` struct with width, height, shaded, visible, resizable.

### Validation

- [x] E2E tests for docking/undocking panels.
- [x] E2E tests for playlist menu actions and equalizer preset popover.
- [x] `cargo test`

---

## TODO 14: Split `show_file_info_dialog_inner` into data, UI, and save logic

### Context

`src/ui/file_info.rs:67` `show_file_info_dialog_inner` is large and mixes:

- selecting current/selected playlist entry
- building GTK widgets
- wiring apply/save callbacks
- ID3 tag editing
- dialog visibility bookkeeping

### How to do it

- [x] Extract data preparation:

```rust
fn prepare_file_info_dialog_data(...) -> Option<(FileInfoDetails, PlaylistColors)>;
```

- [x] Extract widget construction:

```rust
struct FileInfoWidgets { ... }
fn build_file_info_widgets(details: &FileInfoDetails) -> FileInfoWidgets;
```

- [x] Extract callback wiring:

```rust
fn connect_file_info_actions(...);
```

- [x] Keep ID3 read/write helpers separate from GTK code.

### Validation

- [x] Existing file-info tests in `src/ui/file_info.rs`.
- [x] E2E tests that open file info.
- [x] `cargo test`

---

## TODO 15: Consider boxing or aliasing large playback event payloads

### Context

Clippy flagged:

- `src/player.rs:70` — `PlaybackEvent::Spectrum([f32; SPECTRUM_BANDS])`

The enum size is dominated by the `Spectrum` variant. This can make event vectors/copies more expensive.

### How to do it

Option A: Box only the large variant.

```rust
Spectrum(Box<SpectrumData>)
```

- [x] Update event creation in `spectrum_from_structure` / `spectrum_from_values` callers.
- [x] Update `Player::apply_playback_event` to deref/copy as needed.
- [x] Update tests.

Option B: Keep as-is but add a comment if performance impact is negligible.

### Notes / risks

- Boxing introduces heap allocation per spectrum event; this may or may not be better depending on event frequency and ownership flow.
- If spectrum events are high-frequency, a reusable buffer or separate channel might be better later.

### Validation

- [x] Player tests in `src/player.rs`.
- [x] Visualization E2E tests.
- [x] `cargo test`

---

## TODO 16: Reduce CSS builder duplication in `src/ui/style.rs`

### Context

`src/ui/style.rs` already uses helper functions like `append_css_rule`, but Clippy still flags large CSS-building functions:

- `src/ui/style.rs:296` — `xmms_window_css`
- `src/ui/style.rs:472` — `file_info_css`

### How to do it

- [x] Split into rule group helpers:
  - `append_skinned_window_surface_css`
  - `append_skinned_window_frame_css`
  - `append_form_control_css`
  - `append_button_css`
  - `append_file_info_entry_css`
- [x] Keep selectors and properties table-driven where possible.
- [x] Add a small unit test for generated CSS containing important selectors if helpful.

### Validation

- [x] `cargo test`
- [x] Manual/GTK smoke run if available.

---

## Useful commands for starting work

List Rust files by size:

```bash
wc -l src/**/*.rs src/*.rs tests/*.rs 2>/dev/null | sort -n
```

Find current references before moving/renaming symbols:

```bash
rg "PlaylistMenuKind|PlaylistMenuButton|PlaylistMenuRenderKind" src tests
rg "equalizer_position_to_db|position_to_db|db_to_equalizer_position|db_to_position" src tests
rg "WidgetRect|SkinRect|PackRect|ElementSlot" src tests
```

Run all tests:

```bash
cargo test
```

Run standard Clippy:

```bash
cargo clippy --all-targets
```

Run refactoring-focused Clippy:

```bash
cargo clippy --all-targets -- \
  -W clippy::too_many_lines \
  -W clippy::cognitive_complexity \
  -W clippy::type_complexity \
  -W clippy::large_enum_variant \
  -W clippy::large_stack_arrays \
  -W clippy::too_many_arguments
```

---

## Notes for future implementation agents

- Preserve XMMS/Winamp compatibility behavior. Many constants and odd-looking geometry values are likely skin-format parity requirements.
- Prefer small, behavior-preserving commits. Pixel rendering tests are sensitive to draw order and rounding.
- Before editing a symbol, read the exact function body and use LSP references where available.
- For large `src/ui.rs` moves, keep public E2E-facing types re-exported until tests are updated.
- If refactoring rendering, run both unit tests and screenshot/render tests because the UI is heavily pixel-oriented.
