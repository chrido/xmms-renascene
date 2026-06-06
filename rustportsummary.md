# Rust Port Summary

This document summarizes the current state of the Rust port of XMMS Resuscitated.

## Current status

The Rust port is an isolated Cargo crate in `rust/`. It does not replace the existing C/Meson application yet. The current goal is to keep the Rust version runnable after each migration slice while using the C application as the behavioral and visual reference.

Completed so far:

- Captured C ground-truth screenshots in `reference-screenshots/`.
- Created a Rust crate named `xmms-resuscitated-rs`.
- Added a Rust binary named `xmms-rs`.
- Ported initial config defaults.
- Ported initial player state behavior.
- Ported initial playlist and M3U handling, including recursive directory import with C-compatible media-extension filtering plus podcast metadata markers.
- Ported a manual XPM parser from the C implementation's behavior.
- Added a default skin loader for `data/defskin/*.xpm`.
- Embedded bundled default skin XPM assets in the Rust crate for filesystem-independent startup.
- Added external skin directory loading for BMP, PNG, and XPM pixmap files, including the Winamp green transparency key.
- Added path-based external skin loading for directories plus `.wsz`, `.zip`, `.tar`, `.tar.gz`, and `.tar.bz2` archives.
- Ported `viscolor.txt` parsing with C-compatible defaults, comma/space-separated rows, integer clamping, and archive lookup.
- Ported `pledit.txt` playlist color parsing with C-compatible defaults, `#RRGGBB` keys, and archive lookup.
- Preserved the legacy `numbers.*` fallback for missing `nums_ex.*` skins, including the C-compatible expansion to 108-pixel-wide number graphics.
- Preserved the C balance-from-volume fallback when a skin omits `balance.*`.
- Switched XPM loading to prefer the `image-extras` XPM decoder, retaining the compatibility parser only as a fallback for XMMS skin quirks.
- Added Cairo conversion for parsed XPM pixels.
- Added a C-compatible Cairo skin blitter with source/destination rectangles, source clipping, nearest filtering, and pad extend.
- Added renderer coverage for transparent pixels plus C-compatible scale-factor clamping, coordinate rounding, dimension minimums, and window-scale transforms.
- Added Rust main-titlebar rendering for focused/unfocused and shaded/unshaded rows using the same source rectangles as the C app.
- Added Rust main-player background rendering for normal and WindowShade states.
- Added docked panel composition for main, equalizer, and playlist backgrounds, including detached-panel exclusion and playlist frame tiling.
- Added Rust widget list management with visible-only reverse-order hit testing and redraw flags.
- Ported push-button widget state, source selection, left-button press/release activation, motion inside/outside tracking, and allow-draw behavior.
- Ported toggle-button widget state, selected/unselected source selection, and C-compatible release-to-toggle behavior.
- Ported scrolling textbox state, C font glyph mapping, scroll separator behavior, rendered-width tracking, and offset ticking.
- Ported horizontal slider state, click-to-jump, drag offsets, clamping, frame source calculation, and release/motion results.
- Ported numeric time display state, default blank value, digit source mapping, dash fallback, and redraw behavior.
- Ported visualization widget state, defaults, data/peak clamping, decay, milkdrop phase/energy updates, and redraw behavior.
- Ported mono/stereo indicator state and C-compatible active/inactive segment source mapping.
- Ported play-status indicator state and stopped/paused/playing source-row mapping.
- Ported simple invisible hit-area button press/release/motion activation behavior.
- Added a GTK preview window that renders the default main reset state, including titlebar buttons, transport buttons, toggle buttons, text boxes, volume/balance/position sliders, blank time numbers, visualization grid, mono/stereo indicator, and stopped play-status indicator.
- Wired the Rust main-window GTK preview to click and motion controllers: titlebar close/minimize/shade, play/pause/stop/previous/next/eject press states, shuffle/repeat/equalizer/playlist toggles, and volume/balance/position slider dragging now update Rust runtime state and redraw.
- Added Rust GTK preview windows for the equalizer and playlist; the main-window EQ and PL toggle buttons now show and hide those skinned windows.
- Added a Rust UI e2e harness for scripted startup settings, main-window clicks, and window visibility assertions. Current scenarios cover titlebar buttons, visible main menu, transport buttons, shuffle/repeat, sliders, playlist/equalizer toggles, and startup visibility settings.
- Moved Rust main-window input handling to capture-phase GTK window controllers so clicks reach the skinned controls reliably, and added a visible popover menu for the main menu button.
- Wired playlist and equalizer top-right titlebar controls in the Rust GTK preview: shade toggles shrink/restore the skinned windows, and close hides the panel and clears the main toggle state.
- Wired floating equalizer and playlist titlebar drags through GTK4 toplevel movement for the undecorated preview windows.
- Wired panel titlebar focus/drag skin state so floating playlist/equalizer windows redraw active/inactive titlebar skins while moving or when GTK window activation changes.
- Wired the playlist bottom Add, Remove, Select, Misc, and List buttons to open skinned in-window submenus using the C playlist button and menu source coordinates, including selected-row skin state on press/hover.
- Added playlist resizing for the Rust preview, including startup sizing, current-size hit testing, and frame tiling at resized dimensions.
- Added the shaded playlist frame and compact title/time overlay.
- Wired the main player eject/open button to a native file chooser dialog in the GTK preview.
- Wired the main menu actions in the GTK preview: Open Files opens the native multi-select file chooser, Open Location and Skin Browser show Rust placeholder windows, Preferences shows the preferences placeholder, and Quit exits the app.
- Ported the main prompt mechanics for Play Location and Jump to Time: prompt windows use entry plus Cancel/OK controls, `Ctrl+L` and `Ctrl+J` open the prompts, Play Location records the submitted URI for later playlist integration, and Jump to Time parses seconds or `mm:ss` and updates preview seek state.
- Added Rust preview keyboard shortcuts for currently ported main-window behavior: transport keys, open-files, shuffle/repeat/no-advance toggles, preferences, prompts, skin-browser placeholder, main shade, playlist/equalizer show-hide, and playlist/equalizer shade shortcuts.
- Wired GTK file-list drag-and-drop for the Rust main and playlist preview windows. Drops on the main window replace the playlist and start preview playback, while drops on the playlist window append to existing entries.
- Wired accepted Rust file and directory open-dialog selections into playlist state. File selections and directory selections replace the playlist and start preview playback, matching the C main-window open behavior at the current playlist-model level.
- Wired playlist location import through the Rust model for file drops, file/directory dialogs, and Open Location submissions, preserving URL/Spotify/podcast entry support.
- Ported Rust playlist navigation state for next, previous, EOF/no-advance, repeat wraparound, shuffle ordering, and failed-current skip handling; main prev/next controls now update playlist position and preview playback state.
- Ported Rust playlist sorting by title, filename, path, and file date, preserving the current playlist entry across sort operations.
- Ported Rust selected-entry playlist sorting so only selected rows are sorted and reinserted at their original selected indices.
- Expanded deterministic Rust e2e coverage for playlist Spotify/podcast entries, all playlist sort keys, selected-entry sorting, and close-button behavior.
- Ported Rust playlist reverse and randomize operations, preserving the current entry after reordering and covering both in e2e tests.
- Ported Rust playlist duration indexing with a GStreamer `Discoverer` path plus deterministic e2e coverage; missing non-podcast/non-Spotify entries can now receive duration and tag-title updates while stale URI results are ignored.
- Ported Rust playlist row rendering over the skinned playlist background, including selected row background, current-row color, playlist numbering, and duration text.
- Ported Rust playlist row scrolling and scrollbar dragging, including resized-playlist hit testing and deterministic e2e coverage for visible-row updates.
- Wired Rust playlist bottom-menu actions for Select All/None/Invert, Remove Selected/Crop/All, and List New, with e2e coverage for row selection and entry mutation.
- Added a Rust playlist right-click context popover with Remove Selected, Remove Dead Files, Select All, Select None, and Invert Selection actions, including e2e coverage for local dead-file pruning.
- Added confirmed Rust physical-delete handling for selected local playlist files, removing entries only after successful filesystem deletion and covering the behavior in e2e tests.
- Ported Rust playlist incremental search, including `/` startup, printable query input, Backspace editing, Escape/Enter close behavior, case-insensitive row matching, scroll-to-match, skinned search overlay rendering, and e2e coverage.
- Wired Rust playlist List menu load/save actions to native file dialogs and the Rust M3U model, with e2e coverage for opening each dialog, writing M3U output, and replacing entries from loaded playlists.
- Wired Rust playlist Add URL/File/Directory submenu actions to the existing location prompt and append-mode file/directory dialogs, with e2e coverage for each action.
- Completed Rust playlist Misc submenu wiring: Sort opens a GTK sort popover covering every list/selection sort plus randomize/reverse, File Info records the selected/current entry title, and Options records activation; all actions have e2e coverage.
- Added the first Rust GStreamer playback backend construction slice: it initializes GStreamer, creates a `playbin` pipeline, disables video with `fakesink`, and wires an audio sink bin starting at `audioconvert`, optionally passing through `audiopanorama`, `equalizer-10bands`, and `spectrum`, then ending at `autoaudiosink`.
- Added typed Rust GStreamer bus-event polling for end-of-stream, errors, duration changes, tag metadata, and spectrum magnitudes, with unit coverage using real GStreamer message objects.
- Added Rust GStreamer playback-control methods for URI playback, stop, pause, unpause, pause toggling, seeking, and position/duration queries, with deterministic tests using generated silent WAV input and `fakesink`.
- Added Rust GStreamer volume and balance property controls plus player-side stream-info, duration, and spectrum update hooks; bitrate updates now flow from tag bus events into the Rust player state.
- Added Rust GStreamer equalizer band syncing helpers for all ten `equalizer-10bands` properties, including dB clamping and coverage for every band.
- Added Rust GStreamer stream-info reporting from audio caps so frequency and channel counts can be applied to player state alongside tag-derived bitrate.
- Added Rust GStreamer audio output rebuild support by sink factory, preserving the full audio processing chain and explicitly surfacing unsupported device selection requests.
- Added C-compatible Rust equalizer slider-to-dB mapping, exposed equalizer GStreamer band values from the UI state, and expanded e2e coverage across all ten bands plus inactive-EQ zeroing.
- Wired Rust shaded-equalizer volume and balance slider hit-testing to the shared player state using the C equalizer formulas, with e2e coverage for volume and balance endpoints/center.
- Added e2e-visible Rust docked panel state for equalizer/playlist detach and reattach flows, covering docked composition size changes when panels are detached.
- Wired the Rust visualization widget into main-window rendering and timer updates, covering analyzer/scope/off/milkdrop modes, bars/lines, normal/fire/vertical-line analyzer colors, peaks/falloff, shaded VU mode, and refresh divisors with e2e tests.
- Added a Rust preferences state/e2e surface plus a tabbed GTK preferences shell, covering immediate application of audio, playlist, docking, font, title, visualization, podcast, and default-reset settings.
- Added Rust skin browser discovery/search-path behavior for user, legacy, system, and `SKINSDIR` skin directories plus deterministic e2e coverage for sorted discovery, archive display names, default/custom selection, and reload requests.
- Added Rust output-device picker state with GStreamer audio sink enumeration, C-compatible local/network grouping and deduplication, automatic default selection, playback-preserving system selection, and Spotify device selection e2e coverage.
- Fixed the Rust playlist close path to avoid GTK hide/resize callbacks re-entering `MainWindowUiState` while a `RefCell` borrow is still active.
- Added a Rust GTK preview update timer that ticks every 100 ms, advances preview seek position while playing, and queues main/playlist/equalizer redraws.
- Added interactive Rust equalizer state for ON/AUTO/PRESETS, preamp and ten band sliders, EQ graph rendering, and preset application.
- Added a Rust preferences placeholder window and connected the main menu Preferences item to show it.
- Added a GTK smoke mode for non-interactive validation.
- Captured an initial Rust preview screenshot in `rust-preview-screenshots/`.
- Re-captured `rust-preview-screenshots/main-preview.png` and compared it to `reference-screenshots/main-reset.png`; the latest ImageMagick AE and RMSE metrics are both `0`.
- Added Meson run targets for Rust formatting, tests, CLI smoke, and GTK smoke.
- Added a GitLab CI Rust job for formatting, tests, GTK smoke, and CLI smoke.
- Added explicit `AppState` to tie together config, player, and playlist runtime state.
- Expanded config coverage to include equalizer, visualization, podcast, window, docking, and output-device state.
- Added C-compatible `~/.config/xmms/config` keyfile load/save helpers.

## Rust crate layout

| Path | Purpose |
|---|---|
| `rust/Cargo.toml` | Cargo package definition and dependencies |
| `rust/src/lib.rs` | Library module exports |
| `rust/src/main.rs` | `xmms-rs` CLI and preview entry point |
| `rust/src/app_state.rs` | Explicit Rust application state and runtime snapshots |
| `rust/src/config.rs` | Initial Rust config model and defaults |
| `rust/src/player.rs` | Initial player state model |
| `rust/src/playlist.rs` | Playlist entries, M3U load/save, podcast metadata handling |
| `rust/src/e2e.rs` | Rust UI e2e harness for scripted settings, clicks, and assertions |
| `rust/src/skin/mod.rs` | Skin pixmap definitions, bundled default skin loading, external BMP/PNG/XPM files, skin archives, visualization colors, and playlist colors |
| `rust/src/skin/xpm.rs` | Manual XPM parser |
| `rust/src/skin/widget.rs` | Widget list/hit-testing model, all initial widget state machines, and visualization enums |
| `rust/src/render.rs` | XPM-to-Cairo conversion, skin blitting, docked panel rendering, and main reset-state composition |
| `rust/src/ui.rs` | GTK preview windows, smoke mode, interactive main-window control state, and EQ/playlist preview window visibility |
| `rust/tests/default_skin.rs` | Default skin parsing tests |
| `rust/tests/e2e.rs` | Rust UI e2e scenarios |
| `rust/tests/render.rs` | Cairo render tests |

## Dependencies

Current Rust dependencies:

- `gtk4` via crate rename `gtk`, with GTK 4.6 feature enabled.
- `gstreamer` and `gstreamer-pbutils` for playback backend construction and playlist duration discovery.
- `cairo-rs` via crate rename `cairo`.
- `image` with PNG and BMP support for external skin pixmap files.
- `image-extras` with XPM support for primary XPM decoding.
- `zip`, `tar`, `flate2`, and `bzip2` for external skin archives.

The existing C application remains built by Meson and still depends on GTK4, GStreamer, libsoup, json-glib, libxml2, and optional libarchive.

## How to run

From the repository root:

```sh
cd rust
cargo run
```

This loads all bundled default skin pixmaps and prints a CLI smoke message.

To open the Rust GTK preview:

```sh
cd rust
cargo run -- --gtk
```

To open the Rust GTK preview with the playlist visible at a specific size:

```sh
cd rust
cargo run -- --gtk --playlist-size=325x280
```

To run the self-closing GTK smoke path:

```sh
cd rust
cargo run -- --gtk-smoke
```

## Validation commands

Rust tests:

```sh
cd rust
cargo test
```

Rust GTK smoke under Xvfb:

```sh
cd rust
xvfb-run -a -s '-screen 0 1024x768x24' \
  env GDK_BACKEND=x11 GSK_RENDERER=cairo GDK_DISABLE=gl NO_AT_BRIDGE=1 \
  cargo run -- --gtk-smoke
```

Rust UI e2e scenarios:

```sh
cd rust
cargo test --test e2e
```

Existing C application build:

```sh
meson compile -C builddir
```

Meson Rust validation targets:

```sh
meson compile -C builddir
meson compile -C builddir rust-fmt
meson compile -C builddir rust-test
meson compile -C builddir rust-e2e
meson compile -C builddir rust-smoke
meson compile -C builddir rust-gtk-smoke
```

## Build and packaging strategy

During migration, Meson remains the primary build and packaging entry point for the production C application. Cargo owns the Rust preview crate under `rust/`, and Meson exposes Rust validation through run targets so contributors can exercise both stacks from the existing build workflow.

The Rust binary remains named `xmms-rs` and does not conflict with the installed C binary `xmms`. Flatpak and RPM packaging continue to ship the C application until the Rust implementation reaches parity; Rust packaging will be enabled when `xmms-rs` is ready to become the primary app.

## Visual reference artifacts

C ground-truth screenshots:

- `reference-screenshots/main-reset.png`
- `reference-screenshots/playlist-reset.png`
- `reference-screenshots/equalizer-reset.png`

Current Rust preview screenshot:

- `rust-preview-screenshots/main-preview.png`
- `rust-preview-screenshots/playlist-sizes/rust-playlist-275x232.png`
- `rust-preview-screenshots/playlist-sizes/rust-playlist-325x232.png`
- `rust-preview-screenshots/playlist-sizes/rust-playlist-325x280.png`
- `rust-preview-screenshots/playlist-sizes/rust-playlist-500x320.png`
- `rust-preview-screenshots/diffs/main-ae-diff.png`
- `rust-preview-screenshots/diffs/main-ae.txt`
- `rust-preview-screenshots/diffs/main-rmse.txt`
- `rust-preview-screenshots/diffs/main-identify.txt`

The current main reset-state Rust preview matches the C reference exactly for the captured root screenshot (`AE 0`, `RMSE 0`). These screenshots are intended as human visual references, not strict pixel-diff test fixtures.

## Current limitations

The Rust version is not yet feature-complete. It currently renders and handles the default main-window controls, including showing skinned playlist and equalizer preview windows with basic controls, resize, menu behavior, file-list drops, file/directory open dialogs, recursive directory import, and Open Location playlist insertion. Playback controls only update Rust runtime state until the GStreamer backend is ported. The Rust port still lacks full playback, complete playlist data operations, audio-connected equalizer behavior, MPRIS, live Spotify/podcast integrations, output device selection UI, real Skin Browser implementation, full preferences UI, packaging, and full command-line/session behavior.

The manual XPM parser is intentionally kept for the first working port. A later cleanup phase can replace it with a library after parity is reached.

## Next major milestones

See `migrationplan.md` for the full checkbox roadmap. The next high-value areas are:

- Port GStreamer playback.
- Port playlist and equalizer windows.
