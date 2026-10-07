# Android generated media artwork: implementation TODO

## Goal and checklist rules

Display per-song, MilkDrop-inspired **static artwork using the active skin's
visualization colors** in Android's native media controls. It must work without
an Activity and must not disturb playback, transport controls, or audio focus.

Work through the numbered sections **from top to bottom**. Complete each section's
validation gate before proceeding. Tests are part of the task, not a final
follow-up. All implementation boxes are unchecked because this feature has not
been implemented or measured by this document.

The initial image is deterministic, track-seeded procedural art, **not an analysis
of the song's audio**. Start with a 320 × 320 bitmap. A 512 × 512 PNG/URI delivery
path is conditional on the phone compatibility spike or an explicitly approved
high-resolution requirement. These are application defaults, not Android mandates.

**Conditional work:** at the section 7 decision gate, either implement section 8
or record “not applicable: bitmap-only selected” and skip it. Do not mark skipped
work as implemented. Section 12 is a separate future backlog, not a v1 gate.

## 1. Establish the baseline and implementation boundaries

- [ ] Inspect the integration points listed in the reference table below against
  the current checkout; update any stale symbols before changing code.
- [ ] Record baseline behavior: no session artwork or notification large icon;
  working foreground/background transport, seek, queue, widget play, and cold
  media-browser/service start.
- [ ] Build/install the API 35 emulator APK and run the existing Android E2E suite;
  record existing failures separately from this feature.
- [ ] Run the existing host tests; record environment prerequisites or failures
  before adding new tests.
- [ ] Establish v1 scope: `Generated` and `Off`, with `Off` as the development
  default. Defer the shipping-default decision to section 11.
- [ ] Document invariants: no animation/projectM/GL readback, no egui-surface
  dependency, no bitmap Intent extras/browser-list artwork, no playlist-wide
  pregeneration, and no new I/O or rendering on the audio thread.
- [ ] Record the main-thread/I/O boundary: existing widget/config costs are baseline
  behavior, not fixed by this feature; do not copy them into new artwork work.
- [ ] **Gate:** baseline artifacts and scope are documented; unrelated failures
  have a disposition and will not be mistaken for artwork regressions.

Baseline commands:

```bash
cargo test --locked --all-targets
cargo test --locked --no-default-features --features egui-ui,rodio-backend --lib --bins
./repo android-apk-emulator
XMMS_E2E_ANDROID_SKIP_BUILD=1 ./repo pye2e -m android
```

## 2. Implement frontend-neutral identity and request state

- [ ] Add `src/artwork.rs` (or a small `src/artwork/` module), exported from
  `src/lib.rs`, independently of GTK/egui/mobile feature gates. Keep Android-only
  glue out of this module.
- [ ] Define immutable `ArtworkSpec`: renderer version, stable track seed, 24 RGB
  palette entries, width, and height.
- [ ] Define request identity: service epoch, request generation, track revision,
  normalized track identity, skin revision, and policy revision.
- [ ] Choose a stable seed algorithm and explicit serialization. Do not use
  randomized runtime hashes, playlist index, or whole-song file hashing.
- [ ] Define normalization for plain paths and `file://` URIs, with lexical
  normalization/percent decoding and no main-thread filesystem canonicalization.
  Normalize known app-private `/data/data/<package>` and `/data/user/0/<package>`
  aliases only when they represent this app's actual storage. Keep `content://`
  identities opaque unless imports already map them to a private file.
- [ ] Define render-cache keys from full normalized track identity, palette bytes,
  dimensions and renderer version. Preserve full fields internally; short-hash
  equality alone is insufficient. Duplicate entries for the same URI share art;
  distinct files with the same title do not.
- [ ] Implement pure latest-wins state transitions: one running request, one
  replaceable pending request, deduplication, cancellation and stale-result
  rejection. Add revision validation for delayed foreground projections.
- [ ] Add unit tests for stable keys, path/URI aliases, duplicate identities,
  playlist reorder/replacement, out-of-order results, settings revisions, stop,
  and service replacement. Use fake clocks/workers rather than timing sleeps.
- [ ] Wire pure tests into the existing default `--all-targets` and egui
  `--lib --bins` CI sets. Public entry points may be integration-tested in
  `tests/android_artwork.rs`; private state can stay unit-tested.
- [ ] **Gate:** pure tests pass in both host feature sets. Integration tests do not
  depend on `pub(crate)` Android modules or library `cfg(test)`. Do not use
  `egui-core,rodio-backend` to exercise egui frontend code: that set excludes it.

## 3. Implement the square skin-colored renderer

- [ ] Add a public entry point in `src/render/artwork.rs` and export it through
  `src/render.rs`. Keep the classic 76×16 MilkDrop-inspired renderer unchanged.
- [ ] Generate a bounded scalar intensity/index field using normalized square
  coordinates and track-seeded tunnel/plasma/wave/particle patterns. Set explicit
  limits on layers and particles; keep important patterns clear of cropped edges.
- [ ] Map the field exclusively through the supplied skin palette: clamp to
  `[0, 23]`, interpolate adjacent entries in encoded RGB, round to 8-bit channels,
  and set alpha to 255. Background uses entry 0; accents may use entries 18–23.
- [ ] Apply glow/layering to the intensity field before color mapping. Do not
  introduce a fixed white/rainbow overlay or randomly chosen hues.
- [ ] Support 320px and 512px square output with checked pixel-count arithmetic;
  reject unsupported/excessive dimensions before allocating.
- [ ] Add renderer tests for opaque output, dimensions, bounded/finite math,
  repeatability, and differing geometry for distinct tracks.
- [ ] Test that the intensity field is unchanged when only the palette changes
  and every output pixel obeys the palette map within one channel value of
  numerical tolerance. Cover red-only, blue-only, dark, monochrome, default and
  all-black palettes; do not require non-black output for an all-black palette.
- [ ] Save tolerant golden geometry fixtures and contrasting real-skin previews
  under test artifacts; inspect 320px notification-scale readability.
- [ ] **Gate:** renderer/key tests pass and previews use skin colors without
  changing the existing player visualization output.

## 4. Implement policy and off-thread palette resolution

- [ ] Add `Generated`/`Off` policy to the existing configuration/session mechanism
  with backward-compatible defaults. Add config round-trip/default tests.
- [ ] Keep `config.skin` as the persisted selector; do **not** add a second
  persistent palette snapshot.
- [ ] Add a process-wide immutable settings snapshot: selector, resolved palette
  when known, policy, and revision. Publish a new revision when selector,
  palette bytes or policy changes.
- [ ] Implement a palette-only loader in `src/skin/mod.rs`, reusing named-text
  precedence and `parse_vis_colors` fallback without decoding all skin images.
  Bound archive/text reads: current archive loading materializes entries and is
  not already an optimized palette-only path.
- [ ] Add directory/archive/missing/partial/malformed palette tests comparing the
  new loader with full skin-loader results, plus archive/text-limit tests.
- [ ] Provide cold settings resolution callable from a background worker. Read
  saved config/skin off-thread; warm requests use memory instead of disk.
- [ ] Prevent a delayed cold resolution from overwriting newer foreground
  settings by validating the captured revision before committing it.
- [ ] On palette-load failure use the built-in resolved palette and one diagnostic;
  preserve the persisted selector and keep playback working.
- [ ] Add host tests for same-selector palette edits, equivalent-palette reuse,
  fallback and cold-resolution versus foreground-update races. Android has no
  palette editor; same-path editing is a host/debug-injection case.
- [ ] **Gate:** settings/palette tests pass; request validation needs only memory,
  never main-thread config/archive/image-file reads.

## 5. Implement the service-owned worker and JNI pixel bridge

- [ ] Add Android glue in `src/ui/egui/android/artwork.rs` and register the JNI
  entry points in `android/jni.rs`.
- [ ] Expose a fast native snapshot operation that copies media identity/settings
  under short locks and returns before any rendering/I/O.
- [ ] Audit `handle_media_control()` ordering locks before nested JNI calls.
  Defer Java snapshot retrieval until the Rust→Java callback has returned, or
  pass an immutable snapshot through it; never reacquire the same ordering lock
  recursively from Java.
- [ ] Add a blocking worker-callable native renderer returning bounded 320px ARGB
  `int[]`, reusing the existing widget pixel-conversion convention. Inputs are
  explicit immutable snapshots; do not reuse widget rendering itself.
- [ ] Add a service-owned single-thread executor with one drain task and one
  replaceable pending request. Do not enqueue every request into an unbounded
  `newSingleThreadExecutor` queue. Align Java scheduling with the tested pure
  transition contract; verify actual Java behavior later with client probes.
- [ ] Create the small `Bitmap` off-main-thread, then `Handler.post` completion.
  Validate service epoch, generation and current native request identity on the
  main thread before accepting it. Do not wait for the 500/1000ms playback poll.
- [ ] Avoid a Rust service `GlobalRef`: Java owns worker calls and completion.
  No task or Handler closure may retain an Activity.
- [ ] Cache at most two immutable current/recent bitmaps initially; render-key
  hits reuse the same Bitmap. Process death regenerates in bitmap-only mode.
- [ ] Implement stop/`onDestroy()` invalidation, pending cancellation and executor
  shutdown. Native work is bounded and cancellable between stages; Java thread
  interruption alone is not proof that native work stopped.
- [ ] Bound overlapping old/new service workers with a process-wide execution
  lease or equivalent handoff. Discard stale completions and release owner
  references safely; do not recycle bitmaps still used by framework readers.
- [ ] Add bounded retry/backoff and rate-limited errors; failures fall back to
  icon/no artwork without blocking transport.
- [ ] Add JNI/packaging assertions beside `tests/android_widget_packaging.rs` and
  a minimal debug worker probe for background completion and teardown.
- [ ] **Gate:** APK builds, native pixel transfer produces a valid 320px Bitmap,
  worker completion works without an Activity, and delayed teardown results are
  rejected. No new rendering/I/O/bitmap construction holds playback locks or
  runs on the main/audio thread.

## 6. Connect triggers and deduplicated publication

- [ ] Connect accepted track-state updates from both foreground Activity Intents
  and service-authoritative direct JNI to the service artwork coordinator.
- [ ] Carry media revision/generation through foreground projections and reject
  stale Intents before applying them. Pair title, media ID and artwork identity
  from the same accepted snapshot; querying a fresh artwork key alone cannot
  repair a delayed Intent's old title.
- [ ] Publish resolved `active_skin.vis_colors()` during foreground initialization
  and `replace_active_skin()`/`apply_preferences_config()`.
- [ ] Add an explicit compact `ACTION_ARTWORK_INVALIDATE` skin/policy path through
  application context. Publish the settings revision before invalidating the
  service. It must work when foreground media projection is not pending and
  playback fields are unchanged. Avoid starting a stopped playback service
  solely to process a palette change; retain settings for its next start.
- [ ] Add one shared Java metadata gate for both publication paths. Deduplicate
  `(title, display title, duration, media ID, stable track identity, art key/presence)`.
  Do not rely only on foreground `MEDIA_NOTIFICATION`; extend that identity if
  needed without leaving the background path unguarded.
- [ ] Supply `METADATA_KEY_ALBUM_ART` and notification `setLargeIcon()` using the
  retained 320px Bitmap. Avoid redundant bitmap metadata keys.
- [ ] Separate `setPlaybackState`/position updates from bitmap metadata changes;
  pause/resume/seek/stream-info updates must not regenerate/decode/resend unchanged
  artwork. Deduplicate notification rebuilds by displayed state/actions/icon.
- [ ] Publish artwork-only completions through the metadata gate and
  `notificationManager.notify()` on an existing foreground notification. Do not
  call `applyNativePlaybackState()`, re-request focus or restart `startForeground`.
- [ ] Preserve foreground-service startup deadlines, transport/queue/seek controls,
  wake-lock and audio-focus behavior. A stale completion must not revive a stopped
  notification/service. Keep browse/queue descriptions on the app icon.
- [ ] Replace ready cache hits atomically. For misses, publish new metadata without
  mismatched old art, then matching completed art. Bound settled misses to clear
  plus ready transitions; ready hits to one transition.
- [ ] Add Android preference controls for `Generated`/`Off`. `Off` removes art and
  large icon, cancels pending work, persists normally and survives cold starts.
- [ ] **Gate:** foreground/background track changes, palette switches and `Off`
  work; delayed Intents/completions cannot mismatch track metadata and art.

## 7. Add probes, instrument the spike and choose delivery

- [ ] Extend `XmmsAutoProbeActivity` or a debug-only client to observe
  `MediaController` metadata, bitmap dimensions/digest, request tags, palette
  assertions, clear/ready transitions and publication counters.
- [ ] Add bounded debug delay/failure injection for native completion, foreground
  Intents and cold settings resolution. Gate all externally reachable debug hooks
  and probe manifest registration out of release builds; an unregistered probe
  class remaining in dex is not itself exposure.
- [ ] Add optional, rate-limited structured logcat/probe timing records with epoch,
  generation and key. Do not log raw source paths or from audio callbacks.
- [ ] Use Java `SystemClock.elapsedRealtimeNanos()` for cross-process timestamps;
  return native stage durations, not unsynchronized Rust absolute timestamps.
  Measure queue, palette resolution, render, array transfer, bitmap creation,
  cache hit, metadata/notification publication and client inspection.
- [ ] Define `request_to_client_ready` as accepted service request → matching
  metadata received and image inspected/decoded by a client; define
  `callback_to_decode` separately. Measure actual System UI visibility separately
  rather than inferring it from `setMetadata`.
- [ ] Implement `e2e/test_android_artwork.py` fixtures using existing private-file
  and device helpers; save probe/source-image/screenshot/logcat/media-session/
  notification artifacts under `testoutput`.
- [ ] On API 35, prove fixed/default and skin-colored 320px art in expanded
  notifications and lock-screen controls; test cropping/darkening, flicker,
  actual Binder transport/downscaling and artwork-only `notify()` updates.
- [ ] Write an API 26/36 and physical-device procedure: compatible ABI build,
  explicit `adb -s <serial>` install, seeded audio/skins, play/skip/pause/skin
  switches/background/relaunch and artifact collection. Existing fixtures are
  API-35-emulator-oriented; either extend serial/API selection explicitly or use
  this manual procedure. Do not presume these runs already work in the harness.
- [ ] Run the phone compatibility spike on API 26/35/36 and record results or
  blockers. Do not claim phone compatibility from an in-app probe alone.
- [ ] **Decision gate:** record bitmap-only versus URI delivery, supported phone
  versions, tested bitmap keys and notification behavior. If bitmap-only satisfies
  v1, mark section 8 not applicable and proceed to section 9. If URI is required,
  identify the concrete unmet requirement and complete section 8 first.

## 8. Conditional: add 512px URI delivery and bounded disk cache

**Only execute if section 7 selects URI delivery.** No provider/disk cache is
needed in the bitmap-only path. Use a minimal URI prototype to settle permission
policy before completing the cache/provider implementation.

- [ ] Render/encode an opaque 512px PNG off-thread with the existing `image`
  dependency; retain only the small bitmap fallback proved necessary by the spike.
- [ ] Add `XmmsArtworkProvider.java` and a same-process manifest entry in
  `scripts/repo.py`. The Java glob already includes new classes automatically.
- [ ] Add a separate-package client APK build/install task and test System UI plus
  genuine separate-UID URI reads. Metadata strings do not grant permissions.
  Prefer narrow grants if proven supported; document artwork readability by
  other apps if an exported read-only provider is necessary.
- [ ] Use random mapped URI IDs or keyed digests with a per-install random secret;
  never expose deterministic public digests of track paths, titles or index queries.
  Preserve full internal keys and test reinstall/key rotation/stale mappings.
- [ ] Restrict read-only URI grammar/object mapping, MIME type and file opens.
  Reject unknown IDs, writes/mutations, arbitrary paths, encoded traversal and
  unsupported modes; never expose the whole private cache.
- [ ] Keep synchronous provider calls independent of Activity state and rendering.
  Fixed-size files only in v1; rate-limit legitimate known-current-key recovery.
  Untrusted unknown requests cannot force render work or unbounded allocations.
- [ ] Add atomic temporary-file rename and bounded missing/corrupt-file recovery.
  Android can remove cache files despite application pinning; fail reads cleanly
  and regenerate off-thread without blocking playback.
- [ ] Enforce normal cache limits of 32 MiB/128 entries and a hard total limit of
  40 MiB/160 entries including pinned/grace/temp objects. Pin current art; keep
  previous published URIs up to five minutes but evict grace entries early under
  pressure. Already-open descriptors remain readable.
- [ ] Publish `ALBUM_ART_URI`, `ART_URI` and `DISPLAY_ICON_URI` through metadata;
  leave bitmaps out of browser/queue lists and service Intents. Avoid duplicate
  bitmap fields and decode the small fallback off-main-thread only when needed.
- [ ] Add unit/security/cache tests and external-client E2E for traversal, writes,
  guessed IDs, corruption, eviction/grace limits, delayed readers and missing
  files. Shell `content read` without `adb root` may supplement tests but does not
  substitute for regular-client permission evidence.
- [ ] Add PNG/write/open/decode timing and buffer/cache counters to instrumentation.
- [ ] **Gate:** external clients can read current artwork, private non-art files
  remain inaccessible, total cache/worker/memory bounds hold, and lifecycle/
  transport regressions still pass. Re-run section 7 presentation checks in URI mode.

## 9. Complete functional and lifecycle coverage

- [ ] Test no-cover synthetic tracks: source pixels obey palette mapping, controller
  sees matching art, and notification/lock screen visibly display it. Use tolerant
  native-surface assertions because System UI may crop/tint/darken.
- [ ] Test distinct skin palettes and switch-back cache reuse; add same-selector
  palette revision through host/debug injection, not a nonexistent Android editor.
- [ ] Test next/previous/EOF, duplicate titles, same-URI entries, playlist reorder/
  replacement/clear, and rapid skipping with delayed worker/Intent injection.
  Final title, media ID, stable identity and image must agree; stale publications zero.
- [ ] Test pause/resume/seek/stream-info and steady playback: no new render/decode/
  art-metadata publication for unchanged keys; controls and playback progress work.
- [ ] Test Home/lock screen, Activity recreation/exit, widget play, cold browser/
  service play, idle cached-process kill and relaunch. No Activity render surface
  is required; persisted policy and selected skin restore correctly.
- [ ] Test concurrent cold palette resolution/foreground skin switch, stop during
  render and service destroy/recreate during render. Validate epoch/revision and
  process-wide execution bounds against the actual Java worker implementation.
- [ ] Test renderer/palette-read failure, retries/cancellation and `Off`; assert no
  crash, leftover large icon, retry storm, lost controls or revived stopped service.
- [ ] Complete the conditional URI cases from section 8 when applicable.
- [ ] Check release APK manifest/debug-hook gating and Java/JNI signatures; keep
  packaging source assertions but do not treat them as behavioral proof.
- [ ] Run focused/new host tests, both CI host sets and the Android suite; document
  all skipped tests, infrastructure failures and unresolved regressions.
- [ ] **Gate:** automated functional/lifecycle tests pass; manual phone compatibility
  evidence is attached and existing media-browser/widget/transport tests pass.

Validation commands (new test targets must exist before invoking them):

```bash
cargo test --locked --test android_artwork
cargo test --locked --test android_widget_packaging
cargo test --locked --all-targets
cargo test --locked --no-default-features --features egui-ui,rodio-backend --lib --bins
./repo android-apk-emulator
XMMS_E2E_ANDROID_SKIP_BUILD=1 ./repo pye2e -m android
```

## 10. Build the performance harness and measure

- [ ] Extend `scripts/performance.py` and `scripts/perf_driver.py` with artwork
  workloads, explicit device serial, metrics parsing and artifacts. Current Android
  driver reports startup only; desktop `perf::initialize()` is not initialized by
  Android and its environment settings cannot be presumed available.
- [ ] Add parser/scenario unit tests in `scripts/test_performance.py` and
  `scripts/test_perf_driver.py`; include malformed/missing samples and failures.
- [ ] Export p50/p95/max, sample counts and errors for section 7's latency spans.
  Report settings/startup separately from warm selection and client-decode latency.
- [ ] Record renders/decodes, art metadata and notify calls, clear/ready transitions,
  coalesced/cancelled/stale results, pending depth, worker wakeups, pixel/native-array/
  bitmap overlap, cache bytes and conditional PNG/disk work. Measure instrumentation
  overhead; do not count nominal pixels alone as total allocation cost.
- [ ] Prepare one optimized APK and a named physical low/mid-range reference device.
  Compare `Off`/`Generated` with fixed audio/skins; record revision, OS/ABI/density,
  SoC/RAM, renderer version, delivery mode, cache state and temperature. Control
  brightness, volume, network and background apps. Emulator timing is not a
  physical-device performance result.
- [ ] Measure at least 30 cold uncached selections and 100 warm selections within
  the retention window; label outside-window misses separately.
- [ ] Measure 100 skip requests at 100ms spacing, at least 30 skin switches including
  injected revisions, and repeated service recreation while work is running.
- [ ] Measure ten minutes of unchanged-key background playback per mode and enough
  identities to exceed memory retention and, conditionally, both disk cache limits.
- [ ] Collect CPU/scheduling/main-thread/System UI traces with Perfetto, Java/native/
  PSS using memory profiler or `dumpsys meminfo`, and Activity jank using `gfxinfo`
  or frame metrics. Account for existing widget/notification baseline costs.
- [ ] Monitor audio underrun diagnostics where available, playback progress and
  continuity under load. No new audio-thread I/O/rendering/lock work is acceptable.
- [ ] Run repeated paired 30-minute screen-off workloads; report CPU/wakeups and
  batterystats/available power-rail energy with uncertainty. Energy is report-only,
  not a noisy short-run release threshold; investigate roughly >2% incremental
  energy instead of asserting an unmeasured saving.
- [ ] Compare results with the provisional budgets below; document justified budget
  changes using actual named-device evidence, not just assumptions.
- [ ] If needed, profile and optimize trig precomputation/layer count/resolution or
  conditional PNG compression; obtain visual approval for quality reductions.
  Never move work onto main/audio threads or add GL/SIMD complexity without need.
- [ ] Re-run affected functional tests and workloads after optimizations.
- [ ] **Gate:** measured results/artifacts show bounded resources, no playback
  regression and acceptable latency; remaining uncertainty is explicit.

Performance budgets to evaluate:

| Metric | Provisional target / invariant |
| --- | --- |
| Request → client-ready | Cold p95 ≤250ms; retained warm hit p95 ≤100ms; event-driven completion, not polling. |
| Artwork main-thread spans | p95 ≤4ms; none >16ms; no new render/encode/file access/bitmap creation on main/audio thread. |
| Worker backlog | One running plus one replaceable pending; zero accepted stale publications. |
| Art transitions | Settled miss: clear + ready at most; ready hit: one at most. Count actual metadata/notify calls. |
| Steady unchanged key | Zero extra render/decode/art-publication/worker-wakeup work; existing playback polls/state updates are baseline. |
| Peak attributable image buffers | Bitmap-only ≤2 MiB; URI mode ≤4 MiB including 512px render/decode and transfer/replacement overlap. Verify codec/allocator costs. |
| Incremental PSS | Initial goal ≤8 MiB; measure separately from image buffers. |
| Conditional total disk cache | ≤40 MiB/160 entries including grace/temp; normal target 32 MiB/128 entries. |
| Playback/stability | No new Binder failures, ANRs, crashes, audio underruns or persistent CPU growth. |
| Energy | Report-only paired estimate with uncertainty; structural zero-steady-work is the primary battery safeguard. |

## 11. Final device validation and release readiness

- [ ] Repeat the recorded manual/API/device procedure against the final APK on
  API 26, API 36 and the physical reference device; retain API 35 automated evidence.
  Record supported configurations and any outstanding environment limitations.
- [ ] Verify source artwork palette correctness separately from System UI styling;
  capture final notification and lock-screen screenshots with contrasting skins.
- [ ] Verify foreground-service/audio-focus/wake-lock/queue/seek/widget/browser
  behavior has not regressed; check release APK has no reachable debug probes.
- [ ] Confirm the shipping delivery path, bitmap keys, size, setting default and
  supported-client scope. Explicitly document any deferred car/high-resolution work.
- [ ] Document that `Generated` is track-seeded, skin-colored static art; explain
  `Off`, process-death regeneration and conditional disk caching without implying
  audio analysis or aesthetic cover detection.
- [ ] Review the final diff and measured evidence; ensure every applicable earlier
  gate is complete and skipped conditional work has a recorded reason.
- [ ] **Release gate:** all applicable functional/device/performance checks pass or
  an explicit accepted limitation is recorded; no stale-art, unbounded-resource,
  privacy/security or audio/playback blocker remains.

## 12. Future backlog — not required for v1

- [ ] Add `Auto` embedded-cover extraction with explicit format/size/decode limits,
  corrupt/oversized-cover tests and its own performance gate. Covers retain their
  original colors; generated fallback uses skin colors. Do not judge “nice” covers.
- [ ] Add bounded audio-summary influence with deterministic/cache-version rules,
  service-only fallback and audio-thread safety tests; distinguish it from seeded art.
- [ ] Add supported Android Auto/AAOS high-resolution validation with an actual
  car harness and separate-client URI permission evidence; phone screenshots are
  not proof. Use section 8 if it was previously skipped and becomes necessary.
- [ ] Add lazy URI-only browse artwork without playlist-wide rendering or bitmap
  lists, with bounded work and browser Binder/latency tests.
- [ ] Add typed-asset/`EXTRA_SIZE` resizing only if needed, with clamped client sizes,
  bounded off-thread cache variants and oversized-request tests.

## Reference: verified integration points

| Area | Existing source / constraint |
| --- | --- |
| Palette / classic renderer | `src/skin/mod.rs`, `DefaultSkin::vis_colors()`; `src/render/main.rs`, `render_milkdrop_visualization()`. |
| Foreground projection | `src/ui/egui/effect_executor.rs`, `flush_android_media_projection()`; `android/media_session.rs`, `update_playback_notification()` and foreground-only `MEDIA_NOTIFICATION`. |
| Service publication | `src/ui/egui/android/media_session.rs`, `handle_media_control()`, `poll_playback()`, `update_service_from_backend()`; direct JNI bypasses foreground deduplication. |
| Service / Activity | `android/java/org/xmms/renascene/XmmsPlaybackService.java`, `applyNativePlaybackState()`; `XmmsActivity.java`, asynchronous `updatePlaybackNotification()`; `android/activity.rs` holds Activity, not service, `GlobalRef`. |
| Ownership | `src/ui/egui/android_media.rs`; preserve mirror/authoritative state and shared backend. |
| Skin selection | `src/ui/egui/app.rs`, `replace_active_skin()` / `apply_preferences_config()`; current hook is path-based widget refresh. Android has no palette editor; imports use new suffixed paths. |
| Pixel bridge | `android/jni.rs`, `color_image_to_jint_array`; `XmmsPlayerInfoWidget.java`, native array → Bitmap. Avoid whole-skin widget rendering on new artwork paths. |
| Module gates | `src/lib.rs`, `src/ui/egui/mod.rs`, `src/render.rs`; neutral code must compile without egui/Android test gates. |
| Packaging | `scripts/repo.py`, `_package_android_activity()`; Java 8 glob, generated manifest, release omission of probe registration. |
| Testing | `tests/android_widget_packaging.rs`, `e2e/android.py`, `e2e/test_android_ui.py`, `XmmsAutoProbeActivity.java`; managed API 35 x86_64 and emulator-only fixture assumptions. |
| Performance | `src/perf.rs`, `scripts/performance.py`, `scripts/perf_driver.py`; Android startup-only driver and desktop-only trace initialization. |

## Reference: Opus 5.5 review coverage

The source-grounded review used `github-copilot/claude-opus-5.5` against branch
`android-artwork-in-statusbar-player`, HEAD `200440e4`. Its retained report is
`android-artwork-opus-review.md` in subagent artifacts; it ran no builds/device
tests. All findings remain incorporated in this ordered checklist:

| Finding | Tasks addressing it |
| --- | --- |
| P1-1: undefined completion route | Section 5: service executor, JNI worker, Handler, lifecycle epoch, no poll dependency. |
| P1-2: wrong dedup path / missing skin trigger | Section 6: Java gates, explicit invalidation, media-revision race protection. |
| P1-3: palette persistence/threading conflict | Section 4: config selector, off-thread palette-only loading, revisioned memory snapshot. |
| P1-4: inaccessible host modules / CI | Sections 1–3 and 9: neutral modules, existing feature sets and explicit host gates. |
| P2-5: unnecessary provider/cache scope | Section 7 decision; section 8 conditional, bitmap-only path skips it. |
| P2-6: deterministic public URI IDs reveal tracks | Section 8: per-install keyed/random mapping and external security tests. |
| P2-7: unsupported E2E interactions/devices | Sections 4, 7–9 and 11: host/debug palette edits, explicit device procedure and separate-client build. |
| P2-8: missing Android perf tooling | Sections 7 and 10: measured probes, driver tasks, defined metrics, energy report-only. |
| P2-9: clearing, palette map, identity, packaging | Sections 2–3 and 6–8: explicit contracts, transition bounds, source glob and manifest gating. |
