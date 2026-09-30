# Skin Editor — Design

## 1. Goal

Add an in-app **Skin Editor** that opens in its own window and lets the user:

- See **every skin element (pixmap) laid out next to each other** on a scrollable canvas.
- Pick a **color** and a **tool** (initially **brush** and **rectangle**) from a panel on the right.
- **Clone** an existing skin as the starting point, **edit** it, and **save** it.
- **Export** the result in the Winamp skin format as a **`.wsz`** file.
- See **all edits live**: every paint stroke immediately mutates the active skin and
  redraws the main player, equalizer and playlist windows.

This document sketches the design. The concrete, checkbox-tracked work items live in
[`skineditortodo.md`](./skineditortodo.md).

## 2. How the existing skin system works (context)

The pieces the editor builds on (all verified in the current source):

- **`DefaultSkin`** (`src/skin/mod.rs`) is the in-memory skin. It owns:
  - `pixmaps: BTreeMap<SkinPixmapKind, XpmImage>` — the 14 bitmaps.
  - `vis_colors: [[u8; 3]; 24]`, `playlist_colors: PlaylistColors`,
    `text_colors: TextColors`, `region_masks: RegionMasks`.
- **`SkinPixmapKind`** has 14 variants (`Main`, `CButtons`, `Titlebar`, `ShufRep`,
  `Text`, `Volume`, `Balance`, `MonoStereo`, `PlayPause`, `Numbers`, `PosBar`,
  `PlEdit`, `EqMain`, `EqEx`). Each `kind.info()` returns the `file_stem`, `width`
  and `height` (`src/skin/layout.rs`).
- **`XpmImage`** (`src/skin/xpm.rs`) is the pixel container: `width`, `height` and a
  premultiplied-alpha `argb: Vec<u32>`. It is currently **read-only** (`pixel_argb`,
  `pixels_argb`). Loaders for `.bmp`/`.png`/`.xpm` all normalize into this type; the
  magic color `(48, 255, 50)` is mapped to fully transparent.
- **Rendering** (`src/render/core.rs`): `surface_from_xpm(&XpmImage) -> ImageSurface`
  rebuilds a Cairo surface **from the `XpmImage` on every draw**. The window draw
  functions call `render_*` with `state.active_skin()`. **Consequence: mutating an
  `XpmImage` in place and calling `queue_draw()` is sufficient for a live update — no
  caching layer to invalidate.**
- **Windows** (`src/ui.rs`): each auxiliary window (equalizer, playlist, preferences,
  skin browser, …) is a `gtk::ApplicationWindow` built in `PanelWindows::new`, all
  sharing one `Rc<RefCell<MainWindowUiState>>`. The **skin browser**
  (`build_skin_browser_window`) is the closest existing model: it builds a window,
  reloads `active_skin`, and calls `main_area/equalizer_area/playlist_area.queue_draw()`.
- **Skin selection** flows through `MainWindowUiState`: `active_skin: DefaultSkin`,
  `active_skin()`, `reload_skin()`, `select_skin_browser_index()`. The configured skin
  path is `config.skin`; user skins import to `~/.config/xmms/Skins`
  (`user_skin_import_dir`).
- **`.wsz` is a ZIP archive** (`archive_entries` treats `.wsz` and `.zip` identically).
  Pixmaps inside are matched by `file_stem` with extension `bmp`/`png`/`xpm`; colors
  come from `viscolor.txt`, `pledit.txt`; the shape from `region.txt`. The `zip` crate
  is already a dependency (with `deflate`), and `image` is built with `png` + `bmp`.

## 3. Design decisions

### 3.1 What "edit" operates on

The editor **edits the live `active_skin` in place**. Because the renderer rebuilds
surfaces from `XpmImage` every frame, an in-place pixel mutation + `queue_draw()` is an
instant live update across all windows. This directly satisfies the "all edits are live
updated and the player redrawn" requirement without a second copy of the skin to keep in
sync.

**Clone semantics.** "Clone an existing skin, edit and save it" maps to:

- The editor session has a *working skin name* (defaults to e.g. `My Skin`).
- **Clone** takes a base skin (the current active skin, or one chosen from the skin
  browser list / a file) and loads it as the new working `active_skin`. From that point
  edits diverge from the original; the original on disk is untouched until **Save**.
- **Save** writes the working skin to a *new* directory under the user Skins dir, so the
  cloned-from skin is never overwritten unless the user explicitly targets it.

This keeps the model simple (one skin in memory) while honoring the clone → edit → save
workflow.

### 3.2 Module layout

| File | Responsibility | GTK? |
|------|----------------|------|
| `src/skin/xpm.rs` | Add **mutation** API to `XpmImage` (`set_pixel_rgba`, `fill_rect_rgba`, `pixels_argb_mut`). Keeps premultiplied-alpha invariant. | no |
| `src/skin/edit.rs` *(new)* | Skin-level editing + serialization: `DefaultSkin::get_mut`, color setters, `encode_pixmap_bmp/png`, `save_to_dir`, `export_wsz`, and `viscolor.txt`/`pledit.txt`/`region.txt` writers. Pure + unit-testable. | no |
| `src/skineditor.rs` *(new)* | `SkinEditorState`: tool, current color, brush size, zoom, canvas **layout** (where each pixmap sits), **hit-testing** (canvas px → `(kind, x, y)`), tool application (brush/rectangle), working name, in-progress drag. Pure + unit-testable. | no |
| `src/ui.rs` | GTK glue: `build_skin_editor_window`, canvas `DrawingArea` draw func, pointer/gesture handlers, right-hand tool palette + `gtk::ColorDialogButton` + Clone/Save/Export buttons + file choosers. Wire into `PanelWindows` and the main menu. | yes |

Rationale: `ui.rs` is already very large (~10k lines), so all *logic* that does not need
GTK lives in small, testable modules, mirroring how `skin/` and `render/` are split out.

### 3.3 Canvas layout

The canvas shows **all skin pixmaps at native size, scaled by a fractional `zoom`** (default
2×), packed into a compact, approximately square labeled atlas (a `gtk::ScrolledWindow` around
the `DrawingArea`). Each element is drawn into a slot:

```
+---------------------------------------------------------------+  +------------------+
|  [ main ]            275 x 116                                 |  | Tools            |
|  ####################################                         |  |  (•) Brush       |
|  ####################################                         |  |  ( ) Rectangle   |
|                                                               |  |                  |
|  [ titlebar ]        275 x 116                                |  | Brush size [ 1 ] |
|  ####################################                         |  |                  |
|                                                               |  | Color            |
|  [ cbuttons ] 136x36   [ shufrep ] 28x60   ...                |  |  [###  pick... ] |
|  ...                                                          |  |                  |
|  [ volume ] 68x421      [ balance ] 38x421                    |  | Skin: [My Skin ] |
|  (tall strips)                                                |  |  [ Clone... ]    |
|                                                               |  |  [ Save ]        |
|                                                               |  |  [ Export .wsz ] |
+---------------------------------------------------------------+  +------------------+
            canvas (DrawingArea in ScrolledWindow)                    tool palette (Box)
```

- Layout is computed by `SkinEditorState::layout(zoom)` → `Vec<ElementSlot { kind, origin_x, origin_y, draw_w, draw_h }>`. A simple shelf/flow packer keeps it deterministic and testable. Total canvas size derives from the slots.
- The canvas draw func: for each slot, blit the pixmap (reuse `surface_from_xpm` + nearest-neighbour, like the player) scaled by `zoom`, draw a 1px frame + label, and optionally a faint pixel grid when `zoom >= 8`.
- **Hit-testing**: `hit_test(canvas_x, canvas_y, zoom) -> Option<(SkinPixmapKind, u32, u32)>` converts a pointer position to the pixmap and pixel under it.

### 3.4 Tools

A `Tool` enum (extensible — "More we can add later"):

- **`Brush`**: on press/drag, set the pixel(s) under the cursor to the current color.
  A `brush_size` (1..=N) paints an N×N block. Dragging interpolates between successive
  motion points (Bresenham) so fast strokes stay continuous.
- **`Rectangle`**: press records the start pixel; drag previews; release fills (or
  strokes — a "Fill" checkbox, default fill) the rectangle in the current color, clamped
  to the pixmap bounds. The preview is drawn as an overlay so it does not mutate pixels
  until release.

Both tools operate within a **single pixmap** — the one hit on press; crossing into a
neighbouring element during a drag is clamped, so an edit never bleeds across pixmaps.

Tool application is pure: e.g. `apply_brush(skin, kind, x, y, color, size)` and
`apply_rectangle(skin, kind, rect, color, fill)` mutate the `XpmImage` and return whether
anything changed (to decide whether to `queue_draw`).

### 3.5 Color selection

- Current color stored as `[u8; 4]` RGBA in `SkinEditorState` (default opaque black).
- UI uses `gtk::ColorDialogButton` (GTK 4.10+) — the project targets `v4_6`; if the
  button type is unavailable at that version, fall back to `gtk::ColorButton`. The
  chosen `gdk::RGBA` is converted to `[u8; 4]`.
- A dedicated **"transparent" toggle** lets the user paint the skin's transparency key
  (alpha 0), which is what the renderer treats as see-through.

### 3.6 Live update flow

```
pointer press / motion / release
        │
        ▼
SkinEditorState records drag + computes affected pixmap+pixels
        │
        ▼
MainWindowUiState::active_skin_mut()  ──►  XpmImage mutated in place
        │
        ▼
queue_draw on: editor canvas, main_area, equalizer_area, playlist_area
        │
        ▼
each draw func rebuilds Cairo surface from the mutated XpmImage  ──► pixels visible
```

The editor is constructed with clones of the relevant `DrawingArea` handles (exactly as
`connect_skin_browser_selection` already does) so it can request their redraw.

### 3.7 Save and export formats

Two persistence paths share one serializer in `src/skin/edit.rs`:

1. **Save (internal, lossless)** → a **directory** `~/.config/xmms/Skins/<name>/`
   containing one **PNG per pixmap** (PNG preserves alpha exactly) plus
   `viscolor.txt`, `pledit.txt`, and (if present) `region.txt`. After save,
   `config.skin` is pointed at the directory and the skin browser list refreshed, so the
   saved skin shows up alongside the others. PNG is chosen for internal save because the
   existing loader already supports `.png` and it round-trips alpha without a color key.

2. **Export `.wsz` (Winamp format)** → a **ZIP** written with the `zip` crate to a
   user-chosen path. Contents:
   - One **BMP per pixmap**, named by the Winamp/`file_stem` convention
     (`main.bmp`, `cbuttons.bmp`, `titlebar.bmp`, `shufrep.bmp`, `text.bmp`,
     `volume.bmp`, `balance.bmp`, `monoster.bmp`, `playpaus.bmp`, `nums_ex.bmp` +
     `numbers.bmp` for compatibility, `posbar.bmp`, `pledit.bmp`, `eqmain.bmp`,
     `eq_ex.bmp`). BMP has no alpha, so **transparent pixels are written as the color
     key `(48, 255, 50)`** — the same value our loader maps back to transparent on
     import, giving a clean round-trip.
   - `viscolor.txt`, `pledit.txt`, `region.txt` generated from the skin's colors/masks.

`.wsz` round-trips through the existing `load_from_archive` path, so an exported file can
be re-imported via the skin browser.

### 3.8 Entry point / wiring

- Add a `skin_editor: gtk::ApplicationWindow` field to `PanelWindows` and build it in
  `PanelWindows::new`, passing the main/equalizer/playlist `DrawingArea`s for live
  redraw.
- Add a **"Skin Editor"** entry to the main menu popover (next to "Skin Browser" in
  `build_main_menu_popover`) that `present()`s the editor window.
- The editor window follows the same visibility conventions as the skin browser
  (`connect_close_request` → hide; track a `dialogs.skin_editor` flag if a flag is
  useful for session/E2E).

## 4. Data model additions (sketch)

```rust
// src/skineditor.rs
pub enum Tool { Brush, Rectangle }

pub struct ElementSlot {
    pub kind: SkinPixmapKind,
    pub origin_x: i32, pub origin_y: i32, // canvas px (pre-zoom origin)
    pub width: i32,    pub height: i32,   // native px
}

pub struct SkinEditorState {
    pub tool: Tool,
    pub color: [u8; 4],      // RGBA, alpha 0 == transparency key
    pub brush_size: u32,     // >= 1
    pub zoom: f64,           // clamped to 1.0..=10.0
    pub fill_rectangle: bool,
    pub working_name: String,
    drag: Option<DragState>, // brush path or rectangle anchor
}

impl SkinEditorState {
    pub fn layout(&self) -> Vec<ElementSlot>;
    pub fn hit_test(&self, slots: &[ElementSlot], cx: f64, cy: f64)
        -> Option<(SkinPixmapKind, u32, u32)>;
    pub fn canvas_size(&self, slots: &[ElementSlot]) -> (i32, i32);
    // begin/update/finish drag → returns the set of mutations to apply
}
```

```rust
// src/skin/xpm.rs (additions)
impl XpmImage {
    pub fn set_pixel_rgba(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> bool;
    pub fn fill_rect_rgba(&mut self, rect: SkinRect, rgba: [u8; 4]) -> bool;
}

// src/skin/edit.rs (new)
impl DefaultSkin {
    pub fn get_mut(&mut self, kind: SkinPixmapKind) -> Option<&mut XpmImage>;
    pub fn set_vis_color(&mut self, i: usize, rgb: [u8; 3]);
    pub fn set_playlist_colors(&mut self, c: PlaylistColors);
    pub fn save_to_dir(&self, dir: &Path) -> io::Result<()>;       // PNG + txts
    pub fn export_wsz(&self, path: &Path) -> io::Result<()>;        // BMP zip + txts
}
```

## 5. Testing strategy

Pure logic is unit-tested without GTK:

- `XpmImage` mutation keeps premultiplied-alpha invariant; out-of-bounds is a no-op.
- `SkinEditorState::layout`/`canvas_size` are deterministic; `hit_test` maps known
  canvas coordinates to the expected `(kind, x, y)` and rejects gaps/labels.
- Brush (incl. size and Bresenham path) and rectangle (fill + stroke, clamped) produce
  the expected pixels.
- **Round-trip**: build a skin, `export_wsz` to a temp file, reload via
  `DefaultSkin::load_from_path`, assert pixels/colors match (transparent ↔ color key).
- `save_to_dir` then `load_from_dir` round-trips losslessly (PNG keeps alpha).

GTK window construction is covered by a `--gtk-smoke`-style path if practical, but the
substantive coverage is the pure modules. Validate with `cargo fmt --all` and
`cargo test --quiet`.

## 6. Out of scope (for now)

- Tools beyond brush/rectangle (line, fill bucket, eyedropper, move/select) — the `Tool`
  enum and pure tool-application functions are structured so these slot in later.
- Editing `region.txt` polygons graphically, undo/redo history, and per-element
  import/replace. These are noted as natural follow-ups in the todo.
