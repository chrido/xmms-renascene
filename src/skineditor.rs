use crate::skin::layout::SkinRect;
use crate::skin::{DefaultSkin, SkinPixmapKind};

const CANVAS_MARGIN: i32 = 8;
const ELEMENT_GAP: i32 = 10;
const LABEL_HEIGHT: i32 = 12;
const MAX_ATLAS_WIDTH: i32 = 720;
pub const COLOR_SHELF_SIZE: usize = 32;
pub const GRADIENT_SHELF_SIZE: usize = 16;
pub const DEFAULT_GRADIENT_START: [u8; 4] = [0, 0, 0, 255];
pub const DEFAULT_GRADIENT_END: [u8; 4] = [255, 255, 255, 255];
pub const MIN_ZOOM: f64 = 1.0;
pub const MAX_ZOOM: f64 = 10.0;
pub const ZOOM_STEP: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Brush,
    SprayCan,
    Fill,
    Line,
    Rectangle,
    Selection,
    Lighten,
    Darken,
    Dither,
    ColorPicker,
    Drag,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub position: f64,
    pub color: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinGradient {
    stops: Vec<GradientStop>,
}

impl Default for SkinGradient {
    fn default() -> Self {
        Self::new(vec![
            GradientStop {
                position: 0.0,
                color: DEFAULT_GRADIENT_START,
            },
            GradientStop {
                position: 1.0,
                color: DEFAULT_GRADIENT_END,
            },
        ])
    }
}

impl SkinGradient {
    pub fn new(stops: Vec<GradientStop>) -> Self {
        let mut gradient = Self { stops };
        gradient.normalize();
        gradient
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    pub fn color_at(&self, fraction: f64) -> [u8; 4] {
        let fraction = finite_unit(fraction);
        if fraction <= self.stops[0].position {
            return self.stops[0].color;
        }
        for pair in self.stops.windows(2) {
            let left = pair[0];
            let right = pair[1];
            if fraction <= right.position {
                let span = right.position - left.position;
                let local = if span <= f64::EPSILON {
                    0.0
                } else {
                    (fraction - left.position) / span
                };
                return interpolate_rgba(left.color, right.color, local);
            }
        }
        self.stops
            .last()
            .map(|stop| stop.color)
            .unwrap_or(DEFAULT_GRADIENT_END)
    }

    fn normalize(&mut self) {
        if self.stops.len() < 2 {
            self.stops = SkinGradient::default().stops;
            return;
        }
        for stop in &mut self.stops {
            stop.position = finite_unit(stop.position);
        }
        self.stops.sort_by(|a, b| {
            a.position
                .partial_cmp(&b.position)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.stops[0].position = 0.0;
        let last = self.stops.len() - 1;
        self.stops[last].position = 1.0;
    }
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

#[derive(Debug, Clone, PartialEq)]
pub struct SkinEditorState {
    pub tool: Tool,
    pub color: [u8; 4],
    pub color_shelf: [Option<[u8; 4]>; COLOR_SHELF_SIZE],
    pub gradient: SkinGradient,
    pub selected_gradient_stop: usize,
    pub gradient_shelf: [Option<SkinGradient>; GRADIENT_SHELF_SIZE],
    pub couple_control_edits: bool,
    pub coupled_edits_use_gradient: bool,
    pub brush_size: u32,
    pub zoom: f64,
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
            color_shelf: [None; COLOR_SHELF_SIZE],
            gradient: SkinGradient::default(),
            selected_gradient_stop: 0,
            gradient_shelf: std::array::from_fn(|_| None),
            couple_control_edits: false,
            coupled_edits_use_gradient: false,
            brush_size: 1,
            zoom: 2.0,
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
    pub fn layout(&self, skin: &DefaultSkin) -> Vec<ElementSlot> {
        pack_compact_pixmaps(skin)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PackedItem {
    kind: SkinPixmapKind,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PackRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl PackRect {
    fn right(self) -> i32 {
        self.x + self.width
    }

    fn bottom(self) -> i32 {
        self.y + self.height
    }

    fn intersects(self, other: Self) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    fn contains(self, other: Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }
}

fn pack_compact_pixmaps(skin: &DefaultSkin) -> Vec<ElementSlot> {
    let items = skin_pack_items(skin);
    let min_width = items.iter().map(|item| item.width).max().unwrap_or(1);
    let content_area: i32 = items.iter().map(|item| item.width * item.height).sum();
    let total_height: i32 = items.iter().map(|item| item.height).sum();
    let mut best: Option<(i32, i32, i32, Vec<PackedItem>)> = None;

    for width in (min_width..=MAX_ATLAS_WIDTH).step_by(25) {
        if let Some(packed) = pack_items_at_width(&items, width, total_height) {
            let height = packed
                .iter()
                .map(|item| item.y + item.height)
                .max()
                .unwrap_or(0);
            let area = width * height;
            let square_error = (width - height).abs();
            let waste = area.saturating_sub(content_area);
            let replace =
                best.as_ref()
                    .is_none_or(|(best_square_error, best_waste, best_area, _)| {
                        square_error < *best_square_error
                            || (square_error == *best_square_error && waste < *best_waste)
                            || (square_error == *best_square_error
                                && waste == *best_waste
                                && area < *best_area)
                    });
            if replace {
                best = Some((square_error, waste, area, packed));
            }
        }
    }

    let mut packed = best
        .map(|(_, _, _, packed)| packed)
        .unwrap_or_else(|| fallback_vertical_pack(&items));
    packed.sort_by_key(|item| {
        SkinPixmapKind::ALL
            .iter()
            .position(|kind| *kind == item.kind)
            .unwrap_or(usize::MAX)
    });

    packed
        .into_iter()
        .map(|item| ElementSlot {
            kind: item.kind,
            x: item.x + CANVAS_MARGIN,
            y: item.y + CANVAS_MARGIN + LABEL_HEIGHT,
            width: item.width,
            height: item.height - LABEL_HEIGHT,
        })
        .collect()
}

fn skin_pack_items(skin: &DefaultSkin) -> Vec<PackedItem> {
    let mut items: Vec<_> = SkinPixmapKind::ALL
        .iter()
        .map(|kind| {
            let info = kind.info();
            let (width, height) = skin
                .get(*kind)
                .map(|image| (image.width() as i32, image.height() as i32))
                .unwrap_or((info.width as i32, info.height as i32));
            PackedItem {
                kind: *kind,
                x: 0,
                y: 0,
                width,
                height: height + LABEL_HEIGHT,
            }
        })
        .collect();
    items.sort_by_key(|item| (-(item.height as isize), -(item.width as isize)));
    items
}

fn pack_items_at_width(
    items: &[PackedItem],
    width: i32,
    max_height: i32,
) -> Option<Vec<PackedItem>> {
    let mut free = vec![PackRect {
        x: 0,
        y: 0,
        width,
        height: max_height,
    }];
    let mut packed = Vec::with_capacity(items.len());

    for item in items {
        let placement = free
            .iter()
            .filter(|rect| item.width <= rect.width && item.height <= rect.height)
            .map(|rect| {
                let placed = PackRect {
                    x: rect.x,
                    y: rect.y,
                    width: item.width,
                    height: item.height,
                };
                let score = (
                    placed.bottom(),
                    rect.width * rect.height - item.width * item.height,
                    placed.x,
                );
                (placed, score)
            })
            .min_by_key(|(_, score)| *score)
            .map(|(placed, _)| placed)?;

        split_free_rects(&mut free, placement);
        prune_free_rects(&mut free);
        packed.push(PackedItem {
            x: placement.x,
            y: placement.y,
            ..*item
        });
    }

    Some(packed)
}

fn split_free_rects(free: &mut Vec<PackRect>, used: PackRect) {
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < free.len() {
        let rect = free[index];
        if !rect.intersects(used) {
            index += 1;
            continue;
        }

        free.remove(index);
        if used.x > rect.x {
            replacements.push(PackRect {
                x: rect.x,
                y: rect.y,
                width: used.x - rect.x - ELEMENT_GAP,
                height: rect.height,
            });
        }
        if used.right() + ELEMENT_GAP < rect.right() {
            replacements.push(PackRect {
                x: used.right() + ELEMENT_GAP,
                y: rect.y,
                width: rect.right() - used.right() - ELEMENT_GAP,
                height: rect.height,
            });
        }
        if used.y > rect.y {
            replacements.push(PackRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: used.y - rect.y - ELEMENT_GAP,
            });
        }
        if used.bottom() + ELEMENT_GAP < rect.bottom() {
            replacements.push(PackRect {
                x: rect.x,
                y: used.bottom() + ELEMENT_GAP,
                width: rect.width,
                height: rect.bottom() - used.bottom() - ELEMENT_GAP,
            });
        }
    }
    free.extend(
        replacements
            .into_iter()
            .filter(|rect| rect.width > 0 && rect.height > 0),
    );
}

fn prune_free_rects(free: &mut Vec<PackRect>) {
    let mut index = 0;
    while index < free.len() {
        let rect = free[index];
        if free
            .iter()
            .enumerate()
            .any(|(other_index, other)| other_index != index && other.contains(rect))
        {
            free.remove(index);
        } else {
            index += 1;
        }
    }
}

fn fallback_vertical_pack(items: &[PackedItem]) -> Vec<PackedItem> {
    let mut y = 0;
    items
        .iter()
        .map(|item| {
            let packed = PackedItem { x: 0, y, ..*item };
            y += item.height + ELEMENT_GAP;
            packed
        })
        .collect()
}

impl SkinEditorState {
    pub fn canvas_size(&self, slots: &[ElementSlot]) -> (i32, i32) {
        let (width, height) = slots.iter().fold((0, 0), |(width, height), slot| {
            (
                width.max(slot.x + slot.width + CANVAS_MARGIN),
                height.max(slot.y + slot.height + CANVAS_MARGIN),
            )
        });
        (
            (f64::from(width.max(1)) * self.zoom.max(MIN_ZOOM)).ceil() as i32,
            (f64::from(height.max(1)) * self.zoom.max(MIN_ZOOM)).ceil() as i32,
        )
    }

    pub fn hit_test(
        &self,
        slots: &[ElementSlot],
        canvas_x: f64,
        canvas_y: f64,
    ) -> Option<(SkinPixmapKind, u32, u32)> {
        let zoom = self.zoom.max(MIN_ZOOM);
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
            Tool::Fill => self.flood_fill(skin, kind, point),
            Tool::ColorPicker => {
                self.pick_skin_color(skin, kind, point);
                false
            }
            Tool::Drag => false,
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
            Tool::ColorPicker => {
                if let Some(drag) = self.drag.as_mut() {
                    drag.last = point;
                }
                self.pick_skin_color(skin, drag.kind, point);
                false
            }
            Tool::Line | Tool::Rectangle | Tool::Selection | Tool::Drag | Tool::Fill => {
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
            Tool::ColorPicker => {
                self.pick_skin_color(skin, drag.kind, drag.last);
                false
            }
            Tool::Drag | Tool::Fill => false,
            Tool::Line => self.apply_brush_line(skin, drag.kind, drag.start, drag.last),
            Tool::Rectangle => {
                let rect = rect_from_points(drag.start, drag.last);
                if self.fill_rectangle {
                    self.fill_rect(skin, drag.kind, rect)
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

    pub fn set_zoom(&mut self, zoom: f64) {
        let zoom = if zoom.is_finite() { zoom } else { MIN_ZOOM };
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    pub fn store_color_shelf_slot(&mut self, index: usize) -> Option<[u8; 4]> {
        let slot = self.color_shelf.get_mut(index)?;
        *slot = Some(self.color);
        *slot
    }

    pub fn pick_color_shelf_slot(&mut self, index: usize) -> Option<[u8; 4]> {
        let color = self.color_shelf.get(index).copied().flatten()?;
        self.color = color;
        Some(color)
    }

    pub fn gradient_stops(&self) -> &[GradientStop] {
        self.gradient.stops()
    }

    pub fn selected_gradient_stop(&self) -> usize {
        self.selected_gradient_stop
            .min(self.gradient.stops.len() - 1)
    }

    pub fn select_gradient_stop(&mut self, index: usize) -> Option<[u8; 4]> {
        let index = index.min(self.gradient.stops.len().checked_sub(1)?);
        self.selected_gradient_stop = index;
        self.color = self.gradient.stops[index].color;
        Some(self.color)
    }

    pub fn store_selected_gradient_stop(&mut self) -> Option<[u8; 4]> {
        let index = self.selected_gradient_stop();
        self.gradient.stops.get_mut(index)?.color = self.color;
        Some(self.color)
    }

    pub fn set_gradient_stop_position(&mut self, index: usize, position: f64) -> bool {
        if index == 0 || index + 1 >= self.gradient.stops.len() {
            return false;
        }
        let Some(stop) = self.gradient.stops.get_mut(index) else {
            return false;
        };
        stop.position = finite_unit(position);
        self.gradient.normalize();
        self.selected_gradient_stop = index.min(self.gradient.stops.len() - 1);
        true
    }

    pub fn add_gradient_stop(&mut self, position: f64) -> usize {
        let position = finite_unit(position);
        let color = self.color;
        self.gradient.stops.push(GradientStop { position, color });
        self.gradient.normalize();
        let index = self
            .gradient
            .stops
            .iter()
            .position(|stop| stop.position == position && stop.color == color)
            .unwrap_or(0);
        self.selected_gradient_stop = index;
        self.color = color;
        index
    }

    pub fn remove_gradient_stop(&mut self, index: usize) -> bool {
        if self.gradient.stops.len() <= 2 || index >= self.gradient.stops.len() {
            return false;
        }
        self.gradient.stops.remove(index);
        self.gradient.normalize();
        self.selected_gradient_stop = self
            .selected_gradient_stop
            .min(self.gradient.stops.len() - 1);
        true
    }

    pub fn reverse_gradient(&mut self) {
        for stop in &mut self.gradient.stops {
            stop.position = 1.0 - stop.position;
        }
        self.gradient.normalize();
    }

    pub fn store_gradient_shelf_slot(&mut self, index: usize) -> Option<SkinGradient> {
        let slot = self.gradient_shelf.get_mut(index)?;
        *slot = Some(self.gradient.clone());
        slot.clone()
    }

    pub fn pick_gradient_shelf_slot(&mut self, index: usize) -> Option<SkinGradient> {
        let gradient = self.gradient_shelf.get(index).cloned().flatten()?;
        self.gradient = gradient.clone();
        self.selected_gradient_stop = 0;
        self.color = self.gradient.stops[0].color;
        Some(gradient)
    }

    pub fn pick_gradient_start(&mut self) -> [u8; 4] {
        self.select_gradient_stop(0).unwrap_or(self.color)
    }

    pub fn pick_gradient_end(&mut self) -> [u8; 4] {
        let last = self.gradient.stops.len() - 1;
        self.select_gradient_stop(last).unwrap_or(self.color)
    }

    pub fn store_gradient_start(&mut self) -> [u8; 4] {
        self.gradient.stops[0].color = self.color;
        self.color
    }

    pub fn store_gradient_end(&mut self) -> [u8; 4] {
        let last = self.gradient.stops.len() - 1;
        self.gradient.stops[last].color = self.color;
        self.color
    }

    pub fn gradient_color_at(&self, fraction: f64) -> [u8; 4] {
        self.gradient.color_at(fraction)
    }

    pub fn pick_gradient_color_at(&mut self, fraction: f64) -> [u8; 4] {
        let color = self.gradient_color_at(fraction);
        self.color = color;
        color
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
        let zoom = self.zoom.max(MIN_ZOOM);
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
            Tool::ColorPicker | Tool::Drag | Tool::Rectangle | Tool::Selection | Tool::Fill => {
                false
            }
        }
    }

    fn pick_skin_color(
        &mut self,
        skin: &DefaultSkin,
        kind: SkinPixmapKind,
        point: (i32, i32),
    ) -> Option<[u8; 4]> {
        if point.0 < 0 || point.1 < 0 {
            return None;
        }
        let color = skin
            .get(kind)?
            .pixel_argb(point.0 as usize, point.1 as usize)
            .and_then(unpremultiply_argb)?;
        self.color = color;
        Some(color)
    }

    fn flood_fill(&self, skin: &mut DefaultSkin, kind: SkinPixmapKind, point: (i32, i32)) -> bool {
        let mut changed = false;
        for target in self.coupled_point_targets(kind, point) {
            let Some(image) = skin.get_mut(target.kind) else {
                continue;
            };
            if target.point.0 < 0 || target.point.1 < 0 {
                continue;
            }
            let start_x = target.point.0 as usize;
            let start_y = target.point.1 as usize;
            let Some(source_argb) = image.pixel_argb(start_x, start_y) else {
                continue;
            };
            let target_argb = premultiply_rgba(target.color);
            if source_argb == target_argb {
                continue;
            }

            let width = image.width();
            let height = image.height();
            let mut stack = vec![(start_x, start_y)];
            while let Some((x, y)) = stack.pop() {
                if image.pixel_argb(x, y) != Some(source_argb) {
                    continue;
                }
                changed |= image.set_pixel_rgba(x, y, target.color);
                if x > 0 {
                    stack.push((x - 1, y));
                }
                if x + 1 < width {
                    stack.push((x + 1, y));
                }
                if y > 0 {
                    stack.push((x, y - 1));
                }
                if y + 1 < height {
                    stack.push((x, y + 1));
                }
            }
        }
        changed
    }

    fn paint_square_brush(
        &self,
        skin: &mut DefaultSkin,
        kind: SkinPixmapKind,
        point: (i32, i32),
    ) -> bool {
        let mut changed = false;
        for target in self.coupled_point_targets(kind, point) {
            let rect = self.brush_rect(target.point);
            changed |= skin
                .get_mut(target.kind)
                .is_some_and(|image| image.fill_rect_rgba(rect, target.color));
        }
        changed
    }

    fn paint_dither_brush(
        &self,
        skin: &mut DefaultSkin,
        kind: SkinPixmapKind,
        point: (i32, i32),
    ) -> bool {
        let mut changed = false;
        for target in self.coupled_point_targets(kind, point) {
            let Some(image) = skin.get_mut(target.kind) else {
                continue;
            };
            let rect = self.brush_rect(target.point);
            for y in rect.y..rect.y + rect.height {
                for x in rect.x..rect.x + rect.width {
                    if (x + y) & 1 == 0 && x >= 0 && y >= 0 {
                        changed |= image.set_pixel_rgba(x as usize, y as usize, target.color);
                    }
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
        self.spray_counter = self.spray_counter.wrapping_add(1);
        let radius = self.brush_size.max(1) as i32;
        let drops = (radius * radius).max(8);
        let mut changed = false;
        for target in self.coupled_point_targets(kind, point) {
            let Some(image) = skin.get_mut(target.kind) else {
                continue;
            };
            let mut seed = point_hash(target.point, self.spray_counter);
            for _ in 0..drops {
                seed = lcg(seed);
                let dx = (seed % (radius as u32 * 2 + 1)) as i32 - radius;
                seed = lcg(seed);
                let dy = (seed % (radius as u32 * 2 + 1)) as i32 - radius;
                if dx * dx + dy * dy <= radius * radius {
                    let x = target.point.0 + dx;
                    let y = target.point.1 + dy;
                    if x >= 0 && y >= 0 {
                        changed |= image.set_pixel_rgba(x as usize, y as usize, target.color);
                    }
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
        let mut changed = false;
        for target in self.coupled_point_targets(kind, point) {
            let Some(image) = skin.get_mut(target.kind) else {
                continue;
            };
            let rect = self.brush_rect(target.point);
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

    fn fill_rect(&self, skin: &mut DefaultSkin, kind: SkinPixmapKind, rect: SkinRect) -> bool {
        let mut changed = false;
        for target in self.coupled_rect_targets(kind, rect) {
            changed |= skin
                .get_mut(target.kind)
                .is_some_and(|image| image.fill_rect_rgba(target.rect, target.color));
        }
        changed
    }

    fn stroke_rect(&self, skin: &mut DefaultSkin, kind: SkinPixmapKind, rect: SkinRect) -> bool {
        let mut changed = false;
        for target in self.coupled_rect_targets(kind, rect) {
            let Some(image) = skin.get_mut(target.kind) else {
                continue;
            };
            changed |= image.fill_rect_rgba(
                SkinRect::new(target.rect.x, target.rect.y, target.rect.width, 1),
                target.color,
            );
            changed |= image.fill_rect_rgba(
                SkinRect::new(
                    target.rect.x,
                    target.rect.y + target.rect.height - 1,
                    target.rect.width,
                    1,
                ),
                target.color,
            );
            changed |= image.fill_rect_rgba(
                SkinRect::new(target.rect.x, target.rect.y, 1, target.rect.height),
                target.color,
            );
            changed |= image.fill_rect_rgba(
                SkinRect::new(
                    target.rect.x + target.rect.width - 1,
                    target.rect.y,
                    1,
                    target.rect.height,
                ),
                target.color,
            );
        }
        changed
    }

    fn coupled_point_targets(
        &self,
        kind: SkinPixmapKind,
        point: (i32, i32),
    ) -> Vec<CoupledPointTarget> {
        let Some((frames, source_index, local)) = coupled_frame_match(kind, point) else {
            return vec![CoupledPointTarget {
                kind,
                point,
                color: self.color,
            }];
        };
        if !self.couple_control_edits {
            return vec![CoupledPointTarget {
                kind,
                point,
                color: self.color,
            }];
        }
        let frame_count = frames.len();
        frames
            .into_iter()
            .enumerate()
            .skip(source_index)
            .map(|(index, frame)| CoupledPointTarget {
                kind,
                point: (frame.x + local.0, frame.y + local.1),
                color: self.coupled_color(index, frame_count),
            })
            .collect()
    }

    fn coupled_rect_targets(&self, kind: SkinPixmapKind, rect: SkinRect) -> Vec<CoupledRectTarget> {
        let point = (rect.x, rect.y);
        let Some((frames, source_index, local)) = coupled_frame_match(kind, point) else {
            return vec![CoupledRectTarget {
                kind,
                rect,
                color: self.color,
            }];
        };
        if !self.couple_control_edits {
            return vec![CoupledRectTarget {
                kind,
                rect,
                color: self.color,
            }];
        }
        let frame_count = frames.len();
        frames
            .into_iter()
            .enumerate()
            .skip(source_index)
            .map(|(index, frame)| CoupledRectTarget {
                kind,
                rect: SkinRect::new(
                    frame.x + local.0,
                    frame.y + local.1,
                    rect.width,
                    rect.height,
                ),
                color: self.coupled_color(index, frame_count),
            })
            .collect()
    }

    fn coupled_color(&self, index: usize, frame_count: usize) -> [u8; 4] {
        if self.coupled_edits_use_gradient {
            let fraction = if frame_count <= 1 {
                0.0
            } else {
                index as f64 / (frame_count - 1) as f64
            };
            self.gradient_color_at(fraction)
        } else {
            self.color
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoupledPointTarget {
    kind: SkinPixmapKind,
    point: (i32, i32),
    color: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoupledRectTarget {
    kind: SkinPixmapKind,
    rect: SkinRect,
    color: [u8; 4],
}

impl ElementSlot {
    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

fn coupled_frame_match(
    kind: SkinPixmapKind,
    point: (i32, i32),
) -> Option<(Vec<SkinRect>, usize, (i32, i32))> {
    let frames = coupled_control_frames(kind);
    let (index, frame) = frames
        .iter()
        .copied()
        .enumerate()
        .find(|(_, frame)| frame.contains(point.0, point.1))?;
    Some((frames, index, (point.0 - frame.x, point.1 - frame.y)))
}

fn coupled_control_frames(kind: SkinPixmapKind) -> Vec<SkinRect> {
    match kind {
        SkinPixmapKind::Volume => horizontal_slider_frames(0, 68),
        SkinPixmapKind::Balance => horizontal_slider_frames(9, 38),
        SkinPixmapKind::EqMain => equalizer_slider_frames(),
        SkinPixmapKind::EqEx => shaded_equalizer_frames(),
        _ => Vec::new(),
    }
}

fn horizontal_slider_frames(x: i32, width: i32) -> Vec<SkinRect> {
    (0..28)
        .map(|frame| SkinRect::new(x, frame * 15, width, 15))
        .collect()
}

fn equalizer_slider_frames() -> Vec<SkinRect> {
    (0..28)
        .map(|frame| {
            let (x, y) = if frame < 14 {
                (13 + frame * 15, 164)
            } else {
                (13 + (frame - 14) * 15, 229)
            };
            SkinRect::new(x, y, 14, 63)
        })
        .collect()
}

fn shaded_equalizer_frames() -> Vec<SkinRect> {
    [1, 4, 7, 11, 14, 17]
        .into_iter()
        .map(|x| SkinRect::new(x, 30, 3, 7))
        .collect()
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

fn premultiply_rgba([r, g, b, a]: [u8; 4]) -> u32 {
    let pr = ((u16::from(r) * u16::from(a) + 127) / 255) as u32;
    let pg = ((u16::from(g) * u16::from(a) + 127) / 255) as u32;
    let pb = ((u16::from(b) * u16::from(a) + 127) / 255) as u32;
    (u32::from(a) << 24) | (pr << 16) | (pg << 8) | pb
}

fn finite_unit(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn interpolate_rgba(start: [u8; 4], end: [u8; 4], fraction: f64) -> [u8; 4] {
    let fraction = finite_unit(fraction);
    std::array::from_fn(|index| {
        (f64::from(start[index]) + (f64::from(end[index]) - f64::from(start[index])) * fraction)
            .round()
            .clamp(0.0, 255.0) as u8
    })
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

    fn row_flow_canvas_height(skin: &DefaultSkin, zoom: f64) -> i32 {
        let mut x = CANVAS_MARGIN;
        let mut y = CANVAS_MARGIN + LABEL_HEIGHT;
        let mut row_height = 0;
        for kind in SkinPixmapKind::ALL {
            let info = kind.info();
            let (width, height) = skin
                .get(kind)
                .map(|image| (image.width() as i32, image.height() as i32))
                .unwrap_or((info.width as i32, info.height as i32));
            if x > CANVAS_MARGIN && x + width > MAX_ATLAS_WIDTH {
                x = CANVAS_MARGIN;
                y += row_height + ELEMENT_GAP + LABEL_HEIGHT;
                row_height = 0;
            }
            x += width + ELEMENT_GAP;
            row_height = row_height.max(height);
        }
        (f64::from(y + row_height + CANVAS_MARGIN) * zoom.max(MIN_ZOOM)).ceil() as i32
    }

    fn canvas_coord(position: i32, zoom: f64) -> f64 {
        f64::from(position) * zoom
    }

    #[test]
    fn layout_contains_every_pixmap_and_canvas_has_size() {
        let editor = SkinEditorState::default();
        let skin = DefaultSkin::load_bundled().unwrap();
        let slots = editor.layout(&skin);

        assert_eq!(slots.len(), SkinPixmapKind::ALL.len());
        assert_eq!(slots[0].kind, SkinPixmapKind::Main);
        assert!(editor.canvas_size(&slots).0 > 0);
        assert!(editor.canvas_size(&slots).1 > 0);
    }

    #[test]
    fn compact_layout_does_not_overlap_and_is_tighter_than_row_flow() {
        let editor = SkinEditorState::default();
        let skin = DefaultSkin::load_bundled().unwrap();
        let slots = editor.layout(&skin);
        for (index, a) in slots.iter().enumerate() {
            let a_rect = PackRect {
                x: a.x,
                y: a.y - LABEL_HEIGHT,
                width: a.width,
                height: a.height + LABEL_HEIGHT,
            };
            for b in slots.iter().skip(index + 1) {
                let b_rect = PackRect {
                    x: b.x,
                    y: b.y - LABEL_HEIGHT,
                    width: b.width,
                    height: b.height + LABEL_HEIGHT,
                };
                assert!(
                    !a_rect.intersects(b_rect),
                    "{:?} overlaps {:?}",
                    a.kind,
                    b.kind
                );
            }
        }

        let (_, compact_height) = editor.canvas_size(&slots);
        let old_row_flow_height = row_flow_canvas_height(&skin, editor.zoom);
        assert!(compact_height < old_row_flow_height);
    }

    #[test]
    fn compact_layout_is_approximately_square() {
        let editor = SkinEditorState::default();
        let skin = DefaultSkin::load_bundled().unwrap();
        let slots = editor.layout(&skin);
        let (width, height) = editor.canvas_size(&slots);
        let ratio = f64::from(width.max(height)) / f64::from(width.min(height));

        assert!(ratio <= 1.35, "{width}x{height} ratio {ratio}");
    }

    #[test]
    fn hit_test_maps_canvas_position_to_pixmap_pixel() {
        let editor = SkinEditorState::default();
        let skin = DefaultSkin::load_bundled().unwrap();
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        assert_eq!(
            editor.hit_test(
                &slots,
                canvas_coord(main.x + 3, editor.zoom),
                canvas_coord(main.y + 4, editor.zoom)
            ),
            Some((SkinPixmapKind::Main, 3, 4))
        );
        assert_eq!(editor.hit_test(&slots, 0.0, 0.0), None);
    }

    #[test]
    fn brush_size_and_zoom_are_clamped_to_ui_ranges() {
        let mut editor = SkinEditorState::default();

        editor.set_brush_size(99);
        editor.set_zoom(99.0);

        assert_eq!(editor.brush_size, 15);
        assert_eq!(editor.zoom, MAX_ZOOM);

        editor.set_brush_size(0);
        editor.set_zoom(0.0);

        assert_eq!(editor.brush_size, 1);
        assert_eq!(editor.zoom, MIN_ZOOM);

        editor.set_zoom(2.25);
        assert_eq!(editor.zoom, 2.25);
    }

    #[test]
    fn color_shelf_stores_and_picks_colors() {
        let mut editor = SkinEditorState {
            color: [12, 34, 56, 78],
            ..SkinEditorState::default()
        };

        assert_eq!(editor.pick_color_shelf_slot(0), None);
        assert_eq!(editor.store_color_shelf_slot(0), Some([12, 34, 56, 78]));
        editor.color = [1, 2, 3, 4];

        assert_eq!(editor.pick_color_shelf_slot(0), Some([12, 34, 56, 78]));
        assert_eq!(editor.color, [12, 34, 56, 78]);
        assert_eq!(editor.store_color_shelf_slot(COLOR_SHELF_SIZE), None);
        assert_eq!(editor.pick_color_shelf_slot(COLOR_SHELF_SIZE), None);
    }

    #[test]
    fn gradient_endpoints_store_pick_and_interpolate_colors() {
        let mut editor = SkinEditorState {
            color: [10, 20, 30, 40],
            ..SkinEditorState::default()
        };

        assert_eq!(editor.store_gradient_start(), [10, 20, 30, 40]);
        editor.color = [110, 220, 230, 240];
        assert_eq!(editor.store_gradient_end(), [110, 220, 230, 240]);

        assert_eq!(editor.gradient_color_at(0.0), [10, 20, 30, 40]);
        assert_eq!(editor.gradient_color_at(0.5), [60, 120, 130, 140]);
        assert_eq!(editor.gradient_color_at(1.0), [110, 220, 230, 240]);
        assert_eq!(editor.gradient_color_at(-1.0), [10, 20, 30, 40]);
        assert_eq!(editor.gradient_color_at(f64::NAN), [10, 20, 30, 40]);

        editor.color = [1, 2, 3, 4];
        assert_eq!(editor.pick_gradient_start(), [10, 20, 30, 40]);
        assert_eq!(editor.color, [10, 20, 30, 40]);
        assert_eq!(editor.pick_gradient_color_at(0.25), [35, 70, 80, 90]);
        assert_eq!(editor.color, [35, 70, 80, 90]);
        assert_eq!(editor.pick_gradient_end(), [110, 220, 230, 240]);
    }

    #[test]
    fn multi_stop_gradient_interpolates_and_shelves() {
        let mut editor = SkinEditorState {
            gradient: SkinGradient::new(vec![
                GradientStop {
                    position: 0.0,
                    color: [0, 0, 0, 255],
                },
                GradientStop {
                    position: 0.5,
                    color: [255, 0, 0, 255],
                },
                GradientStop {
                    position: 1.0,
                    color: [255, 255, 255, 255],
                },
            ]),
            ..SkinEditorState::default()
        };

        assert_eq!(editor.gradient_color_at(0.25), [128, 0, 0, 255]);
        assert_eq!(editor.gradient_color_at(0.75), [255, 128, 128, 255]);
        editor.add_gradient_stop(0.25);
        assert_eq!(editor.gradient_stops().len(), 4);
        assert!(editor.store_gradient_shelf_slot(0).is_some());
        editor.gradient = SkinGradient::default();
        assert!(editor.pick_gradient_shelf_slot(0).is_some());
        assert_eq!(editor.gradient_stops().len(), 4);
    }

    #[test]
    fn color_picker_picks_existing_skin_pixel() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        skin.get_mut(SkinPixmapKind::Main)
            .unwrap()
            .set_pixel_rgba(4, 5, [90, 91, 92, 255]);
        let mut editor = SkinEditorState {
            tool: Tool::ColorPicker,
            color: [1, 2, 3, 255],
            ..SkinEditorState::default()
        };
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        assert!(!editor.press(
            &mut skin,
            &slots,
            canvas_coord(main.x + 4, editor.zoom),
            canvas_coord(main.y + 5, editor.zoom),
        ));
        assert_eq!(editor.color, [90, 91, 92, 255]);
    }

    #[test]
    fn brush_paints_continuous_path_with_size() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            color: [1, 2, 3, 255],
            brush_size: 1,
            ..SkinEditorState::default()
        };
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();
        let sx = canvas_coord(main.x, editor.zoom);
        let sy = canvas_coord(main.y, editor.zoom);
        let ex = canvas_coord(main.x + 3, editor.zoom);
        let ey = canvas_coord(main.y, editor.zoom);

        assert!(editor.press(&mut skin, &slots, sx, sy));
        assert!(editor.drag(&mut skin, &slots, ex, ey));

        let main = skin.get(SkinPixmapKind::Main).unwrap();
        for x in 0..=3 {
            assert_eq!(main.pixel_argb(x, 0), Some(0xff01_0203));
        }
    }

    #[test]
    fn coupled_brush_repeats_slider_frame_edits() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            color: [250, 10, 20, 255],
            brush_size: 1,
            couple_control_edits: true,
            ..SkinEditorState::default()
        };
        let slots = editor.layout(&skin);
        let volume = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Volume)
            .unwrap();

        assert!(editor.press(
            &mut skin,
            &slots,
            canvas_coord(volume.x + 2, editor.zoom),
            canvas_coord(volume.y + 3, editor.zoom),
        ));

        let volume = skin.get(SkinPixmapKind::Volume).unwrap();
        assert_eq!(volume.pixel_argb(2, 3), Some(0xfffa_0a14));
        assert_eq!(volume.pixel_argb(2, 18), Some(0xfffa_0a14));
        assert_eq!(volume.pixel_argb(2, 408), Some(0xfffa_0a14));
    }

    #[test]
    fn coupled_gradient_colors_slider_frames_by_value() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            color: [250, 10, 20, 255],
            gradient: SkinGradient::new(vec![
                GradientStop {
                    position: 0.0,
                    color: [255, 0, 0, 255],
                },
                GradientStop {
                    position: 1.0,
                    color: [0, 0, 255, 255],
                },
            ]),
            brush_size: 1,
            couple_control_edits: true,
            coupled_edits_use_gradient: true,
            ..SkinEditorState::default()
        };
        let slots = editor.layout(&skin);
        let volume = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Volume)
            .unwrap();

        assert!(editor.press(
            &mut skin,
            &slots,
            canvas_coord(volume.x + 4, editor.zoom),
            canvas_coord(volume.y + 5, editor.zoom),
        ));

        let volume = skin.get(SkinPixmapKind::Volume).unwrap();
        assert_eq!(volume.pixel_argb(4, 5), Some(0xffff_0000));
        assert_eq!(volume.pixel_argb(4, 410), Some(0xff00_00ff));
    }

    #[test]
    fn fill_tool_replaces_contiguous_poured_color() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let image = skin.get_mut(SkinPixmapKind::Main).unwrap();
        image.set_pixel_rgba(0, 0, [120, 30, 40, 255]);
        image.set_pixel_rgba(1, 0, [120, 30, 40, 255]);
        image.set_pixel_rgba(0, 1, [120, 30, 40, 255]);
        image.set_pixel_rgba(1, 1, [1, 2, 3, 255]);
        image.set_pixel_rgba(3, 3, [120, 30, 40, 255]);
        let mut editor = SkinEditorState {
            tool: Tool::Fill,
            color: [200, 210, 220, 255],
            ..SkinEditorState::default()
        };
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        assert!(editor.press(
            &mut skin,
            &slots,
            canvas_coord(main.x, editor.zoom),
            canvas_coord(main.y, editor.zoom),
        ));

        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.pixel_argb(0, 0), Some(0xffc8_d2dc));
        assert_eq!(main.pixel_argb(1, 0), Some(0xffc8_d2dc));
        assert_eq!(main.pixel_argb(0, 1), Some(0xffc8_d2dc));
        assert_eq!(main.pixel_argb(1, 1), Some(0xff01_0203));
        assert_eq!(main.pixel_argb(3, 3), Some(0xff78_1e28));
    }

    #[test]
    fn rectangle_fill_paints_clamped_area() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut editor = SkinEditorState {
            tool: Tool::Rectangle,
            color: [5, 6, 7, 255],
            ..SkinEditorState::default()
        };
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        editor.press(
            &mut skin,
            &slots,
            canvas_coord(main.x + 1, editor.zoom),
            canvas_coord(main.y + 1, editor.zoom),
        );
        assert!(editor.release(
            &mut skin,
            &slots,
            canvas_coord(main.x + 2, editor.zoom),
            canvas_coord(main.y + 2, editor.zoom),
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
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();
        let before = skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(2, 2);

        editor.press(
            &mut skin,
            &slots,
            canvas_coord(main.x + 1, editor.zoom),
            canvas_coord(main.y + 1, editor.zoom),
        );
        assert!(editor.release(
            &mut skin,
            &slots,
            canvas_coord(main.x + 3, editor.zoom),
            canvas_coord(main.y + 3, editor.zoom),
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
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        editor.press(
            &mut skin,
            &slots,
            canvas_coord(main.x, editor.zoom),
            canvas_coord(main.y, editor.zoom),
        );
        assert!(editor.release(
            &mut skin,
            &slots,
            canvas_coord(main.x + 3, editor.zoom),
            canvas_coord(main.y, editor.zoom),
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
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        editor.press(
            &mut skin,
            &slots,
            canvas_coord(main.x + 1, editor.zoom),
            canvas_coord(main.y + 1, editor.zoom),
        );
        editor.release(
            &mut skin,
            &slots,
            canvas_coord(main.x + 1, editor.zoom),
            canvas_coord(main.y + 1, editor.zoom),
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
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        assert!(editor.press(
            &mut skin,
            &slots,
            canvas_coord(main.x + 20, editor.zoom),
            canvas_coord(main.y + 20, editor.zoom),
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
        let slots = editor.layout(&skin);
        let main = slots
            .iter()
            .find(|slot| slot.kind == SkinPixmapKind::Main)
            .unwrap();

        assert!(editor.press(
            &mut skin,
            &slots,
            canvas_coord(main.x + 5, editor.zoom),
            canvas_coord(main.y + 5, editor.zoom),
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
        let slots = SkinEditorState::default().layout(&skin);
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
            canvas_coord(main_slot.x + 6, lighten.zoom),
            canvas_coord(main_slot.y + 6, lighten.zoom),
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
            canvas_coord(main_slot.x + 6, darken.zoom),
            canvas_coord(main_slot.y + 6, darken.zoom),
        ));
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(6, 6),
            Some(0xff64_6464)
        );
    }
}
