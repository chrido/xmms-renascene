# Skin Editor — Implementation Tasks

Tasks for building the Skin Editor described in [`skineditordesign.md`](./skineditordesign.md).
Each item has a checkbox; check it off when the item is complete (code + tests + `cargo fmt`).

## Phase 1 — Editable pixel model (`src/skin/xpm.rs`)

- [x] Add `XpmImage::set_pixel_rgba(&mut self, x, y, rgba: [u8; 4]) -> bool` that stores
      premultiplied ARGB and is a no-op (returns `false`) when out of bounds.
- [x] Add `XpmImage::fill_rect_rgba(&mut self, rect, rgba) -> bool`, clamped to image bounds.
- [x] Add `XpmImage::pixels_argb_mut()` (or an internal helper) if needed by serializers.
- [x] Unit tests: set/fill keep the premultiplied-alpha invariant; out-of-bounds is a no-op;
      alpha 0 (transparency key) stores as fully transparent.

## Phase 2 — Skin editing + serialization (`src/skin/edit.rs`, new)

- [x] Register the module (`pub mod edit;`) in `src/skin/mod.rs`.
- [x] `DefaultSkin::get_mut(kind) -> Option<&mut XpmImage>`.
- [x] Color setters: `set_vis_color`, `set_playlist_colors`, `set_text_colors` as needed.
- [x] `encode_pixmap_png(&XpmImage) -> Vec<u8>` (alpha-preserving, via `image`).
- [x] `encode_pixmap_bmp(&XpmImage) -> Vec<u8>` mapping transparent pixels to color key
      `(48, 255, 50)`.
- [x] `viscolor.txt`, `pledit.txt`, `region.txt` writers matching the existing parsers.
- [x] `DefaultSkin::save_to_dir(dir)` → PNG per pixmap + txt files (lossless internal save).
- [x] `DefaultSkin::export_wsz(path)` → ZIP (via `zip` crate) of BMP pixmaps
      (`main.bmp`, `cbuttons.bmp`, `titlebar.bmp`, `shufrep.bmp`, `text.bmp`, `volume.bmp`,
      `balance.bmp`, `monoster.bmp`, `playpaus.bmp`, `nums_ex.bmp`, `numbers.bmp`,
      `posbar.bmp`, `pledit.bmp`, `eqmain.bmp`, `eq_ex.bmp`) + txt files.
- [x] Unit test: `export_wsz` then `load_from_path` round-trips pixels and colors
      (transparent ↔ color key).
- [x] Unit test: `save_to_dir` then `load_from_dir` round-trips losslessly.

## Phase 3 — Editor logic (`src/skineditor.rs`, new)

- [x] Register the module (`pub mod skineditor;`) in `src/lib.rs`.
- [x] `Tool` enum (`Brush`, `Rectangle`) and `SkinEditorState` (tool, color `[u8;4]`,
      `brush_size`, `zoom`, `fill_rectangle`, `working_name`, drag state).
- [x] `layout()` → `Vec<ElementSlot>` shelf/flow packing all 14 pixmaps at native size.
- [x] `canvas_size(slots)` derived from slots.
- [x] `hit_test(slots, cx, cy) -> Option<(SkinPixmapKind, u32, u32)>` mapping canvas px to
      a pixmap pixel (rejecting gaps/labels), accounting for `zoom`.
- [x] Brush application incl. `brush_size` block and Bresenham interpolation between motion
      points; clamps to the pressed pixmap.
- [x] Rectangle application: anchor on press, fill or stroke on release, clamped to the
      pressed pixmap.
- [x] Unit tests for `layout`/`canvas_size`, `hit_test`, brush (size + path) and rectangle
      (fill + stroke + clamping).

## Phase 4 — State integration (`src/ui.rs`)

- [x] Add `active_skin_mut(&mut self) -> &mut DefaultSkin` to `MainWindowUiState`.
- [x] Add a `SkinEditorState` field (and any `dialogs.skin_editor` visibility flag).
- [x] Editor command methods on `MainWindowUiState`: paint pixel(s), fill rectangle,
      set current color/tool/brush size/zoom — each returns whether a redraw is needed.
- [x] `editor_clone_from(...)` to load a chosen base skin as the working `active_skin`.
- [x] `editor_save()` (writes dir, updates `config.skin`, refreshes browser) and
      `editor_export_wsz(path)`.

## Phase 5 — Editor window UI (`src/ui.rs`)

- [x] `build_skin_editor_window(app, main_state, main_area, equalizer_area, playlist_area)`.
- [x] Canvas: `DrawingArea` in a `ScrolledWindow`; draw func blits every pixmap (scaled by
      `zoom`, nearest-neighbour), draws frames + labels, and a pixel grid at high zoom.
- [x] Right-hand tool palette `Box`: tool toggle (Brush/Rectangle), brush-size spinner,
      zoom control, fill checkbox.
- [x] Color selector (`gtk::ColorDialogButton`, fallback `gtk::ColorButton`) wired to the
      current color, plus a "transparent" toggle for the transparency key.
- [x] Clone / Save / Export buttons + a working-name entry; Export uses a file chooser
      writing `.wsz`.
- [x] Pointer handling: `GestureClick` + motion controller for brush strokes and rectangle
      drag, with a non-destructive rectangle preview overlay.
- [x] After every applied edit, `queue_draw` the editor canvas **and** main/equalizer/
      playlist areas (live update).

## Phase 6 — Wiring & entry point (`src/ui.rs`)

- [x] Add `skin_editor` field to `PanelWindows` and build it in `PanelWindows::new`.
- [x] Add a **"Skin Editor"** entry to the main menu popover that `present()`s the window.
- [x] Close/hide behavior consistent with the skin browser
      (`connect_close_request` → hide).

## Phase 7 — Validation & docs

- [x] `cargo fmt --all` and `cargo test --quiet` pass.
- [ ] Manual smoke check: open editor, paint with brush + rectangle, confirm the player
      redraws live, Save, Export `.wsz`, and re-import the `.wsz` via the skin browser.
- [x] Update `README.md` with a short Skin Editor section.

## Future follow-ups (not required now)

- [ ] More tools: line, fill bucket, eyedropper, select/move.
- [ ] Undo/redo history.
- [ ] Per-element import/replace from a file.
- [ ] Graphical `region.txt` polygon editing.
