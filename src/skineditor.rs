use crate::skin::layout::SkinRect;
use crate::skin::{DefaultSkin, SkinPixmapKind};

const CANVAS_MARGIN: i32 = 8;
const ELEMENT_GAP: i32 = 10;
const LABEL_HEIGHT: i32 = 12;
const MAX_ROW_WIDTH: i32 = 720;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Brush,
    SprayCan,
    Line,
    Rectangle,
    Selection,
    Lighten,
    Darken,
    Dither,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinEditorClipboard {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementSlot {
    pub kind: SkinPixmapKind,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DragState {
    kind: SkinPixmapKind,
    start: (i32, i32),
    last: (i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinEditorState {
    pub tool: Tool,
    pub color: [u8; 4],
    pub brush_size: u32,
    pub zoom: u32,
    pub fill_rectangle: bool,
    pub working_name: String,
    pub selection: Option<(SkinPixmapKind, SkinRect)>,
    drag: Option<DragState>,
    clipboard: Option<SkinEditorClipboard>,
    spray_counter: u32,
}

impl Default for SkinEditorState {
    fn default() -> Self {
        Self {
            tool: Tool::Brush,
            color: [0, 0, 0, 255],
            brush_size: 1,
            zoom: 2,
            fill_rectangle: true,
            working_name: "My Skin".to_string(),
            selection: None,
            drag: None,
            clipboard: None,
            spray_counter: 0,
        }
    }
}

impl SkinEditorState {
    pub fn layout(&self) -> Vec<ElementSlot> {
        let mut slots = Vec::with_capacity(SkinPixmapKind::ALL.len());
        let mut x = CANVAS_MARGIN;
        let mut y = CANVAS_MARGIN + LABEL_HEIGHT;
        let mut row_height = 0;

        for kind in SkinPixmapKind::ALL {
            let info = kind.info();
            let width = info.width as i32;
            let height = info.height as i32;
            if x > CANVAS_MARGIN && x + width > MAX_ROW_WIDTH {
                x = CANVAS_MARGIN;
                y += row_height + ELEMENT_GAP + LABEL_HEIGHT;
                row_height = 0;
            }

            slots.push(ElementSlot {
                kind,
                x,
                y,
                width,
                height,
            });
            x += width + ELEMENT_GAP;
            row_height = row_height.max(height);
        }

        slots
    }

    pub fn canvas_size(&self, slots: &[ElementSlot]) -> (i32, i32) {
        let (width, height) = slots.iter().fold((0, 0), |(width, height), slot| {
            (
                width.max(slot.x + slot.width + CANVAS_MARGIN),
                height.max(slot.y + slot.height + CANVAS_MARGIN),
            )
        });
        (
            width.max(1) * self.zoom.max(1) as i32,
            height.max(1) * self.zoom.max(1) as i32,
        )
    }

    pub fn hit_test(
        &self,
        slots: &[ElementSlot],
        canvas_x: f64,
        canvas_y: f64,
    ) -> Option<(SkinPixmapKind, u32, u32)> {
        let zoom = f64::from(self.zoom.max(1));
        let x = (canvas_x / zoom).floor() as i32;
        let y = (canvas_y / zoom).floor() as i32;
        let slot = slots.iter().find(|slot| slot.contains(x, y))?;
        Some((slot.kind, (x - slot.x) as u32, (y - slot.y) as u32))
    }

    pub fn press(&mut self, skin: &mut DefaultSkin, slots: &[ElementSlot], x: f64, y: f64) -> bool {
        let Some((kind, px, py)) = self.hit_test(slots, x, y) else {
            self.drag = None;
            return false;
        };
        let point = (px as i32, py as i32);
        self.drag = Some(DragState {
            kind,
            start: point,
            last: point,
        });
        match self.tool {
            Tool::Brush | Tool::SprayCan | Tool::Lighten | Tool::Darken | Tool::Dither => {
                self.apply_brush_point(skin, kind, point)
            }
            Tool::Selection => {
                self.selection = Some((kind, SkinRect::new(point.0, point.1, 1, 1)));
                false
            }
            Tool::Line | Tool::Rectangle => false,
        }
    }

    pub fn drag(&mut self, skin: &mut DefaultSkin, slots: &[ElementSlot], x: f64, y: f64) -> bool {
        let Some(drag) = self.drag.clone() else {
            return false;
        };
        let point = self.point_for_slot(slots, drag.kind, x, y);
        match self.tool {
            Tool::Brush | Tool::SprayCan | Tool::Lighten | Tool::Darken | Tool::Dither => {
                let changed = self.apply_brush_line(skin, drag.kind, drag.last, point);
                if let Some(drag) = self.drag.as_mut() {
                    drag.last = point;
                }
                changed
            }
            Tool::Line | Tool::Rectangle | Tool::Selection => {
                if let Some(drag) = self.drag.as_mut() {
                    drag.last = point;
                }
                if matches!(self.tool, Tool::Selection) {
                    self.selection = Some((drag.kind, rect_from_points(drag.start, point)));
                }
                false
            }
        }
    }

    pub fn release(
        &mut self,
        skin: &mut DefaultSkin,
        slots: &[ElementSlot],
        x: f64,
        y: f64,
    ) -> bool {
        let Some(mut drag) = self.drag.take() else {
            return false;
        };
        drag.last = self.point_for_slot(slots, drag.kind, x, y);
        match self.tool {
            Tool::Brush | Tool::SprayCan | Tool::Lighten | Tool::Darken | Tool::Dither => {
                self.apply_brush_line(skin, drag.kind, drag.last, drag.last)
            }
            Tool::Line => self.apply_brush_line(skin, drag.kind, drag.start, drag.last),
            Tool::Rectangle => {
                let rect = rect_from_points(drag.start, drag.last);
                if self.fill_rectangle {
                    skin.get_mut(drag.kind)
                        .is_some_and(|image| image.fill_rect_rgba(rect, self.color))
                } else {
                    self.stroke_rect(skin, drag.kind, rect)
                }
            }
            Tool::Selection => {
                self.selection = Some((drag.kind, rect_from_points(drag.start, drag.last)));
                false
            }
        }
    }

    pub fn rectangle_preview(&self) -> Option<(SkinPixmapKind, SkinRect)> {
        let drag = self.drag.as_ref()?;
        matches!(self.tool, Tool::Rectangle)
            .then_some((drag.kind, rect_from_points(drag.start, drag.last)))
    }

    pub fn line_preview(&self) -> Option<(SkinPixmapKind, (i32, i32), (i32, i32))> {
        let drag = self.drag.as_ref()?;
        matches!(self.tool, Tool::Line).then_some((drag.kind, drag.start, drag.last))
    }

    pub fn selection_preview(&self) -> Option<(SkinPixmapKind, SkinRect)> {
        if let Some(drag) = self.drag.as_ref().filter(|_| self.tool == Tool::Selection) {
            return Some((drag.kind, rect_from_points(drag.start, drag.last)));
        }
        self.selection
    }

    pub fn has_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }

    pub fn copy_selection(&mut self, skin: &DefaultSkin) -> bool {
        let Some((kind, rect)) = self.selection else {
            return false;
        };
        self.copy_rect(skin, kind, rect)
    }

    pub fn cut_selection(&mut self, skin: &mut DefaultSkin) -> bool {
        let Some((kind, rect)) = self.selection else {
            return false;
        };
        if !self.copy_rect(skin, kind, rect) {
            return false;
        }
        skin.get_mut(kind)
            .is_some_and(|image| image.fill_rect_rgba(rect, [0, 0, 0, 0]))
    }

    pub fn paste_clipboard(&self, skin: &mut DefaultSkin) -> bool {
        let Some(clipboard) = self.clipboard.as_ref() else {
            return false;
        };
        let Some((kind, rect)) = self.selection else {
            return false;
        };
        let Some(image) = skin.get_mut(kind) else {
            return false;
        };

        let mut changed = false;
        for y in 0..clipboard.height {
            for x in 0..clipboard.width {
                let Some(rgba) = unpremultiply_argb(clipboard.pixels[y * clipboard.width + x])
                else {
                    continue;
                };
                changed |= image.set_pixel_rgba(
                    rect.x.max(0) as usize + x,
                    rect.y.max(0) as usize + y,
                    rgba,
                );
            }
        }
        changed
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn set_brush_size(&mut self, size: u32) {
        self.brush_size = size.clamp(1, 15);
    }

    pub fn set_zoom(&mut self, zoom: u32) {
        self.zoom = zoom.clamp(1, 10);
    }

    fn point_for_slot(
        &self,
        slots: &[ElementSlot],
        kind: SkinPixmapKind,
        canvas_x: f64,
        canvas_y: f64,
    ) -> (i32, i32) {
        let Some(slot) = slots.iter().find(|slot| slot.kind == kind) else {
            return (0, 0);
        };
        let zoom = f64::from(self.zoom.max(1));
        let x = ((canvas_x / zoom).floor() as i32 - slot.x).clamp(0, slot.width - 1);
        let y = ((canvas_y / zoom).floor() as i32 - slot.y).clamp(0, slot.height - 1);
        (x, y)
    }

    fn apply_brush_line(
        &mut self,
        skin: &mut DefaultSkin,
        kind: SkinPixmapKind,
        from: (i32, i32),
        to: (i32, i32),
    ) -> bool {
        let mut changed = false;
        for point in bresenham_points(from, to) {
            changed |= self.apply_brush_point(skin, kind, point);
        }
        changed
    }

    fn apply_brush_point(
        &mut self,
        skin: &mut DefaultSkin,
        kind: SkinPixmapKind,
        point: (i32, i32),
    ) -> bool {
        match self.tool {
            Tool::Brush | Tool::Line => self.paint_square_brush(skin, kind, point),
            Tool::SprayCan => self.paint_spray_brush(skin, kind, point),
            Tool::Lighten => self.adjust_square_brush(skin, kind, point, 24),
            Tool::Darken => self.adjust_square_brush(skin, kind, point, -24),
            Tool::Dither => self.paint_dither_brush(skin, kind, point),
            Tool::Rectangle | Tool::Selection => false,
        }
    }

    fn paint_square_brush(
        &self,
        skin: &mut DefaultSkin,
        kind: SkinPixmapKind,
        point: (i32, i32),
    ) -> bool {
        let rect = self.brush_rect(point);
        skin.get_mut(kind)
            .is_some_and(|image| image.fill_rect_rgba(rect, self.color))
    }

    fn paint_dither_brush(
        &self,
        skin: &mut DefaultSkin,
        kind: SkinPixmapKind,
        point: (i32, i32),
    ) -> bool {
        let Some(image) = skin.get_mut(kind) else {
            return false;
        };
        let rect = self.brush_rect(point);
        let mut changed = false;
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                if (x + y) & 1 == 0 && x >= 0 && y >= 0 {
                    changed |= image.set_pixel_rgba(x as usize, y as usize, self.color);
                }
            }
        }
        changed
    }

    fn paint_spray_brush(
        &mut self,
        skin: &mut DefaultSkin,
        kind: SkinPixmapKind,
        point: (i32, i32),
    ) -> bool {
        let Some(image) = skin.get_mut(kind) else {
            return false;
        };
        self.spray_counter = self.spray_counter.wrapping_add(1);
        let radius = self.brush_size.max(1) as i32;
        let drops = (radius * radius).max(8);
        let mut seed = point_hash(point, self.spray_counter);
        let mut changed = false;
        for _ in 0..drops {
            seed = lcg(seed);
            let dx = (seed % (radius as u32 * 2 + 1)) as i32 - radius;
            seed = lcg(seed);
            let dy = (seed % (radius as u32 * 2 + 1)) as i32 - radius;
            if dx * dx + dy * dy <= radius * radius {
                let x = point.0 + dx;
                let y = point.1 + dy;
                if x >= 0 && y >= 0 {
                    changed |= image.set_pixel_rgba(x as usize, y as usize, self.color);
                }
            }
        }
        changed
    }

    fn adjust_square_brush(
        &self,
        skin: &mut DefaultSkin,
        kind: SkinPixmapKind,
        point: (i32, i32),
        delta: i16,
    ) -> bool {
        let Some(image) = skin.get_mut(kind) else {
            return false;
        };
        let rect = self.brush_rect(point);
        let mut changed = false;
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                if x < 0 || y < 0 {
                    continue;
                }
                let Some(rgba) = image
                    .pixel_argb(x as usize, y as usize)
                    .and_then(unpremultiply_argb)
                else {
                    continue;
                };
                if rgba[3] == 0 {
                    continue;
                }
                let adjusted = [
                    adjust_channel(rgba[0], delta),
                    adjust_channel(rgba[1], delta),
                    adjust_channel(rgba[2], delta),
                    rgba[3],
                ];
                changed |= image.set_pixel_rgba(x as usize, y as usize, adjusted);
            }
        }
        changed
    }

    fn brush_rect(&self, point: (i32, i32)) -> SkinRect {
        let size = self.brush_size.max(1) as i32;
        let offset = size / 2;
        SkinRect::new(point.0 - offset, point.1 - offset, size, size)
    }

    fn copy_rect(&mut self, skin: &DefaultSkin, kind: SkinPixmapKind, rect: SkinRect) -> bool {
        let Some(image) = skin.get(kind) else {
            return false;
        };
        let start_x = rect.x.max(0) as usize;
        let start_y = rect.y.max(0) as usize;
        let end_x = (rect.x + rect.width).clamp(0, image.width() as i32) as usize;
        let end_y = (rect.y + rect.height).clamp(0, image.height() as i32) as usize;
        if start_x >= end_x || start_y >= end_y {
            return false;
        }

        let width = end_x - start_x;
        let height = end_y - start_y;
        let mut pixels = Vec::with_capacity(width * height);
        for y in start_y..end_y {
            for x in start_x..end_x {
                pixels.push(image.pixel_argb(x, y).unwrap_or(0));
            }
        }
        self.clipboard = Some(SkinEditorClipboard {
            width,
            height,
            pixels,
        });
        true
    }

    fn stroke_rect(&self, skin: &mut DefaultSkin, kind: SkinPixmapKind, rect: SkinRect) -> bool {
        let Some(image) = skin.get_mut(kind) else {
            return false;
        };
        let mut changed = false;
        changed |= image.fill_rect_rgba(SkinRect::new(rect.x, rect.y, rect.width, 1), self.color);
        changed |= image.fill_rect_rgba(
            SkinRect::new(rect.x, rect.y + rect.height - 1, rect.width, 1),
            self.color,
        );
        changed |= image.fill_rect_rgba(SkinRect::new(rect.x, rect.y, 1, rect.height), self.color);
        changed |= image.fill_rect_rgba(
            SkinRect::new(rect.x + rect.width - 1, rect.y, 1, rect.height),
            self.color,
        );
        changed
    }
}

impl ElementSlot {
    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

fn rect_from_points(start: (i32, i32), end: (i32, i32)) -> SkinRect {
    let x1 = start.0.min(end.0);
    let y1 = start.1.min(end.1);
    let x2 = start.0.max(end.0);
    let y2 = start.1.max(end.1);
    SkinRect::new(x1, y1, x2 - x1 + 1, y2 - y1 + 1)
}

fn bresenham_points(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x0, mut y0) = from;
    let (x1, y1) = to;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut points = Vec::new();

    loop {
        points.push((x0, y0));
        if x0 == x1 && y0 == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }

    points
}

fn unpremultiply_argb(argb: u32) -> Option<[u8; 4]> {
    let a = ((argb >> 24) & 0xff) as u8;
    let r = ((argb >> 16) & 0xff) as u8;
    let g = ((argb >> 8) & 0xff) as u8;
    let b = (argb & 0xff) as u8;
    if a == 0 {
        return Some([0, 0, 0, 0]);
    }
    if a == 255 {
        return Some([r, g, b, a]);
    }
    Some([
        ((u32::from(r) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
        ((u32::from(g) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
        ((u32::from(b) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
        a,
    ])
}

fn adjust_channel(value: u8, delta: i16) -> u8 {
    (i16::from(value) + delta).clamp(0, 255) as u8
}

fn lcg(seed: u32) -> u32 {
    seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)
}

fn point_hash(point: (i32, i32), counter: u32) -> u32 {
    (point.0 as u32).rotate_left(5) ^ (point.1 as u32).rotate_left(17) ^ counter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_contains_every_pixmap_and_canvas_has_size() {
        let editor = SkinEditorState::default();
        let slots = editor.layout();

        assert_eq!(slots.len(), SkinPixmapKind::ALL.len());
        assert_eq!(slots[0].kind, SkinPixmapKind::Main);
        assert!(editor.canvas_size(&slots).0 > 0);
        assert!(editor.canvas_size(&slots).1 > 0);
    }

    #[test]
    fn hit_test_maps_canvas_position_to_pixmap_pixel() {
        let editor = SkinEditorState::default();
        let slots = editor.layout();
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        assert_eq!(
            editor.hit_test(
                &slots,
                f64::from((main.x + 3) * editor.zoom as i32),
                f64::from((main.y + 4) * editor.zoom as i32)
            ),
            Some((SkinPixmapKind::Main, 3, 4))
        );
        assert_eq!(editor.hit_test(&slots, 0.0, 0.0), None);
    }

    #[test]
    fn brush_size_and_zoom_are_clamped_to_ui_ranges() {
        let mut editor = SkinEditorState::default();

        editor.set_brush_size(99);
        editor.set_zoom(99);

        assert_eq!(editor.brush_size, 15);
        assert_eq!(editor.zoom, 10);

        editor.set_brush_size(0);
        editor.set_zoom(0);

        assert_eq!(editor.brush_size, 1);
        assert_eq!(editor.zoom, 1);
    }

    #[test]
    fn brush_paints_continuous_path_with_size() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            color: [1, 2, 3, 255],
            brush_size: 1,
            ..SkinEditorState::default()
        };
        let slots = editor.layout();
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();
        let sx = f64::from((main.x + 0) * editor.zoom as i32);
        let sy = f64::from((main.y + 0) * editor.zoom as i32);
        let ex = f64::from((main.x + 3) * editor.zoom as i32);
        let ey = f64::from((main.y + 0) * editor.zoom as i32);

        assert!(editor.press(&mut skin, &slots, sx, sy));
        assert!(editor.drag(&mut skin, &slots, ex, ey));

        let main = skin.get(SkinPixmapKind::Main).unwrap();
        for x in 0..=3 {
            assert_eq!(main.pixel_argb(x, 0), Some(0xff01_0203));
        }
    }

    #[test]
    fn rectangle_fill_paints_clamped_area() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            tool: Tool::Rectangle,
            color: [5, 6, 7, 255],
            ..SkinEditorState::default()
        };
        let slots = editor.layout();
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        editor.press(
            &mut skin,
            &slots,
            f64::from((main.x + 1) * editor.zoom as i32),
            f64::from((main.y + 1) * editor.zoom as i32),
        );
        assert!(editor.release(
            &mut skin,
            &slots,
            f64::from((main.x + 2) * editor.zoom as i32),
            f64::from((main.y + 2) * editor.zoom as i32),
        ));

        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.pixel_argb(1, 1), Some(0xff05_0607));
        assert_eq!(main.pixel_argb(2, 2), Some(0xff05_0607));
    }

    #[test]
    fn rectangle_stroke_only_paints_border() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            tool: Tool::Rectangle,
            color: [8, 9, 10, 255],
            fill_rectangle: false,
            ..SkinEditorState::default()
        };
        let slots = editor.layout();
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();
        let before = skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(2, 2);

        editor.press(
            &mut skin,
            &slots,
            f64::from((main.x + 1) * editor.zoom as i32),
            f64::from((main.y + 1) * editor.zoom as i32),
        );
        assert!(editor.release(
            &mut skin,
            &slots,
            f64::from((main.x + 3) * editor.zoom as i32),
            f64::from((main.y + 3) * editor.zoom as i32),
        ));

        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.pixel_argb(1, 1), Some(0xff08_090a));
        assert_eq!(main.pixel_argb(2, 2), before);
    }

    #[test]
    fn line_tool_draws_between_press_and_release() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            tool: Tool::Line,
            color: [20, 21, 22, 255],
            ..SkinEditorState::default()
        };
        let slots = editor.layout();
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        editor.press(
            &mut skin,
            &slots,
            f64::from((main.x + 0) * editor.zoom as i32),
            f64::from((main.y + 0) * editor.zoom as i32),
        );
        assert!(editor.release(
            &mut skin,
            &slots,
            f64::from((main.x + 3) * editor.zoom as i32),
            f64::from((main.y + 0) * editor.zoom as i32),
        ));

        let main = skin.get(SkinPixmapKind::Main).unwrap();
        for x in 0..=3 {
            assert_eq!(main.pixel_argb(x, 0), Some(0xff14_1516));
        }
    }

    #[test]
    fn selection_copy_and_paste_moves_pixels() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        skin.get_mut(SkinPixmapKind::Main)
            .unwrap()
            .set_pixel_rgba(1, 1, [30, 31, 32, 255]);
        let mut editor = SkinEditorState {
            tool: Tool::Selection,
            ..SkinEditorState::default()
        };
        let slots = editor.layout();
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        editor.press(
            &mut skin,
            &slots,
            f64::from((main.x + 1) * editor.zoom as i32),
            f64::from((main.y + 1) * editor.zoom as i32),
        );
        editor.release(
            &mut skin,
            &slots,
            f64::from((main.x + 1) * editor.zoom as i32),
            f64::from((main.y + 1) * editor.zoom as i32),
        );
        assert!(editor.copy_selection(&skin));
        editor.selection = Some((SkinPixmapKind::Main, SkinRect::new(3, 3, 1, 1)));
        assert!(editor.paste_clipboard(&mut skin));

        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(3, 3),
            Some(0xff1e_1f20)
        );
    }

    #[test]
    fn cut_selection_clears_source_after_copying() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        skin.get_mut(SkinPixmapKind::Main)
            .unwrap()
            .set_pixel_rgba(2, 2, [40, 41, 42, 255]);
        let mut editor = SkinEditorState {
            selection: Some((SkinPixmapKind::Main, SkinRect::new(2, 2, 1, 1))),
            ..SkinEditorState::default()
        };

        assert!(editor.cut_selection(&mut skin));

        assert!(editor.has_clipboard());
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(2, 2),
            Some(0)
        );
    }

    #[test]
    fn spraycan_paints_deterministic_scatter() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            tool: Tool::SprayCan,
            color: [50, 51, 52, 255],
            brush_size: 4,
            ..SkinEditorState::default()
        };
        let slots = editor.layout();
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        assert!(editor.press(
            &mut skin,
            &slots,
            f64::from((main.x + 20) * editor.zoom as i32),
            f64::from((main.y + 20) * editor.zoom as i32),
        ));

        let painted = skin
            .get(SkinPixmapKind::Main)
            .unwrap()
            .pixels_argb()
            .iter()
            .filter(|pixel| **pixel == 0xff32_3334)
            .count();
        assert!(painted > 0);
        assert!(painted < 81);
    }

    #[test]
    fn dither_brush_paints_checker_pattern() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            tool: Tool::Dither,
            color: [60, 61, 62, 255],
            brush_size: 3,
            ..SkinEditorState::default()
        };
        let slots = editor.layout();
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        assert!(editor.press(
            &mut skin,
            &slots,
            f64::from((main.x + 5) * editor.zoom as i32),
            f64::from((main.y + 5) * editor.zoom as i32),
        ));

        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.pixel_argb(4, 4), Some(0xff3c_3d3e));
        assert_ne!(main.pixel_argb(5, 4), Some(0xff3c_3d3e));
    }

    #[test]
    fn lighten_and_darken_adjust_existing_pixels() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        skin.get_mut(SkinPixmapKind::Main)
            .unwrap()
            .set_pixel_rgba(6, 6, [100, 100, 100, 255]);
        let slots = SkinEditorState::default().layout();
        let main_slot = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        let mut lighten = SkinEditorState {
            tool: Tool::Lighten,
            brush_size: 1,
            ..SkinEditorState::default()
        };
        assert!(lighten.press(
            &mut skin,
            &slots,
            f64::from((main_slot.x + 6) * lighten.zoom as i32),
            f64::from((main_slot.y + 6) * lighten.zoom as i32),
        ));
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(6, 6),
            Some(0xff7c_7c7c)
        );

        let mut darken = SkinEditorState {
            tool: Tool::Darken,
            brush_size: 1,
            ..SkinEditorState::default()
        };
        assert!(darken.press(
            &mut skin,
            &slots,
            f64::from((main_slot.x + 6) * darken.zoom as i32),
            f64::from((main_slot.y + 6) * darken.zoom as i32),
        ));
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(6, 6),
            Some(0xff64_6464)
        );
    }
}
