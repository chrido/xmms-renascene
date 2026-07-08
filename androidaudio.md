# Android audio backend plan

Date: 2026-07-07

This document sketches the first implementation step for introducing an audio
abstraction boundary so the desktop build can keep using GStreamer while the
Android build uses native Android audio through `rodio`/`cpal`.

## Goals

- Keep the application/controller layer independent of a concrete audio engine.
- Preserve the existing GStreamer desktop path and behavior.
- Add an Android-friendly backend built on `rodio`.
- Make backend differences explicit: playback control, events, metadata/duration
  probing, output devices, equalizer/balance, and visualization data.
- Allow incremental migration without blocking the current GTK/egui desktop app.

## First implementation step scope

The first step is **not** the Android APK, emulator, or UI/E2E port. The first
step is to make the audio layer platform-independent and add a `rodio` backend
that can run behind the same playback abstraction as GStreamer.

First-step deliverable:

- A `rodio-backend` Cargo feature builds on a normal development machine.
- The audio layer exposes a `Box<dyn PlaybackBackend>` factory for GStreamer or
  rodio. Frontend effect-handler migration to own that trait object is Step 2.
- GStreamer remains the default desktop backend.
- Rodio can play local seekable media through the backend abstraction.
- Android-only work such as APK packaging, emulator E2E, `content://` platform
  bridges, Android lifecycle, media session, and audio focus stays in later
  phases.

First-step non-goals:

- No Android APK build requirement yet.
- No emulator tests yet.
- No Android `content://` resolver implementation yet, though the resolver
  boundary should be designed so it can be added later.
- No streaming URL playback.
- No audible rodio equalizer/balance/spectrum implementation; store/no-op those
  controls until the DSP phase.

## Current audio shape in this repo

Important existing files:

- `src/player.rs`
  - Contains backend-neutral model types: `Player`, `PlayerState`,
    `PlaybackEvent`, `PlaybackTags`, `StreamInfo`, `OutputDevice`, etc.
  - Also contains the concrete `GStreamerBackend` implementation.
  - GStreamer currently provides playback, seek, position/duration queries,
    stream info, tags, output device listing, balance, preamp/equalizer, and
    spectrum events.
- `src/playback/backend.rs`
  - Already introduces a `PlaybackBackend` trait, but it only covers a small
    command subset and is not yet the real frontend boundary.
- `src/playback/gstreamer.rs`
  - Adapts `GStreamerBackend` to the current narrow `PlaybackBackend` trait.
- `src/ui.rs` and `src/ui/egui/app.rs`
  - Store concrete `GStreamerBackend` values and call GStreamer-specific methods
    directly.
  - Poll GStreamer bus messages and convert them to controller playback events.
  - Run GStreamer duration discovery for playlist entries.
- `src/playlist.rs`
  - Has a generic `index_missing_durations_with(...)` helper, but also a
    GStreamer-specific `index_missing_durations_with_gstreamer(...)` convenience
    method.

The controller side is already in good shape: `AppController` emits
`AppEffect::StartPlaybackUri`, `PausePlayback`, `SeekPlayback`,
`SetBackendVolume`, `SetBackendEqualizer`, etc. The missing boundary is between
frontend/runtime effect handling and the concrete audio implementation.

## Rodio research snapshot

Checked current crates.io/docs source for `rodio 0.22.2`.

### Dependency and features

`rodio 0.22.2`:

- License: `MIT OR Apache-2.0`.
- Rust version: `1.87` minimum. This repo currently builds with a newer Rust
  toolchain, so that should be acceptable.
- Playback is behind the `playback` feature and uses `cpal`.
- The default feature set also enables `recording`; we should disable default
  features for an output-only Android player.

Recommended initial dependency shape:

```toml
[features]
default = ["gtk-ui", "gstreamer-backend"]
gtk-ui = ["dep:gtk"]
egui-ui = ["dep:egui", "dep:eframe", "dep:rfd", "dep:zbus"]
gstreamer-backend = ["dep:gstreamer", "dep:gstreamer-pbutils"]
rodio-backend = ["dep:rodio"]
mobile-ui = []

[dependencies]
rodio = { version = "0.22.2", optional = true, default-features = false, features = [
  "playback",
  "mp3",
  "mp4",
  "vorbis",
  "flac",
  "wav",
] }
```

If we need broader codec/container coverage, consider enabling targeted
`symphonia-*` features or `symphonia-all`, but measure APK size. The current
playlist extension list includes formats beyond rodio's common default set
(e.g. WMA/WebM/Opus), so Android codec parity needs an explicit decision.

### Current rodio API to use

The current API is centered on:

- `rodio::DeviceSinkBuilder::open_default_sink()`
- `rodio::MixerDeviceSink`
- `rodio::Player::connect_new(handle.mixer())`
- `rodio::Decoder::try_from(std::fs::File)` or `Decoder::builder()`
- `rodio::Source` methods such as `total_duration()`, `channels()`,
  `sample_rate()`, and `try_seek(...)`
- `rodio::Player` controls:
  - `append(source)`
  - `pause()` / `play()`
  - `stop()` / `clear()` / recreate player
  - `try_seek(Duration)`
  - `get_pos()`
  - `set_volume(f32)`
  - `empty()`

Example shape:

```rust
use std::fs::File;
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, Player, Source};

let device_sink = DeviceSinkBuilder::open_default_sink()?;
let player = Player::connect_new(device_sink.mixer());

let file = File::open(path)?;
let source = Decoder::try_from(file)?;
let duration = source.total_duration();
let channels = source.channels();
let sample_rate = source.sample_rate();

player.append(source);
player.try_seek(Duration::from_millis(start_ms))?;
```

Keep `MixerDeviceSink` alive for as long as playback should be possible. If it
is dropped, audio stops.

### Android native audio stack

`rodio` uses `cpal` for playback. On Android, `cpal` uses the Android audio
backend through Oboe, which in turn uses AAudio on newer Android versions and
OpenSL ES as fallback where needed:

```text
xmms-renascene Rust code
  -> rodio
  -> cpal
  -> Oboe
  -> AAudio / OpenSL ES
  -> Android audio system
```

Output playback should not require microphone permissions. Build integration
will require an Android NDK/C++ toolchain because the Android audio backend uses
native code under the hood.

### Rodio limitations relevant to XMMS Renascene

- No GStreamer-style bus. We must synthesize `PlaybackEvent`s by tracking
  player state and polling `Player::empty()`, `Player::get_pos()`, known
  duration, stream errors, etc.
- Best seeking/duration behavior comes from `Decoder::try_from(File)` or a
  builder with byte length and a seekable `Read + Seek` source.
- Android `content://` URIs are not plain files. We need a URI resolver that can
  hand rodio a seekable reader, likely by using Android's `ContentResolver` /
  `ParcelFileDescriptor`, or by copying selected media into app-private storage.
- Rodio has volume control, but not XMMS-style balance, 10-band equalizer, or
  spectrum analyzer as direct backend controls.
  - Balance can be implemented as a custom `Source` adapter that scales left and
    right channels.
  - Equalizer needs a DSP/filter adapter or a dedicated DSP crate.
  - Spectrum visualization needs a tap/analyzer source adapter if Android parity
    requires live bands.
- Rodio does not provide desktop-style output device enumeration. On Android we
  should initially expose only the default route and hide/disable the output
  device preference.
- Metadata/tag extraction is not a rodio strength. For Android duration/title
  indexing we should either use rodio/Symphonia where practical plus existing
  ID3 helpers, or define a separate metadata probe abstraction.

## Proposed abstraction boundary

Replace concrete frontend ownership of `GStreamerBackend` with a backend trait
that covers the full runtime contract used by effect handling and polling.

### Backend-neutral data module

Move backend-neutral audio types out of `src/player.rs` over time:

- `PlayerState`
- `PlaybackEvent`
- `PlaybackTags`
- `StreamInfo`
- `OutputDevice`
- `OutputDeviceGroups`
- `OutputDeviceSelection`
- `EqualizerBackendState`

Suggested location:

```text
src/playback/model.rs      // backend-neutral types
src/playback/backend.rs    // traits and factory-facing types
```

`src/player.rs` can continue to re-export these during migration to avoid a big
call-site churn.

### Runtime playback trait

The existing `PlaybackBackend` trait should be expanded from a command-only
adapter into the real effect/polling boundary.

Sketch:

```rust
pub trait PlaybackBackend {
    fn play_uri(&mut self, uri: &str, start_ms: i64) -> Result<(), String>;
    fn pause(&mut self) -> Result<(), String>;
    fn resume(&mut self) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
    fn seek(&mut self, position_ms: i64) -> Result<(), String>;

    fn poll_events(&mut self) -> Result<Vec<PlaybackEvent>, String>;
    fn position_ms(&self) -> Option<i64>;
    fn duration_ms(&self) -> Option<i64>;
    fn stream_info(&self) -> StreamInfo;
    fn state(&self) -> PlayerState;

    fn set_volume(&mut self, volume_percent: i32) -> Result<(), String>;
    fn set_balance(&mut self, balance_percent: i32) -> Result<(), String>;
    fn set_equalizer(&mut self, state: EqualizerBackendState) -> Result<(), String>;

    fn output_devices(&self) -> Result<Vec<OutputDevice>, String> {
        Ok(Vec::new())
    }

    fn set_output_device(&mut self, _device: Option<&str>) -> Result<(), String> {
        Ok(())
    }
}
```

Notes:

- Use `&mut self` at the trait boundary. GStreamer can still use interior
  mutability internally; rodio will benefit from ordinary mutable state.
- Do not require `Send` initially. GTK currently uses `Rc<RefCell<_>>`, and the
  frontend owns/polls the backend on the UI thread. Android can revisit `Send`
  later if we move audio management to a dedicated runtime thread.
- Keep errors as `String` at first to match existing code. A structured
  `AudioError` can be introduced later.

### Metadata/duration probe trait

Playback and playlist indexing should be separated. GStreamer discovery and
rodio decoding have different capabilities and cost profiles.

Sketch:

```rust
pub trait AudioMetadataProbe {
    fn probe(&self, item: &DurationIndexItem) -> Result<Option<DurationIndexResult>, String>;
}
```

Implementations:

- `GStreamerMetadataProbe` using `gstreamer_pbutils::Discoverer`.
- `RodioMetadataProbe` using `rodio::Decoder`/`Source::total_duration()` for
  local seekable media, plus optional tag extraction using existing ID3 helpers
  or a Symphonia-based helper.
- `NoopMetadataProbe` for builds where duration indexing is not available.

Then both GTK and egui scheduling can call shared code instead of embedding
GStreamer-specific threads.

### Playlist audio length requirement

Yes: playlist audio length/duration indexing is a required part of the audio
layer port, not an Android-only later task. The playlist UI currently displays
per-track lengths and footer totals from `PlaylistEntry.length_ms`, and those
values are populated by `DurationIndexResult`. The rodio path must therefore
provide a replacement for GStreamer's duration discovery.

First-step behavior:

- For local seekable files and `file://` URIs, `RodioMetadataProbe` opens the
  file, builds a `rodio::Decoder`, reads `Source::total_duration()`, and returns
  `DurationIndexResult { length_ms, title }`.
- If rodio cannot determine the duration, keep the existing convention of
  `length_ms = -1` so the playlist can omit the time rather than showing a wrong
  value.
- For malformed/unsupported media, log/return a probe error and continue
  scanning the rest of the playlist.
- Title extraction is best-effort in the first step. Duration is the required
  field; title can continue using existing filename/ID3 fallback behavior until
  a richer metadata layer is added.

Later behavior:

- For Android `content://` URIs, the Android resolver should expose a seekable
  file descriptor or copy to app-private storage, so the same rodio duration
  probe can run.
- For streaming URLs, duration may remain unknown unless we add a cache/probe
  layer that can determine it reliably.

### Backend factory

Add a small factory so frontends ask for "the configured/default backend"
instead of constructing `GStreamerBackend` directly.

Sketch:

```rust
pub enum PlaybackBackendKind {
    Auto,
    GStreamer,
    Rodio,
}

pub fn create_playback_backend(kind: PlaybackBackendKind) -> Result<Box<dyn PlaybackBackend>, String> {
    match kind {
        PlaybackBackendKind::Auto => create_default_playback_backend(),
        PlaybackBackendKind::GStreamer => create_gstreamer_backend(),
        PlaybackBackendKind::Rodio => create_rodio_backend(),
    }
}
```

Default selection:

```rust
#[cfg(all(target_os = "android", feature = "rodio-backend"))]
// Auto -> Rodio

#[cfg(all(not(target_os = "android"), feature = "gstreamer-backend"))]
// Auto -> GStreamer
```

This keeps Android from linking GStreamer and keeps desktop defaults unchanged.

## Rodio backend design sketch

Suggested file:

```text
src/playback/rodio.rs
```

Initial structure:

```rust
pub struct RodioBackend {
    device_sink: rodio::MixerDeviceSink,
    player: rodio::Player,
    uri_resolver: Box<dyn AudioUriResolver>,
    current_uri: Option<String>,
    state: PlayerState,
    duration_ms: Option<i64>,
    stream_info: StreamInfo,
    pending_events: Vec<PlaybackEvent>,
    volume_percent: i32,
    balance_percent: i32,
    equalizer: EqualizerBackendState,
    emitted_eos_for_current: bool,
}
```

`play_uri(uri, start_ms)` flow:

1. Stop/recreate the rodio `Player` to guarantee a clean queue.
2. Resolve `uri` into a seekable source.
3. Build a `rodio::Decoder`.
4. Read `total_duration()`, `channels()`, and `sample_rate()` before appending.
5. Wrap source with future adapters for balance/equalizer/visualization.
6. Append source to the player.
7. Apply volume.
8. If `start_ms > 0`, call `try_seek(Duration::from_millis(start_ms))`.
9. Set state to `Playing`.
10. Queue synthetic events:
    - `DurationChanged(duration_ms)`
    - `StreamInfo(...)`

`poll_events()` flow:

1. Drain any internal stream-error channel into `PlaybackEvent::Error`.
2. If playing and `player.empty()` transitions true, emit `EndOfStream` once.
3. Optionally emit stream info or duration changes if the backend learned new
   values.
4. Return queued events.

Position:

- `position_ms()` maps `player.get_pos()` to milliseconds.
- The existing app currently also ticks position in the controller. We should
  decide whether to keep controller ticking as the display source of truth or
  periodically reconcile with backend position for Android. For first pass,
  retain existing ticking and use backend position only for explicit queries or
  drift correction.

Stop/pause/resume:

- `pause()` -> `player.pause()`, state `Paused`.
- `resume()` -> `player.play()`, state `Playing`.
- `stop()` -> `player.stop()` or recreate `Player`, state `Stopped`, clear
  current URI/duration if needed.

Volume:

- `volume_percent` 0..100 maps to rodio volume `0.0..1.0` using
  `player.set_volume(volume as f32 / 100.0)`.

Balance/equalizer phase 1:

- Accept and store values.
- Return `Ok(())`.
- Clearly mark as not yet audible on Android until DSP adapters are added.

Balance/equalizer phase 2:

- Introduce `Source` wrappers:
  - `BalanceSource<S>`: per-channel gain adjustment.
  - `EqualizerSource<S>`: 10-band DSP. Either implement biquad filters or use a
    focused DSP crate.
  - `AnalyzerSource<S>`: copies samples into a ring buffer/FFT analyzer and
    emits/synthesizes `PlaybackEvent::Spectrum`.

## Android URI resolution

Add a resolver boundary instead of assuming every playlist item is a `file://`
path.

Sketch:

```rust
pub trait AudioUriResolver {
    fn open(&self, uri: &str) -> Result<AudioReader, String>;
    fn hint(&self, uri: &str) -> Option<String>;
}
```

Implementation options:

1. **Fast first pass: local files only**
   - Support app-private files and `file://` URIs with `std::fs::File`.
   - Android file picker/import copies media to app-private storage.
   - Easiest to make seek/duration work with rodio.
2. **Full Android storage access**
   - Support `content://` URIs from the Storage Access Framework.
   - Java/Kotlin side opens a `ParcelFileDescriptor` through `ContentResolver`.
   - Rust duplicates/owns the fd and wraps it as `std::fs::File`.
   - Confirm seekability and lifetime rules.
3. **Streaming URLs**
   - Rodio can decode from a reader, but reliable seeking/duration usually needs
     a seekable source.
   - For HTTP streams we likely need a downloader/cache layer or accept limited
     controls.

Recommendation: start with option 1 unless Android GUI already commits to
long-lived `content://` playlist entries.

## Migration TODO list

### Step 1: platform-independent rodio audio layer (current first step)

Goal: introduce rodio behind the playback abstraction and make it testable on a
normal host machine before doing any Android packaging or emulator work.

TODO:

- [x] Add optional `rodio` dependency and `rodio-backend` Cargo feature.
- [x] Keep `gstreamer-backend` as the desktop default feature.
- [x] Expand `src/playback/backend.rs` from a command-only trait into the full
      runtime boundary:
  - [x] `play_uri(uri, start_ms)`
  - [x] `pause()` / `resume()` / `stop()`
  - [x] `seek(position_ms)`
  - [x] `poll_events()`
  - [x] `position_ms()` / `duration_ms()` / `stream_info()` / `state()`
  - [x] `set_volume()` / `set_balance()` / `set_equalizer()`
  - [x] optional output-device methods with harmless defaults
- [x] Move or re-export backend-neutral types through `src/playback/model.rs`:
  - [x] `PlayerState`
  - [x] `PlaybackEvent`
  - [x] `PlaybackTags`
  - [x] `StreamInfo`
  - [x] `OutputDevice` and related grouping/selection types
  - [x] `EqualizerBackendState`
- [x] Add a backend factory that can select `Auto`, `GStreamer`, or `Rodio`.
- [x] Implement the expanded trait for the existing `GStreamerBackend` without
      changing desktop behavior.
- [x] Add `src/playback/rodio.rs` with `RodioBackend`.
- [x] Implement platform-independent local URI resolution:
  - [x] support plain absolute paths if the app passes them,
  - [x] support existing `file://` playlist URIs,
  - [x] return a clear error for unsupported schemes such as `content://` and
        `http://` for now.
- [x] Implement rodio playback basics:
  - [x] create and retain `rodio::MixerDeviceSink`,
  - [x] create/recreate `rodio::Player` per track or stop/reset,
  - [x] decode with `rodio::Decoder::try_from(File)` for seekable local files,
  - [x] capture duration, sample rate, and channel count before appending,
  - [x] append the decoded source,
  - [x] apply start seek when requested,
  - [x] map volume 0..100 to rodio volume 0.0..1.0.
- [x] Synthesize backend events that GStreamer currently provides via bus
      messages:
  - [x] `DurationChanged`,
  - [x] `StreamInfo`,
  - [x] `EndOfStream` when `rodio::Player::empty()` transitions while playing,
  - [x] `Error` for decode/open/device failures.
- [x] Implement position/duration queries using rodio state:
  - [x] `position_ms()` from `Player::get_pos()`,
  - [x] `duration_ms()` from decoded `Source::total_duration()`.
- [x] Store, but do not audibly apply, Android-v1-deferred controls:
  - [x] balance,
  - [x] equalizer/preamp,
  - [x] spectrum analyzer data is a no-data/no-op path for Step 1.
- [x] Add `AudioMetadataProbe` boundary for playlist duration indexing.
- [x] Add `RodioMetadataProbe` for local seekable files using
      `rodio::Decoder`/`Source::total_duration()`.
- [x] Keep `content://`, streaming URLs, Android lifecycle, APK packaging, and
      Android E2E as later steps.

Deliverable: a platform-independent rodio backend that can be built and tested
on the host, with desktop GStreamer behavior unchanged.

### Step 2: migrate frontend effect handling to the backend trait

Goal: make GTK/egui playback effect handling independent from the concrete
GStreamer type.

TODO:

- [ ] Replace `Option<GStreamerBackend>` in `src/ui/egui/app.rs` with a backend
      trait object or small owning wrapper.
- [ ] Replace `Option<Rc<RefCell<GStreamerBackend>>>` in `src/ui.rs` similarly,
      or introduce a GTK-compatible wrapper during migration.
- [ ] Route `StartPlaybackUri`, pause/resume/stop, seek, volume, balance, and
      equalizer effects through `PlaybackBackend`.
- [ ] Route event polling through `PlaybackBackend::poll_events()`.
- [ ] Route stream-info polling through `PlaybackBackend::stream_info()`.
- [ ] Hide, disable, or no-op output-device preferences when the active backend
      does not support output device selection.
- [ ] Keep GStreamer-specific UI wording/preferences only behind
      `gstreamer-backend` or active-backend checks.

Deliverable: frontend code no longer imports or names `GStreamerBackend` except
inside the GStreamer adapter/factory.

### Step 3: abstract duration indexing

Goal: make playlist duration scanning backend-neutral.

TODO:

- [ ] Introduce `AudioMetadataProbe`.
- [ ] Move duplicated duration-indexing thread logic out of GTK/egui frontends.
- [ ] Provide `GStreamerMetadataProbe` using `gstreamer_pbutils::Discoverer`.
- [ ] Provide `RodioMetadataProbe` for local seekable files.
- [ ] Provide `NoopMetadataProbe` for builds without a probe.
- [ ] Replace `Playlist::index_missing_durations_with_gstreamer()` with a
      generic helper or retain it only as a compatibility wrapper.

Deliverable: playlist duration code no longer hard-depends on GStreamer from
frontend modules.

### Step 4: Android URI bridge and platform integration

Goal: after the platform-independent rodio backend works, add Android-specific
file access and lifecycle behavior.

TODO:

- [ ] Wire `Auto` backend selection to rodio for
      `target_os = "android" && feature = "rodio-backend"`.
- [ ] Confirm the Android UI/runtime creates the backend after the Activity is
      initialized.
- [ ] Keep `MixerDeviceSink` alive for the app lifetime or active audio-service
      lifetime.
- [ ] Implement Android `content://` support through a platform resolver:
  - [ ] Java/Kotlin `ContentResolver` opens a `ParcelFileDescriptor`,
  - [ ] Rust receives/duplicates the fd and wraps it as `std::fs::File`,
  - [ ] verify seekability for rodio duration and seek support,
  - [ ] fall back to copying into app-private storage if needed.
- [ ] Keep app-private/local `file://` support.
- [ ] Add lifecycle handling:
  - [ ] pause or continue on Activity pause according to product decision,
  - [ ] release/recreate backend on route/device errors if needed,
  - [ ] preserve playback/app state across Activity recreation.

Deliverable: Android app can play local/app-private files and SAF `content://`
media through the same rodio backend abstraction.

### Step 5: APK build environment

Goal: make debug/release APK creation reproducible.

TODO:

- [ ] Add a dedicated Android build Docker image, separate from
      `e2e/Dockerfile`.
- [ ] Install Rust Android targets:
  - [ ] `aarch64-linux-android`,
  - [ ] optionally `armv7-linux-androideabi`,
  - [ ] optionally `x86_64-linux-android`.
- [ ] Install Android SDK command-line tools, platform tools, build tools, and a
      pinned Android platform.
- [ ] Install a pinned Android NDK.
- [ ] Install JDK 17, Gradle/project wrapper support, CMake, and Ninja.
- [ ] Decide packaging path:
  - [ ] Gradle + `cargo-ndk`, or
  - [ ] `cargo-apk`.
- [ ] Add repo commands for debug APK and release APK builds.
- [ ] Add release signing through mounted secrets/CI secrets, not baked into the
      image.

Deliverable: CI or a developer can create APKs from a clean Docker environment.

### Step 6: Android E2E test harness

Goal: test Android behavior with adb/emulator/device without trying to reuse
Xvfb directly.

TODO:

- [ ] Keep current desktop Xvfb tests as the fast parity suite.
- [ ] Add an E2E driver abstraction under `e2e/drivers/`.
- [ ] Implement `X11Driver` with existing `xdotool` helpers when tests are
      gradually refactored.
- [ ] Implement `AndroidDriver` with `adb` tap/swipe/keyevent/screencap.
- [ ] Add Android E2E control-socket commands:
  - [ ] `ready`,
  - [ ] `geometry`,
  - [ ] `state`,
  - [ ] `seed_playlist`,
  - [ ] `open_file_uri`,
  - [ ] `last_error`.
- [ ] Add initial Android smoke tests for launch, render, taps, sliders, and
      basic playback.
- [ ] Add Android audio tests for local files, `content://`, pause/resume,
      seek, and EOS.
- [ ] Add pytest markers for Android subsets.

Deliverable: Android emulator/device smoke tests run through adb and shared
logical skin coordinates.

### Step 7: parity features after first Android audio version

Goal: restore audio/visual parity features that rodio does not provide directly.

TODO:

- [ ] Implement balance as a custom rodio `Source` adapter.
- [ ] Implement 10-band equalizer/preamp with DSP filters or a dedicated DSP
      crate.
- [ ] Implement live spectrum analyzer data by tapping decoded samples before
      rodio output.
- [ ] Revisit codec/container support and APK size.
- [ ] Add streaming URL playback.
- [ ] Add Android media session, media buttons, and audio focus if required for
      release polish.

Deliverable: Android rodio backend reaches feature parity targets beyond basic
playback.

## Build/APK environment

The existing repository Docker image (`e2e/Dockerfile`) is for desktop Linux GUI
E2E testing. It has Rust, GTK, GStreamer, Xvfb, and screenshot tooling, but it
is not an Android build environment. We should keep it separate and create a new
Android build image instead of bloating the desktop E2E image.

A reproducible APK build environment needs:

- Linux container host with Docker or Podman.
- Rust toolchain plus Android Rust targets:
  - `aarch64-linux-android` first,
  - optionally `armv7-linux-androideabi` and `x86_64-linux-android` for wider
    device/emulator coverage.
- Android SDK command-line tools:
  - `platform-tools`,
  - one Android platform, e.g. `platforms;android-35`,
  - matching build tools, e.g. `build-tools;35.0.0`.
- Android NDK, e.g. a pinned modern version such as `27.x` or `28.x`.
- Java JDK 17 and Gradle or the project Gradle wrapper.
- C/C++ build helpers (`cmake`, `ninja`) because rodio's Android path goes
  through `cpal`/Oboe/native Android audio.
- Rust Android helper:
  - `cargo-ndk` if the APK is produced by a Gradle Android project that embeds
    Rust as a native library,
  - or `cargo-apk` if the Rust app owns the Android Activity/native app
    packaging.

Recommended container approach:

1. Use a public Android SDK image as the base, then install Rust and
   `cargo-ndk`/`cargo-apk`.
   - Good base candidates: `ghcr.io/cirruslabs/android-sdk`, `cimg/android`, or
     another maintained Android SDK image with pinned tags.
2. Or build from `rust:bookworm` and install Android command-line tools with
   `sdkmanager`.
   - This is more verbose but maximally explicit and reproducible.

Suggested separate file:

```text
android/Dockerfile
```

Sketch:

```Dockerfile
FROM rust:1-bookworm

ARG ANDROID_API=35
ARG ANDROID_BUILD_TOOLS=35.0.0
ARG ANDROID_NDK_VERSION=27.2.12479018

ENV ANDROID_HOME=/opt/android-sdk \
    ANDROID_SDK_ROOT=/opt/android-sdk \
    ANDROID_NDK_HOME=/opt/android-sdk/ndk/${ANDROID_NDK_VERSION} \
    PATH=/opt/android-sdk/cmdline-tools/latest/bin:/opt/android-sdk/platform-tools:${PATH}

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl unzip git openjdk-17-jdk cmake ninja-build pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Install Android command-line tools, then:
# sdkmanager --licenses
# sdkmanager \
#   "platform-tools" \
#   "platforms;android-${ANDROID_API}" \
#   "build-tools;${ANDROID_BUILD_TOOLS}" \
#   "ndk;${ANDROID_NDK_VERSION}"

RUN rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android \
    && cargo install cargo-ndk cargo-apk
```

Build command shape for a Gradle + Rust native-library packaging path:

```sh
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs \
  build --release --no-default-features --features mobile-ui,rodio-backend
./gradlew assembleDebug
```

Build command shape for a `cargo-apk` packaging path:

```sh
cargo apk build --release --no-default-features --features mobile-ui,rodio-backend
```

APK signing:

- Debug APKs can be signed by Gradle/cargo-apk with debug keys.
- Release APKs should mount the keystore into the container as a secret/volume
  and pass signing passwords through CI secrets, not bake them into the image.

What Docker can and cannot cover:

- Good fit: deterministic Rust cross-compiles and debug/release APK creation.
- Good fit: CI builds with cached Cargo/Gradle/Android SDK directories.
- Limited fit: emulator/device testing. Android emulators in Docker generally
  need privileged containers and KVM access. For smoke testing, it is usually
  simpler to install the APK on a physical device via host `adb`, or run emulator
  tests in CI infrastructure designed for Android emulators.

## Android E2E testing strategy

The existing X11/Xvfb tests should not be mapped by trying to run `xdotool` in
or against an Android emulator. The emulator is not an X server. Instead, keep
the existing coordinate-test intent, but put a small driver layer underneath it:

```text
existing test intent / skin rectangles
  -> E2E driver interface
     -> X11 driver: xdotool + Xvfb + import/xwd
     -> Android driver: adb/Appium actions + screencap + logcat/control socket
```

### Driver API to introduce in `e2e/`

Refactor direct `run_xdotool(...)` usage behind methods such as:

- `launch_app(...)`
- `wait_ready()`
- `viewport_rect()` / `scale_factor()`
- `tap_skin_rect(rect)`
- `press_skin_rect(rect)` / `release()` or `press_with_screenshot(rect)`
- `drag_skin_rect(rect, end_fraction, horizontal)`
- `key_escape_or_back()`
- `screenshot(path)`
- `assert_log_contains(...)` or `control_socket.command(...)`

Then:

- Desktop tests keep using an `X11Driver` implemented with `xdotool` and Xvfb.
- Android tests use an `AndroidDriver` implemented with `adb` and, where needed,
  Appium/UiAutomator-style pointer actions.

### Mapping X11 operations to Android

| Existing X11 operation | Android equivalent |
| --- | --- |
| `xdotool search --name ...` | Wait for package/activity with `adb shell dumpsys activity` or app-ready control socket. |
| Window activate/focus/raise/move | Usually no-op; ensure foreground with `adb shell am start ...` or `monkey -p`. |
| Window-relative click | Convert skin coordinate to physical screen coordinate, then `adb shell input tap x y`. |
| Drag slider/scrollbar | `adb shell input swipe x1 y1 x2 y2 duration` or Appium pointer action. |
| Press-and-hold screenshot | Prefer Appium/W3C pointer down + pause + screenshot + pointer up. For simpler smoke tests, run `adb shell input swipe x y x y duration` asynchronously and capture during the hold. |
| `Escape` for menus/dialogs | `adb shell input keyevent KEYCODE_BACK`. |
| Screenshot via `import`/`xwd` | `adb exec-out screencap -p > screenshot.png`. |
| Console stdout assertions | Android `logcat` tags, or better: existing E2E control socket through `adb forward tcp:HOST tcp:DEVICE`. |

### Coordinate mapping

Keep the same skin rectangle constants from `e2e/gui.py`. The Android driver
should convert them to device pixels using app-reported geometry:

```text
screen_x = viewport_left + round(skin_x * skin_scale)
screen_y = viewport_top  + round(skin_y * skin_scale)
```

Do not guess status-bar/navigation-bar offsets from screenshots. Add a debug/E2E
geometry endpoint or log line in the Android app that reports:

- content viewport left/top in physical pixels,
- skin scale in physical pixels,
- active panel layout positions,
- current scale factor / density if relevant.

This makes desktop and Android tests share the same logical coordinates while
allowing Android to run fullscreen, edge-to-edge, or letterboxed without fragile
hard-coded offsets.

### App control and observability

The current desktop E2E suite already has a JSON-lines control socket. Reuse the
same idea on Android in debug/E2E builds:

- Start the app with E2E mode via intent extras, e.g. `adb shell am start ...
  --ez e2e true --ei control_port 28452`.
- Bind the app's control socket on device loopback.
- Use `adb forward tcp:28452 tcp:28452` so the Python test client can connect
  from the host.
- Use it for readiness, current state, geometry, playlist seeding, and precise
  assertions.

Use `logcat` as a secondary source, not the primary assertion mechanism. A
control socket is less flaky than parsing Android logs.

### What to port first

Do not port every Xvfb test 1:1 at the start. Use a smaller Android smoke suite:

1. Install and launch APK; wait for app-ready control socket.
2. Capture initial screenshot and verify the player is rendered.
3. Tap core transport buttons: play, pause, stop, next, previous.
4. Drag volume and position sliders.
5. Open/close equalizer and playlist panels if Android GUI supports them.
6. Playback smoke with a bundled or pushed test track:
   - play,
   - pause/resume,
   - seek,
   - EOS advances/stops as expected.
7. One `content://` playback test using a test provider or SAF fixture.

Keep the full bitmap/button parity tests on Xvfb for speed and determinism, and
use Android E2E for platform integration and audio/backend confidence.

### Emulator/CI approach

- Building APKs in Docker is straightforward.
- Running emulators inside Docker is possible but usually painful because it
  needs KVM and privileged/container runtime setup.
- Prefer one of:
  - host/CI-managed emulator with KVM, running tests via `adb`,
  - Gradle Managed Devices,
  - GitHub Actions-style Android emulator runner,
  - physical device smoke tests for release validation.

The Android Docker image can build the APK and host-side Python test tools. The
actual emulator can run outside the container, with `adb` exposed to the test
runner, or in specialized CI that supports Android emulators.

### Proposed Android E2E file layout

Keep the existing desktop tests working while adding Android-specific tests and
shared driver helpers:

```text
e2e/
  gui.py                    # existing skin constants and shared geometry helpers
  drivers/
    base.py                 # abstract E2E driver protocol
    x11.py                  # xdotool/Xvfb implementation
    android.py              # adb/Appium implementation
  test_android_smoke.py     # Android launch/render/control smoke tests
  test_android_audio.py     # rodio playback, seek, EOS, file:// and content://
```

The first refactor should be small: do not rewrite every existing test. Add the
new driver interface for Android tests first, then gradually move reusable X11
helpers behind the same interface when touching those tests.

### Proposed runner commands

Add repo commands separate from desktop `pye2e`:

```sh
./repo android-apk          # build debug APK in the Android build image
./repo android-e2e          # install APK and run pytest Android tests over adb
./repo android-e2e-docker   # build/test from Android Docker image when adb is available
```

Useful environment variables:

```sh
XMMS_ANDROID_APK=android/app/build/outputs/apk/debug/app-debug.apk
XMMS_ANDROID_PACKAGE=org.xmms.renascene
XMMS_ANDROID_ACTIVITY=.MainActivity
ANDROID_SERIAL=emulator-5554        # or physical device serial
XMMS_ANDROID_E2E_PORT=28452
XMMS_E2E_SCREENSHOT_DIR=testoutput/android
```

The Android E2E runner should roughly do:

1. `adb wait-for-device`.
2. `adb install -r "$XMMS_ANDROID_APK"`.
3. `adb forward tcp:$XMMS_ANDROID_E2E_PORT tcp:$XMMS_ANDROID_E2E_PORT`.
4. Launch with E2E intent extras.
5. Wait for the control socket `ready` response.
6. Run only Android-marked pytest tests, e.g. `pytest e2e -m android`.
7. Pull/capture screenshots and logs into `testoutput/android`.

### Android E2E control socket additions

The desktop control socket is already a good starting point. Android E2E should
add or expose these commands in debug/E2E builds:

- `ready`: app initialized and first frame rendered.
- `geometry`: returns viewport origin, skin scale, visible panels, and panel
  rects in physical pixels.
- `state`: returns player state, playlist position, current URI, current
  playback position/duration, volume, balance, and panel visibility.
- `seed_playlist`: create a deterministic playlist from files pushed into the
  app sandbox or from provided URIs.
- `open_file_uri`: optional helper to copy/publish a pushed test file into the
  app-private location and return the app's playable URI.
- `last_error`: returns the latest backend/UI error message for assertions.

For `content://` tests, prefer a deterministic fixture over manual picker UI.
Options:

- add a small debug-only Android `ContentProvider` that serves test audio files,
- or install a companion test APK/provider,
- or copy media into `MediaStore` and query its resulting content URI.

The first option is usually simplest and most repeatable.

### Android test categories

Use pytest markers so CI can run fast subsets:

```text
android              any Android/emulator test
android_smoke        launch/render/basic tap tests
android_audio        rodio playback and backend behavior
android_content_uri  SAF/content URI resolver tests
android_slow         longer emulator/device scenarios
```

Keep these separate from the existing `gtk`/`egui` desktop matrix. Android tests
should not require `DISPLAY`, `xvfb-run`, or `xdotool`.

## Testing and verification plan

### Step 1 testing: platform-independent rodio backend

The first-step tests should run on a normal development host and in regular CI.
They should not require an Android SDK, APK, emulator, physical device, or Xvfb.

TODO:

- [ ] Add small audio fixtures for tests, preferably generated during the test
      or checked in as tiny WAV/OGG files.
- [ ] Add unit tests for backend-neutral model and trait behavior:
  - [ ] `GStreamerBackend` still implements `PlaybackBackend` when the
        `gstreamer-backend` feature is enabled,
  - [ ] `RodioBackend` implements `PlaybackBackend` when the `rodio-backend`
        feature is enabled,
  - [ ] backend factory selects the expected backend for feature/target combos.
- [ ] Add local URI resolver tests:
  - [ ] absolute path resolves to a file,
  - [ ] `file://` URI resolves to a file,
  - [ ] missing file returns a useful error,
  - [ ] unsupported `content://` and `http://` return explicit
        "not supported in platform-independent rodio step" errors.
- [ ] Add decoder/probe tests that do not open a physical audio device:
  - [ ] `RodioMetadataProbe` reports duration for a local WAV/OGG fixture,
  - [ ] decoder extracts channel count and sample rate,
  - [ ] invalid/corrupt media returns an error event or probe failure.
- [ ] Add rodio backend core tests without depending on real audio hardware:
  - [ ] synthetic `DurationChanged` event is queued after successful decode,
  - [ ] synthetic `StreamInfo` event is queued after successful decode,
  - [ ] synthetic `EndOfStream` is emitted once when the underlying player is
        observed empty,
  - [ ] volume clamps and maps 0..100 to 0.0..1.0,
  - [ ] balance/equalizer setters store state and return `Ok(())` as first-step
        no-ops.
- [ ] Keep real-device rodio playback as an ignored or opt-in smoke test:
  - [ ] mark it `#[ignore]`, or
  - [ ] require `XMMS_RODIO_PLAYBACK_SMOKE=1`,
  - [ ] skip cleanly if no default audio output device is available.
- [ ] Add compile/build checks:

```sh
cargo test --no-default-features --features rodio-backend
cargo test --no-default-features --features egui-ui,rodio-backend
cargo test --features gstreamer-backend
cargo build --no-default-features --features mobile-ui,rodio-backend
```

- [ ] If CI permits, add a Linux host smoke command that exercises the rodio
      backend with a generated WAV but skips when no audio device exists.

Acceptance criteria for Step 1:

- GStreamer desktop tests still pass.
- Rodio backend builds without GStreamer.
- Rodio can decode and probe local seekable media.
- Rodio backend behavior is verified through unit/core tests without requiring a
  sound card.
- Optional host playback smoke can play a local test file through rodio.
- No Android SDK/APK/emulator setup is required to finish this step.

### Later Android testing

These are not required for Step 1, but remain required for the Android release
path:

- [ ] Cross-compile check for Android target once build tooling is chosen:
  - [ ] `aarch64-linux-android` first.
- [ ] Build APK in the Android Docker image using either the Gradle +
      `cargo-ndk` path or the `cargo-apk` path.
- [ ] Device/emulator smoke tests:
  - [ ] app starts without GStreamer libraries,
  - [ ] opens default audio output,
  - [ ] plays local MP3/OGG/FLAC/WAV/M4A,
  - [ ] plays Android SAF `content://` media,
  - [ ] pause/resume/seek/stop work,
  - [ ] EOS advances playlist,
  - [ ] app background/foreground behavior is acceptable,
  - [ ] no crash if audio route changes or backend initialization fails.

## Follow-up decisions and fix-later items

Resolved for the first Android audio implementation:

- Android playlists/backend resolution must support both app-private/local
  `file://` paths and Android Storage Access Framework `content://` URIs.
- Streaming URL playback is not required for the first version.
- Audible equalizer, balance, and live spectrum visualization are not required
  for the first version; backend calls may store state/no-op until DSP support
  is added.

Fix later / post-v1 work:

- Add streaming URL playback support after local and `content://` playback are
  stable.
- Implement Android balance as a custom rodio `Source` adapter.
- Implement the 10-band equalizer/preamp using DSP filters or a dedicated DSP
  crate.
- Implement live spectrum analyzer data by tapping decoded samples before they
  reach rodio output.

Remaining open questions:

1. Which Android build/runtime stack are we using: `cargo-apk`, `cargo-ndk` with
   Gradle, or another setup?
2. Which formats are mandatory on Android for release parity? In particular,
   should WMA/WebM/Opus be supported or can the first release match rodio's
   common codec set?
3. Should Android obey audio focus / media buttons / lock-screen controls in the
   first pass, or is that a later media-session task?
