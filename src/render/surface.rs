use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;
use std::sync::OnceLock;

use fontdue::Font;

#[derive(Debug, Clone)]
pub struct Error(String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone)]
pub struct BorrowError(String);

impl BorrowError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for BorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BorrowError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    ARgb32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extend {
    Pad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Nearest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Antialias {
    Gray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintStyle {
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintMetrics {
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSlant {
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Normal,
    Bold,
}

#[derive(Debug, Default, Clone)]
pub struct FontOptions;

impl FontOptions {
    pub fn new() -> Result<Self, Error> {
        Ok(Self)
    }

    pub fn set_antialias(&mut self, _value: Antialias) {}
    pub fn set_hint_style(&mut self, _value: HintStyle) {}
    pub fn set_hint_metrics(&mut self, _value: HintMetrics) {}
}

#[derive(Debug, Clone, Copy)]
pub struct Rectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rectangle {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TextExtents {
    width: f64,
    height: f64,
    y_bearing: f64,
}

impl TextExtents {
    pub fn width(&self) -> f64 {
        self.width
    }

    pub fn height(&self) -> f64 {
        self.height
    }

    pub fn y_bearing(&self) -> f64 {
        self.y_bearing
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FontExtents {
    ascent: f64,
}

impl FontExtents {
    pub fn ascent(&self) -> f64 {
        self.ascent
    }
}

#[derive(Clone)]
pub struct ImageSurface {
    inner: Rc<ImageSurfaceInner>,
}

struct ImageSurfaceInner {
    width: i32,
    height: i32,
    stride: i32,
    data: RefCell<Vec<u8>>, // native-endian ARgb32-compatible BGRA bytes
}

impl ImageSurface {
    pub fn create(_format: Format, width: i32, height: i32) -> Result<Self, Error> {
        if width < 0 || height < 0 {
            return Err(Error::new(format!(
                "negative surface dimensions {width}x{height}"
            )));
        }
        let stride = width.saturating_mul(4);
        let len = (stride as usize).saturating_mul(height as usize);
        Ok(Self {
            inner: Rc::new(ImageSurfaceInner {
                width,
                height,
                stride,
                data: RefCell::new(vec![0; len]),
            }),
        })
    }

    pub fn width(&self) -> i32 {
        self.inner.width
    }

    pub fn height(&self) -> i32 {
        self.inner.height
    }

    pub fn stride(&self) -> i32 {
        self.inner.stride
    }

    pub fn flush(&self) {}
    pub fn mark_dirty(&self) {}

    pub fn data(&self) -> Result<Ref<'_, [u8]>, BorrowError> {
        self.inner
            .data
            .try_borrow()
            .map(|borrow| Ref::map(borrow, |vec| vec.as_slice()))
            .map_err(|err| BorrowError(err.to_string()))
    }

    pub fn set_pixel_argb(&self, x: i32, y: i32, argb: u32) {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return;
        }
        let offset = y as usize * self.stride() as usize + x as usize * 4;
        if let Ok(mut data) = self.inner.data.try_borrow_mut() {
            data[offset..offset + 4].copy_from_slice(&argb.to_ne_bytes());
        }
    }

    pub fn pixel_argb(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return 0;
        }
        let offset = y as usize * self.stride() as usize + x as usize * 4;
        let data = self.inner.data.borrow();
        u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap())
    }

    pub fn blend_pixel_rgba(&self, x: i32, y: i32, source: [u8; 4], coverage: u8) {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() || coverage == 0 {
            return;
        }
        let src_a = (u16::from(source[3]) * u16::from(coverage)) / 255;
        if src_a == 255 {
            self.set_pixel_argb(x, y, rgba_to_argb([source[0], source[1], source[2], 255]));
            return;
        }
        let dst = argb_to_rgba(self.pixel_argb(x, y));
        let inv_a = 255 - src_a;
        let blend = |src: u8, dst: u8| -> u8 {
            ((u16::from(src) * src_a + u16::from(dst) * inv_a) / 255) as u8
        };
        let out_a = (src_a + (u16::from(dst[3]) * inv_a) / 255) as u8;
        self.set_pixel_argb(
            x,
            y,
            rgba_to_argb([
                blend(source[0], dst[0]),
                blend(source[1], dst[1]),
                blend(source[2], dst[2]),
                out_a,
            ]),
        );
    }

    pub fn create_for_rectangle(&self, rect: Rectangle) -> Result<Self, Error> {
        let x = rect.x.floor() as i32;
        let y = rect.y.floor() as i32;
        let width = rect.width.ceil().max(0.0) as i32;
        let height = rect.height.ceil().max(0.0) as i32;
        let cropped = ImageSurface::create(Format::ARgb32, width, height)?;
        for yy in 0..height {
            for xx in 0..width {
                cropped.set_pixel_argb(xx, yy, self.pixel_argb(x + xx, y + yy));
            }
        }
        Ok(cropped)
    }

    pub fn to_rgba(&self) -> Vec<u8> {
        let width = self.width().max(0) as usize;
        let height = self.height().max(0) as usize;
        let stride = self.stride().max(0) as usize;
        let data = self.inner.data.borrow();
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            let row = &data[y * stride..y * stride + width * 4];
            for px in row.chunks_exact(4) {
                let [b, g, r, a] = px else { unreachable!() };
                rgba.extend_from_slice(&[*r, *g, *b, *a]);
            }
        }
        rgba
    }

    pub fn save_png(&self, path: &std::path::Path) -> std::io::Result<()> {
        let rgba = self.to_rgba();
        image::RgbaImage::from_raw(self.width() as u32, self.height() as u32, rgba)
            .ok_or_else(|| std::io::Error::other("invalid image surface buffer"))?
            .save(path)
            .map_err(std::io::Error::other)
    }

    pub fn to_tiny_skia_pixmap(&self) -> Option<tiny_skia::Pixmap> {
        tiny_skia::Pixmap::from_vec(
            self.to_rgba(),
            tiny_skia::IntSize::from_wh(self.width() as u32, self.height() as u32)?,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct ClipRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl ClipRect {
    fn full(surface: &ImageSurface) -> Self {
        Self {
            x: 0,
            y: 0,
            width: surface.width(),
            height: surface.height(),
        }
    }

    fn intersect(self, other: Self) -> Self {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);
        Self {
            x: x1,
            y: y1,
            width: (x2 - x1).max(0),
            height: (y2 - y1).max(0),
        }
    }

    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

#[derive(Clone)]
struct ContextState {
    color: [u8; 4],
    clip: ClipRect,
    rect: Option<Rectangle>,
    path: Vec<Rectangle>,
    source: Option<(ImageSurface, f64, f64)>,
    tx: f64,
    ty: f64,
    sx: f64,
    sy: f64,
    font_size: f64,
    font_family: String,
    font_slant: FontSlant,
    font_weight: FontWeight,
    current_x: f64,
    current_y: f64,
}

pub struct Context {
    surface: ImageSurface,
    state: RefCell<ContextState>,
    stack: RefCell<Vec<ContextState>>,
}

impl Context {
    pub fn new(surface: &ImageSurface) -> Result<Self, Error> {
        Ok(Self {
            surface: surface.clone(),
            state: RefCell::new(ContextState {
                color: [0, 0, 0, 255],
                clip: ClipRect::full(surface),
                rect: None,
                path: Vec::new(),
                source: None,
                tx: 0.0,
                ty: 0.0,
                sx: 1.0,
                sy: 1.0,
                font_size: 10.0,
                font_family: "Helvetica".to_string(),
                font_slant: FontSlant::Normal,
                font_weight: FontWeight::Normal,
                current_x: 0.0,
                current_y: 0.0,
            }),
            stack: RefCell::new(Vec::new()),
        })
    }

    pub fn save(&self) -> Result<(), Error> {
        self.stack.borrow_mut().push(self.state.borrow().clone());
        Ok(())
    }

    pub fn restore(&self) -> Result<(), Error> {
        if let Some(state) = self.stack.borrow_mut().pop() {
            *self.state.borrow_mut() = state;
        }
        Ok(())
    }

    pub fn scale(&self, sx: f64, sy: f64) {
        let mut state = self.state.borrow_mut();
        state.sx *= sx;
        state.sy *= sy;
    }

    pub fn translate(&self, tx: f64, ty: f64) {
        let mut state = self.state.borrow_mut();
        state.tx += tx * state.sx;
        state.ty += ty * state.sy;
    }

    pub fn rectangle(&self, x: f64, y: f64, width: f64, height: f64) {
        let mut state = self.state.borrow_mut();
        let rect = Rectangle::new(x, y, width, height);
        state.rect = Some(rect);
        state.path.push(rect);
    }

    fn transformed_rect(&self, rect: Rectangle) -> ClipRect {
        let state = self.state.borrow();
        let x = (rect.x * state.sx + state.tx).floor() as i32;
        let y = (rect.y * state.sy + state.ty).floor() as i32;
        let width = (rect.width * state.sx).ceil().max(0.0) as i32;
        let height = (rect.height * state.sy).ceil().max(0.0) as i32;
        ClipRect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn clip(&self) {
        let rect = self.state.borrow().rect;
        if let Some(rect) = rect {
            let transformed = self.transformed_rect(rect);
            let mut state = self.state.borrow_mut();
            state.clip = state.clip.intersect(transformed);
        }
    }

    pub fn new_path(&self) {
        let mut state = self.state.borrow_mut();
        state.rect = None;
        state.path.clear();
    }

    pub fn set_source_rgb(&self, r: f64, g: f64, b: f64) {
        let mut state = self.state.borrow_mut();
        state.color = [unit_to_u8(r), unit_to_u8(g), unit_to_u8(b), 255];
        state.source = None;
    }

    pub fn set_source_surface(&self, surface: &ImageSurface, x: f64, y: f64) -> Result<(), Error> {
        self.state.borrow_mut().source = Some((surface.clone(), x, y));
        Ok(())
    }

    pub fn source(&self) -> Pattern {
        Pattern
    }

    pub fn fill(&self) -> Result<(), Error> {
        let clip = self.state.borrow().clip;
        let color = self.state.borrow().color;
        let path = std::mem::take(&mut self.state.borrow_mut().path);
        for rect in path {
            let rect = self.transformed_rect(rect).intersect(clip);
            for y in rect.y..rect.y + rect.height {
                for x in rect.x..rect.x + rect.width {
                    self.surface.set_pixel_argb(x, y, rgba_to_argb(color));
                }
            }
        }
        Ok(())
    }

    pub fn stroke(&self) -> Result<(), Error> {
        let Some(rect) = self.state.borrow().rect else {
            return Ok(());
        };
        let clip = self.state.borrow().clip;
        let color = rgba_to_argb(self.state.borrow().color);
        let rect = self.transformed_rect(rect).intersect(clip);
        for x in rect.x..rect.x + rect.width {
            self.surface.set_pixel_argb(x, rect.y, color);
            self.surface
                .set_pixel_argb(x, rect.y + rect.height - 1, color);
        }
        for y in rect.y..rect.y + rect.height {
            self.surface.set_pixel_argb(rect.x, y, color);
            self.surface
                .set_pixel_argb(rect.x + rect.width - 1, y, color);
        }
        Ok(())
    }

    pub fn paint(&self) -> Result<(), Error> {
        let state = self.state.borrow().clone();
        let Some((source, source_x, source_y)) = state.source else {
            let color = rgba_to_argb(state.color);
            for y in state.clip.y..state.clip.y + state.clip.height {
                for x in state.clip.x..state.clip.x + state.clip.width {
                    self.surface.set_pixel_argb(x, y, color);
                }
            }
            return Ok(());
        };
        let dest_x = (source_x * state.sx + state.tx).round() as i32;
        let dest_y = (source_y * state.sy + state.ty).round() as i32;
        let dest_w = ((source.width() as f64) * state.sx).round().max(1.0) as i32;
        let dest_h = ((source.height() as f64) * state.sy).round().max(1.0) as i32;
        for dy in 0..dest_h {
            let sy = ((dy as f64) / state.sy).floor() as i32;
            for dx in 0..dest_w {
                let x = dest_x + dx;
                let y = dest_y + dy;
                if !state.clip.contains(x, y) {
                    continue;
                }
                let sx = ((dx as f64) / state.sx).floor() as i32;
                let pixel = source.pixel_argb(sx, sy);
                self.surface.set_pixel_argb(x, y, pixel);
            }
        }
        Ok(())
    }

    pub fn select_font_face(&self, family: &str, slant: FontSlant, weight: FontWeight) {
        let mut state = self.state.borrow_mut();
        state.font_family = family.to_string();
        state.font_slant = slant;
        state.font_weight = weight;
    }
    pub fn set_font_size(&self, size: f64) {
        self.state.borrow_mut().font_size = size;
    }
    pub fn set_font_options(&self, _options: &FontOptions) {}

    pub fn font_extents(&self) -> Result<FontExtents, Error> {
        Ok(FontExtents {
            ascent: self.state.borrow().font_size * 0.8,
        })
    }

    pub fn text_extents(&self, text: &str) -> Result<TextExtents, Error> {
        let state = self.state.borrow();
        let font = font_for(&state.font_family, state.font_slant, state.font_weight)?;
        let size = state.font_size as f32;
        let width = text
            .chars()
            .map(|ch| font.metrics(ch, size).advance_width as f64)
            .sum();
        Ok(TextExtents {
            width,
            height: self.state.borrow().font_size,
            y_bearing: -self.state.borrow().font_size * 0.8,
        })
    }

    pub fn move_to(&self, x: f64, y: f64) {
        let mut state = self.state.borrow_mut();
        state.current_x = x;
        state.current_y = y;
    }

    pub fn matrix(&self) -> Matrix {
        let state = self.state.borrow();
        Matrix {
            xx: state.sx,
            yy: state.sy,
        }
    }

    pub fn show_text(&self, text: &str) -> Result<(), Error> {
        let state = self.state.borrow().clone();
        let font = font_for(&state.font_family, state.font_slant, state.font_weight)?;
        let scale_x = state.sx.abs() as f32;
        let scale_y = state.sy.abs() as f32;
        let scaled = (scale_x - 1.0).abs() > f32::EPSILON || (scale_y - 1.0).abs() > f32::EPSILON;

        if scaled {
            // Cairo rasterizes text after the CTM is applied, so a 10px font at
            // 2x zoom becomes a crisp 20px glyph. If we rasterize at the base
            // font size and only scale pixel coordinates, every source pixel lands
            // on every Nth device pixel and the playlist text looks blurry/sparse
            // at integer zoom levels. Render transformed text directly in device
            // space instead.
            let device_scale = scale_x.max(scale_y).max(0.01);
            let device_font_size = (state.font_size as f32 * device_scale).max(1.0);
            let mut pen_x = (state.current_x as f32 * scale_x) + state.tx as f32;
            let baseline = (state.current_y as f32 * scale_y) + state.ty as f32;
            for ch in text.chars() {
                let (metrics, bitmap) = font.rasterize(ch, device_font_size);
                let origin_x = pen_x + metrics.xmin as f32;
                let origin_y = baseline - metrics.height as f32 - metrics.ymin as f32;
                for by in 0..metrics.height {
                    for bx in 0..metrics.width {
                        let alpha = bitmap[by * metrics.width + bx];
                        if alpha == 0 {
                            continue;
                        }
                        let x = (origin_x + bx as f32).round() as i32;
                        let y = (origin_y + by as f32).round() as i32;
                        if !state.clip.contains(x, y) {
                            continue;
                        }
                        self.surface.blend_pixel_rgba(x, y, state.color, alpha);
                    }
                }
                pen_x += metrics.advance_width;
            }
            return Ok(());
        }

        let mut pen_x = state.current_x as f32;
        let baseline = state.current_y as f32;
        for ch in text.chars() {
            let (metrics, bitmap) = font.rasterize(ch, state.font_size as f32);
            let origin_x = pen_x + metrics.xmin as f32;
            let origin_y = baseline - metrics.height as f32 - metrics.ymin as f32;
            for by in 0..metrics.height {
                for bx in 0..metrics.width {
                    let alpha = bitmap[by * metrics.width + bx];
                    if alpha == 0 {
                        continue;
                    }
                    let x = ((origin_x + bx as f32) as f64 * state.sx + state.tx).round() as i32;
                    let y = ((origin_y + by as f32) as f64 * state.sy + state.ty).round() as i32;
                    if !state.clip.contains(x, y) {
                        continue;
                    }
                    self.surface.blend_pixel_rgba(x, y, state.color, alpha);
                }
            }
            pen_x += metrics.advance_width;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Matrix {
    xx: f64,
    yy: f64,
}

impl Matrix {
    pub fn xx(&self) -> f64 {
        self.xx
    }
    pub fn yy(&self) -> f64 {
        self.yy
    }
}

pub struct Pattern;
impl Pattern {
    pub fn set_extend(&self, _extend: Extend) {}
    pub fn set_filter(&self, _filter: Filter) {}
}

fn unit_to_u8(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn rgba_to_argb([r, g, b, a]: [u8; 4]) -> u32 {
    u32::from_be_bytes([a, r, g, b])
}

fn argb_to_rgba(argb: u32) -> [u8; 4] {
    let [a, r, g, b] = argb.to_be_bytes();
    [r, g, b, a]
}

fn font_for(_family: &str, slant: FontSlant, weight: FontWeight) -> Result<&'static Font, Error> {
    static REGULAR: OnceLock<Result<Font, String>> = OnceLock::new();
    static BOLD: OnceLock<Result<Font, String>> = OnceLock::new();
    static ITALIC: OnceLock<Result<Font, String>> = OnceLock::new();
    static BOLD_ITALIC: OnceLock<Result<Font, String>> = OnceLock::new();

    let (slot, bytes): (&OnceLock<Result<Font, String>>, &'static [u8]) = match (slant, weight) {
        (FontSlant::Normal, FontWeight::Normal) => (
            &REGULAR,
            include_bytes!("../../data/fonts/Arimo-Regular.ttf"),
        ),
        (FontSlant::Normal, FontWeight::Bold) => {
            (&BOLD, include_bytes!("../../data/fonts/Arimo-Bold.ttf"))
        }
        (FontSlant::Italic, FontWeight::Normal) => {
            (&ITALIC, include_bytes!("../../data/fonts/Arimo-Italic.ttf"))
        }
        (FontSlant::Italic, FontWeight::Bold) => (
            &BOLD_ITALIC,
            include_bytes!("../../data/fonts/Arimo-BoldItalic.ttf"),
        ),
    };
    slot.get_or_init(|| {
        Font::from_bytes(bytes, fontdue::FontSettings::default()).map_err(|err| err.to_string())
    })
    .as_ref()
    .map_err(|err| Error::new(err.clone()))
}
