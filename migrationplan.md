# Rust Migration Plan

Goal: port every active XMMS Resuscitated feature to Rust while keeping the app runnable after each phase. Use the C implementation as behavioral reference and `reference-screenshots/` as human visual ground truth.

## Parity roadmap

- [x] Capture C ground-truth screenshots in `reference-screenshots/`.
- [x] Create isolated Rust crate under `rust/`.
- [x] Port initial core models: config defaults, player state, playlist/M3U handling.
- [x] Port manual XPM parser for the first working skin path.
- [x] Render parsed XPM pixels through Cairo.
- [x] Add Rust GTK preview and smoke mode.
- [x] Capture initial Rust preview screenshot in `rust-preview-screenshots/`.

- [x] Finalize build strategy.
  - [x] Decide whether Meson drives Cargo or Cargo becomes primary.
  - [x] Keep C and Rust binary names non-conflicting until replacement.
  - [x] Add CI coverage for Rust formatting, tests, and GTK smoke.
  - [x] Update Flatpak/RPM packaging strategy.

- [x] Complete core application state.
  - [x] Replace global-style state with explicit `AppState`.
  - [x] Preserve C config defaults and `~/.config/xmms/config` keys.
  - [x] Add config load/save compatibility.
  - [x] Track window visibility, docking, shading, scale, sticky, easy-move, playlist position, equalizer state, visualization state, and podcast preferences.

- [x] Complete skin system.
  - [x] Load default skin assets from bundled resources.
  - [x] Load external BMP, PNG, and XPM skin files.
  - [x] Support `.wsz`, `.zip`, `.tar`, `.tar.gz`, and `.tar.bz2` skin archives.
  - [x] Port `viscolor.txt` parsing.
  - [x] Port `pledit.txt` playlist color parsing.
  - [x] Preserve `numbers` fallback for `nums_ex`.
  - [x] Preserve balance-from-volume fallback.
  - [x] Replace manual XPM parser with a library in a later cleanup phase.

- [x] Complete Cairo renderer.
  - [x] Implement skin pixmap blitting with source/destination rectangles.
  - [x] Match transparency and scaling behavior.
  - [x] Draw focused/unfocused titlebars.
  - [x] Draw normal and WindowShade main-player states.
  - [x] Draw docked panel composition for main, equalizer, and playlist windows.

- [x] Port widget framework.
  - [x] Port `Widget` list management and hit testing.
  - [x] Port push buttons.
  - [x] Port toggle buttons.
  - [x] Port scrolling text boxes.
  - [x] Port horizontal sliders.
  - [x] Port numeric time display.
  - [x] Port visualization widget state.
  - [x] Port mono/stereo indicator.
  - [x] Port play-status indicator.
  - [x] Port simple invisible hit-area buttons.

- [ ] Port main window.
  - [x] Recreate exact main reset-state layout and skin coordinates for visual parity.
  - [x] Port transport controls to Rust UI hit testing and runtime player state.
  - [x] Port volume, balance, and position sliders to Rust UI hit testing and runtime state.
  - [x] Port shaded mode controls for main-window shade/unshade.
  - [x] Port menu actions.
  - [x] Port prompts: play location and jump to time.
  - [x] Port keyboard shortcuts for currently ported actions.
  - [ ] Complete feature-dependent keyboard shortcuts as their target features land.
  - [x] Port drag-and-drop file handling.
  - [x] Port file and directory open dialogs.
  - [x] Port update timer behavior.

- [ ] Complete playlist model.
  - [x] Add files, directories, URLs, Spotify tracks, and podcast entries.
  - [x] Preserve recursive directory import and media-extension filtering.
  - [x] Preserve M3U load/save including `#EXTINF` and `#XMMSPODCAST` markers.
  - [x] Port shuffle, repeat, no-advance, next, previous, EOF, and failed-item skip behavior.
  - [x] Port sorting by title, filename, path, and date.
  - [x] Port selected-entry sorting.
  - [x] Port reverse and randomize.
  - [x] Port duration indexing with GStreamer discoverer.

- [ ] Port playlist window.
  - [x] Render playlist background and rows.
  - [x] Port scrolling and scrollbar dragging.
  - [ ] Port resizing.
  - [ ] Port shaded mode.
  - [ ] Port docked/detached mode.
  - [x] Port add/remove/select/misc/list menus.
  - [x] Port context menu.
  - [x] Port selection, crop, remove dead, and physical delete actions.
  - [x] Port search behavior.
  - [x] Port playlist load/save dialogs.

- [x] Port GStreamer player.
  - [x] Create `playbin` pipeline.
  - [x] Build audio sink chain: `audioconvert`, `audiopanorama`, `equalizer-10bands`, `spectrum`, sink.
  - [x] Disable video with fake sink.
  - [x] Port bus handling for EOS, errors, duration changes, tags, and spectrum messages.
  - [x] Port play, stop, pause, unpause, toggle pause, seek, position, and duration.
  - [x] Port bitrate, frequency, and channel reporting.
  - [x] Port volume and balance.
  - [x] Port output-device rebuild behavior.

- [x] Port equalizer window.
  - [x] Render equalizer skin and controls.
  - [x] Port preamp slider.
  - [x] Port ten band sliders.
  - [x] Port active and auto toggles.
  - [x] Sync equalizer state to GStreamer band properties.
  - [x] Sync volume and balance with the main window.
  - [x] Preserve shaded and detached modes.

- [x] Port visualization behavior.
  - [x] Port analyzer mode.
  - [x] Port scope mode.
  - [x] Port off mode.
  - [x] Port current milkdrop placeholder behavior.
  - [x] Port analyzer styles: bars and lines.
  - [x] Port analyzer modes: normal, fire, vertical lines.
  - [x] Port peaks and falloff speeds.
  - [x] Port VU mode and refresh divisor.

- [ ] Port preferences window.
  - [ ] Port playback/output preferences.
  - [ ] Port playlist options.
  - [ ] Port font controls.
  - [ ] Port title format control.
  - [ ] Port visualization controls.
  - [ ] Port podcast cache TTL and refresh interval controls.
  - [ ] Preserve immediate apply/save behavior.

- [x] Port skin browser.
  - [x] Discover user skins in `~/.config/xmms/Skins/`.
  - [x] Discover system skins in installed data directory.
  - [x] Preview/select installed skin directories and archives.
  - [x] Port reload skin behavior.
  - [x] Port Alt+S shortcut integration.

- [x] Port output device picker.
  - [x] Enumerate GStreamer audio sink devices.
  - [x] Deduplicate PipeWire/Pulse devices like C implementation.
  - [x] Group local and network devices.
  - [x] Preserve automatic system-default output.
  - [x] Switch output device while preserving playback state.
  - [x] Include Spotify devices when authenticated.

- [ ] Port MPRIS D-Bus interface.
  - [x] Own `org.mpris.MediaPlayer2.xmms_resuscitated`.
  - [x] Add deterministic Rust MPRIS model and e2e surface.
  - [x] Implement `org.mpris.MediaPlayer2` root property semantics.
  - [x] Implement `org.mpris.MediaPlayer2.Player` property semantics.
  - [x] Port metadata, playback status, volume, and position properties.
  - [x] Port Next, Previous, Pause, PlayPause, Stop, Play, Seek, SetPosition, and OpenUri.
  - [x] Emit metadata and playback status changes in Rust state.

- [ ] Port Spotify integration.
  - [ ] Preserve PKCE auth flow.
  - [x] Preserve built-in client ID behavior.
  - [x] Preserve refresh token storage in `spotify.conf`.
  - [ ] Port token refresh.
  - [x] Add deterministic playlist fetch response parsing and endpoint construction.
  - [x] Add deterministic playlist track response parsing and endpoint construction.
  - [x] Add deterministic Web API playback-control request construction.
  - [x] Add deterministic device listing, preferred-device, and transfer request handling.
  - [x] Add deterministic Spotify playback-state response parsing.
  - [x] Integrate Spotify URI playback into player state.

- [x] Port Spotify UI.
  - [x] Port Spotify playlist chooser.
  - [x] Port track import into playlist.
  - [x] Port authentication prompts.
  - [x] Port error handling and empty states.

- [x] Port podcast integration.
  - [x] Detect feed URLs vs direct audio streams.
  - [x] Fetch RSS and Atom feeds.
  - [x] Parse RSS/Atom enclosures, media content URLs, and enclosure links.
  - [x] Preserve feed/guid metadata and title fallbacks.
  - [x] Import parsed feed episodes into the playlist model with existing GUID/URL de-duplication.
  - [x] Cache podcast downloads under config dir.
  - [x] Add deterministic SHA-256 cache path, freshness, TTL cleanup, playback URI, retry, and refresh-interval helpers.
  - [x] Discover cached duration with GStreamer.
  - [x] Retry 429 and 503 downloads with backoff.
  - [x] Cleanup cache by TTL.
  - [x] Refresh feeds on timer.
  - [x] Skip failed current podcast item when needed.
  - [x] Wire live HTTP fetch/download execution.

- [ ] Port session and command-line behavior.
  - [ ] Preserve `G_APPLICATION_HANDLES_COMMAND_LINE`.
  - [ ] Preserve `XMMS_NON_UNIQUE`.
  - [ ] Port `--playlist`, `--equalizer`, docking, undocking, shading, `--reset`, `--skin`, and playlist menu options.
  - [ ] Port secondary activation behavior.
  - [ ] Port GTK session save/restore where available.
  - [ ] Preserve fallback save on shutdown/query-end.

- [ ] Port installation and packaging assets.
  - [ ] Update README build/run instructions.
  - [ ] Update Flatpak manifest.
  - [ ] Update RPM spec.
  - [ ] Update man page if CLI behavior changes.
  - [ ] Install desktop file, appstream metadata, icon, skins, and binary.

- [ ] Add parity validation.
  - [ ] Keep Rust unit tests for pure logic.
  - [ ] Add Rust integration tests for playlist/config/skin fixtures.
  - [x] Add Rust UI e2e harness for scripted player settings, clicks, and window visibility assertions.
  - [x] Add GTK smoke tests under Xvfb.
  - [ ] Capture Rust screenshots for main, playlist, and equalizer windows.
    - [x] Capture Rust playlist screenshots at multiple startup sizes.
  - [ ] Compare visually against `reference-screenshots/`.
  - [ ] Validate local playback with representative audio formats.
  - [ ] Validate MPRIS with desktop media controls.
  - [ ] Validate Spotify and podcast flows manually or with mocked services.

- [ ] Cleanup after parity.
  - [ ] Remove temporary working-first unsafe code where practical.
  - [ ] Replace manual XPM parser with a library if still desired.
  - [ ] Remove duplicated C compatibility scaffolding.
  - [ ] Make Rust implementation the primary installed app.
  - [ ] Retire or archive the C implementation once parity is proven.
