# UI Refactoring Plan

Goal: encapsulate scattered UI state in `src/ui.rs` (mainly the ~70-field
`MainWindowUiState`) into cohesive sub-states and explicit state machines,
without changing behaviour or breaking parity with the original C XMMS
(`xmms-1.2.6`).

## Guiding constraints (read before starting)

- **Keep public method signatures stable.** `MainWindowUiState` is driven by the
  `src/e2e.rs` test builder (~200 `&mut self` wrappers) and by `tests/e2e.rs`.
  Refactor *representation* behind existing methods; do not rename/relocate the
  public surface in the same step.
- **Migrate field-poking tests first.** In-module tests set fields directly
  (e.g. `state.equalizer_shaded = true` at ui.rs ~9047/9101/9258/9644). Route
  these through accessors before changing the field's type, or they break.
- **One cohesive state object per XMMS window/panel** — not one global mega
  struct. The C original used per-window globals (`mainwin_*`, `playlistwin_*`,
  `equalizerwin_*`); mirror that grouping in Rust, but as owned sub-structs.
- **Watch borrow conflicts.** Many methods read `app_state.player`/`playlist`
  *and* UI fields together (footer time, titles, slider positions). Sub-structs
  must not force double mutable borrows of `MainWindowUiState`.
- **Validate after each task:** `cargo fmt --all && cargo test --quiet`.

## Tasks (ordered low-risk leaf refactors first)

- [x] **T1 — Playlist search as an enum.**
  Replace `playlist_search_active: bool` + `playlist_search_query: String` with
  `PlaylistSearch::{Inactive, Active { query }}`. Eliminates the "inactive but
  stale query" invalid state. Leaf change; only search helpers and the two
  render call-sites (ui.rs 1142/1445) touch it. Low risk.

- [x] **T2 — Playlist menu as one atomic enum.**
  Fold `playlist_menu`, `playlist_menu_hover`, `playlist_menu_pressed` into
  `PlaylistMenu::{Closed, Open { kind, hover, pressed }}`. Preserve current
  behaviour where opening selects the last item
  (`item_count().saturating_sub(1)`). Removes states where the three fields
  disagree. Low risk.

- [x] **T3 — De-duplicate playlist render-state construction.**
  The docked path (ui.rs ~1123–1150) and detached path (~1426–1453) build the
  same `PlaylistRowsRenderState`/frame args. Extract one
  `PlaylistUiState::rows_render_state()` (+ frame helper) and call it from both.
  Pure dedup, no behaviour change. Low risk.

- [ ] **T4 — Pointer interaction state machines.**
  Replace boolean/offset bags with explicit, mutually-exclusive enums:
  - Main: `active: Option<MainControl>` + `active_inside` + `slider_press_offset`
    → `MainPointer::{Idle, PressedButton { control, inside },
    DraggingSlider { slider, offset }}` (already close to this shape).
  - Equalizer: `equalizer_pressed_control` + `equalizer_pressed_inside` +
    `equalizer_dragging` + `equalizer_slider_press_offset`
    → `EqPointer::{Idle, PressedControl { control, inside },
    DraggingSlider { slider, offset }}` (already mutually exclusive in
    `equalizer_press`).
  - Playlist: `playlist_drag_index` + `playlist_drag_moved` +
    `playlist_scrollbar_dragging` + `playlist_scrollbar_drag_offset` +
    `playlist_docked_resizing` + `playlist_resize_drag_offset_y`
    → `PlaylistPointer::{Idle, DraggingEntry { index, moved },
    DraggingScrollbar { offset }, Resizing { offset }}`. Keep
    `playlist_last_click`/`pending_double_click` as separate double-click
    tracking (orthogonal). Medium risk; covered by e2e drag/scroll tests.

- [ ] **T5 — Consolidate panel placement; remove shaded/visible/detached duplication.**
  `shaded`/`playlist_shaded`/`equalizer_shaded` live in both `Config`
  (persisted) and `MainWindowUiState` (runtime), synced manually
  (ui.rs 4716–4718 in, 5519–5521 out). `visible`/`detached` live only in config
  but are read together with the runtime `shaded`. Introduce a per-panel
  `PanelPlacement { visible, detached, shaded, focused, dragging_title }` as the
  runtime source of truth; derive the config fields on save instead of
  mirroring. Reuse the existing good `PanelState::{Hidden,Docked,Detached}` enum
  for queries. Medium risk: touches config round-trip + several tests.

- [ ] **T6 — Encapsulate playback transitions as methods.**
  `PlaybackTransitionState` is already an enum but is assigned in ~14 scattered
  sites (ui.rs 5508/5968/5992/6128/6160/7550/7825/7833/7884/7900/7905/7976/
  7982/7994). Give it transition methods (`on_play`, `on_stop`, `on_tick`,
  `on_backend_eos`, `on_seek_requested`). Caveat: most sites also mutate
  `playback_position_ms`, `app_state.player`, and the backend — so transitions
  must either return an effect/decision or take the player as a collaborator;
  do not pretend it is fully self-contained. Medium risk.

- [ ] **T7 — Group fields into cohesive sub-structs (behind a stable facade).**
  Extract `EqualizerUiState` (active/automatic/preamp/bands/presets/preset_dir +
  EqPointer), `PlaylistUiState` (geometry/scroll + PlaylistMenu/Search/Pointer),
  `DialogVisibility` (file/dir/open-location/jump-time/skin-browser/output-picker
  flags), and `SkinBrowserState`. Keep `MainWindowUiState`'s existing public
  methods as thin delegators so `e2e.rs`/`tests` stay unchanged. Do this last,
  after the enums above shrink each group. Higher risk: borrow-checker churn
  where methods read `app_state` and a sub-state together. Big risk if done first.

- [ ] **T8 — (Stretch / optional) Reducer-style event handling.**
  Consider `state.handle(UiEvent) -> Vec<UiEffect>` (`Redraw`, `ResizeMain`,
  `SyncPanels`, `OpenFileDialog`, …) to replace the borrow-call-then-sync pattern
  in the GTK callbacks. Note this is *partly already present* via `UiAction`/
  `PanelAction`, and GTK-native effects (`begin_move`, `begin_resize`, window
  `present`) still need glue. Only pursue if T1–T7 land cleanly; high risk, mostly
  architectural payoff.

## Dropped / demoted from the original idea list

- "Split MainWindowUiState" and "equalizer model vs interaction" merged into T7.
- "Playlist interaction state machine", "search enum", "menu enum" split into the
  concrete T1/T2/T4 (the original grouping was overlapping).
- "Reduce config/runtime duplication" merged into T5.
- "Preserve XMMS mental model" is a guiding constraint above, not a task — it is a
  principle, not actionable on its own.
