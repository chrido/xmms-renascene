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
- Ported initial playlist and M3U handling, including podcast metadata markers.
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

The Rust version is not yet feature-complete. It currently renders and handles the default main-window controls, including showing skinned playlist and equalizer preview windows with basic controls, resize, and menu behavior. Playback controls only update Rust runtime state until the GStreamer backend is ported. The Rust port still lacks full playback, complete playlist data operations, audio-connected equalizer behavior, MPRIS, Spotify, podcasts, output device selection UI, playlist-connected Open Location behavior, real Skin Browser implementation, full preferences UI, packaging, and full command-line/session behavior.

The manual XPM parser is intentionally kept for the first working port. A later cleanup phase can replace it with a library after parity is reached.

## Next major milestones

See `migrationplan.md` for the full checkbox roadmap. The next high-value areas are:

- Port GStreamer playback.
- Port playlist and equalizer windows.
