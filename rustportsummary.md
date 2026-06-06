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
- Added a GTK preview window that renders the default main skin.
- Added a GTK smoke mode for non-interactive validation.
- Captured an initial Rust preview screenshot in `rust-preview-screenshots/`.
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
| `rust/src/skin/mod.rs` | Skin pixmap definitions, bundled default skin loading, external BMP/PNG/XPM files, skin archives, visualization colors, and playlist colors |
| `rust/src/skin/xpm.rs` | Manual XPM parser |
| `rust/src/skin/widget.rs` | Initial widget/visualization enums |
| `rust/src/render.rs` | XPM-to-Cairo surface conversion |
| `rust/src/ui.rs` | GTK preview window and smoke mode |
| `rust/tests/default_skin.rs` | Default skin parsing tests |
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

Existing C application build:

```sh
meson compile -C builddir
```

Meson Rust validation targets:

```sh
meson compile -C builddir
meson compile -C builddir rust-fmt
meson compile -C builddir rust-test
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

These screenshots are intended as human visual references, not strict pixel-diff test fixtures.

## Current limitations

The Rust version is not yet feature-complete. It currently renders only the default main skin preview and does not yet implement full playback, controls, playlist UI, equalizer UI, MPRIS, Spotify, podcasts, output device selection UI, preferences UI, packaging, or full command-line/session behavior.

The manual XPM parser is intentionally kept for the first working port. A later cleanup phase can replace it with a library after parity is reached.

## Next major milestones

See `migrationplan.md` for the full checkbox roadmap. The next high-value areas are:

- Finalize build and packaging strategy.
- Complete skin loading beyond bundled XPM defaults.
- Expand the renderer to support pixmap blitting and all main-window widgets.
- Port the widget framework and main player controls.
- Port GStreamer playback.
- Port playlist and equalizer windows.
