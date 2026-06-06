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

- [ ] Complete skin system.
  - [x] Load default skin assets from bundled resources.
  - [x] Load external BMP, PNG, and XPM skin files.
  - [x] Support `.wsz`, `.zip`, `.tar`, `.tar.gz`, and `.tar.bz2` skin archives.
  - [x] Port `viscolor.txt` parsing.
  - [x] Port `pledit.txt` playlist color parsing.
  - [x] Preserve `numbers` fallback for `nums_ex`.
  - [x] Preserve balance-from-volume fallback.
  - [ ] Replace manual XPM parser with a library in a later cleanup phase.

- [ ] Complete Cairo renderer.
  - [ ] Implement skin pixmap blitting with source/destination rectangles.
  - [ ] Match transparency and scaling behavior.
  - [ ] Draw focused/unfocused titlebars.
  - [ ] Draw normal and WindowShade main-player states.
  - [ ] Draw docked panel composition for main, equalizer, and playlist windows.

- [ ] Port widget framework.
  - [ ] Port `Widget` list management and hit testing.
  - [ ] Port push buttons.
  - [ ] Port toggle buttons.
  - [ ] Port scrolling text boxes.
  - [ ] Port horizontal sliders.
  - [ ] Port numeric time display.
  - [ ] Port visualization widget state.
  - [ ] Port mono/stereo indicator.
  - [ ] Port play-status indicator.
  - [ ] Port simple invisible hit-area buttons.

- [ ] Port main window.
  - [ ] Recreate exact main player layout and skin coordinates.
  - [ ] Port transport controls.
  - [ ] Port volume, balance, and position sliders.
  - [ ] Port shaded mode controls.
  - [ ] Port menu actions.
  - [ ] Port prompts: play location and jump to time.
  - [ ] Port keyboard shortcuts.
  - [ ] Port drag-and-drop file handling.
  - [ ] Port file and directory open dialogs.
  - [ ] Port update timer behavior.

- [ ] Complete playlist model.
  - [ ] Add files, directories, URLs, Spotify tracks, and podcast entries.
  - [ ] Preserve recursive directory import and media-extension filtering.
  - [ ] Preserve M3U load/save including `#EXTINF` and `#XMMSPODCAST` markers.
  - [ ] Port shuffle, repeat, no-advance, next, previous, EOF, and failed-item skip behavior.
  - [ ] Port sorting by title, filename, path, and date.
  - [ ] Port selected-entry sorting.
  - [ ] Port reverse and randomize.
  - [ ] Port duration indexing with GStreamer discoverer.

- [ ] Port playlist window.
  - [ ] Render playlist background and rows.
  - [ ] Port scrolling and scrollbar dragging.
  - [ ] Port resizing.
  - [ ] Port shaded mode.
  - [ ] Port docked/detached mode.
  - [ ] Port add/remove/select/misc/list menus.
  - [ ] Port context menu.
  - [ ] Port selection, crop, remove dead, and physical delete actions.
  - [ ] Port search behavior.
  - [ ] Port playlist load/save dialogs.

- [ ] Port GStreamer player.
  - [ ] Create `playbin` pipeline.
  - [ ] Build audio sink chain: `audioconvert`, `audiopanorama`, `equalizer-10bands`, `spectrum`, sink.
  - [ ] Disable video with fake sink.
  - [ ] Port bus handling for EOS, errors, duration changes, tags, and spectrum messages.
  - [ ] Port play, stop, pause, unpause, toggle pause, seek, position, and duration.
  - [ ] Port bitrate, frequency, and channel reporting.
  - [ ] Port volume and balance.
  - [ ] Port output-device rebuild behavior.

- [ ] Port equalizer window.
  - [ ] Render equalizer skin and controls.
  - [ ] Port preamp slider.
  - [ ] Port ten band sliders.
  - [ ] Port active and auto toggles.
  - [ ] Sync equalizer state to GStreamer band properties.
  - [ ] Sync volume and balance with the main window.
  - [ ] Preserve shaded and detached modes.

- [ ] Port visualization behavior.
  - [ ] Port analyzer mode.
  - [ ] Port scope mode.
  - [ ] Port off mode.
  - [ ] Port current milkdrop placeholder behavior.
  - [ ] Port analyzer styles: bars and lines.
  - [ ] Port analyzer modes: normal, fire, vertical lines.
  - [ ] Port peaks and falloff speeds.
  - [ ] Port VU mode and refresh divisor.

- [ ] Port preferences window.
  - [ ] Port playback/output preferences.
  - [ ] Port playlist options.
  - [ ] Port font controls.
  - [ ] Port title format control.
  - [ ] Port visualization controls.
  - [ ] Port podcast cache TTL and refresh interval controls.
  - [ ] Preserve immediate apply/save behavior.

- [ ] Port skin browser.
  - [ ] Discover user skins in `~/.config/xmms/Skins/`.
  - [ ] Discover system skins in installed data directory.
  - [ ] Preview/select installed skin directories and archives.
  - [ ] Port reload skin behavior.
  - [ ] Port Alt+S shortcut integration.

- [ ] Port output device picker.
  - [ ] Enumerate GStreamer audio sink devices.
  - [ ] Deduplicate PipeWire/Pulse devices like C implementation.
  - [ ] Group local and network devices.
  - [ ] Preserve automatic system-default output.
  - [ ] Switch output device while preserving playback state.
  - [ ] Include Spotify devices when authenticated.

- [ ] Port MPRIS D-Bus interface.
  - [ ] Own `org.mpris.MediaPlayer2.xmms_resuscitated`.
  - [ ] Implement `org.mpris.MediaPlayer2`.
  - [ ] Implement `org.mpris.MediaPlayer2.Player`.
  - [ ] Port metadata, playback status, volume, and position properties.
  - [ ] Port Next, Previous, Pause, PlayPause, Stop, Play, Seek, SetPosition, and OpenUri.
  - [ ] Emit metadata and playback status changes.

- [ ] Port Spotify integration.
  - [ ] Preserve PKCE auth flow.
  - [ ] Preserve built-in client ID behavior.
  - [ ] Preserve refresh token storage in `spotify.conf`.
  - [ ] Port token refresh.
  - [ ] Port playlist fetch.
  - [ ] Port playlist track fetch.
  - [ ] Port Web API playback controls.
  - [ ] Port device listing and device transfer.
  - [ ] Port Spotify playback-state polling.
  - [ ] Integrate Spotify URI playback into player state.

- [ ] Port Spotify UI.
  - [ ] Port Spotify playlist chooser.
  - [ ] Port track import into playlist.
  - [ ] Port authentication prompts.
  - [ ] Port error handling and empty states.

- [ ] Port podcast integration.
  - [ ] Detect feed URLs vs direct audio streams.
  - [ ] Fetch RSS and Atom feeds.
  - [ ] Parse enclosures and media content URLs.
  - [ ] Preserve feed/guid metadata.
  - [ ] Cache podcast downloads under config dir.
  - [ ] Discover cached duration with GStreamer.
  - [ ] Retry 429 and 503 downloads with backoff.
  - [ ] Cleanup cache by TTL.
  - [ ] Refresh feeds on timer.
  - [ ] Skip failed current podcast item when needed.

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
  - [ ] Add GTK smoke tests under Xvfb.
  - [ ] Capture Rust screenshots for main, playlist, and equalizer windows.
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
