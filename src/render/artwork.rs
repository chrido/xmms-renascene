//! Pure, bounded square artwork inspired by the classic player's MilkDrop mode.
//!
//! Geometry is a scalar palette-index field, independent of skin colors. Glow
//! and layer blending happen in that field, never by adding arbitrary RGB hues.
//! There is no filesystem, GL, egui, JNI, or audio-backend access here.

use std::f32::consts::TAU;

use crate::artwork::{ArtworkError, ArtworkPalette, ArtworkSpec};

const PARTICLE_COUNT: usize = 12;
const MAX_PALETTE_INDEX: f32 = 23.0;

/// Validated geometry. Private storage prevents malformed dimensions/non-finite
/// values from being injected into the color-mapping stage.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtworkField {
    width: u32,
    height: u32,
    indices: Vec<f32>,
}

impl ArtworkField {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn indices(&self) -> &[f32] {
        &self.indices
    }
}

/// Opaque, unpremultiplied ARGB words, ready for Android's int[] bitmap bridge.
/// Image ownership is transferred without requiring an egui/Cairo image copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkImage {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl ArtworkImage {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    pub fn into_argb(self) -> Vec<u32> {
        self.pixels
    }
}

/// Linear interpolation in encoded RGB between adjacent skin palette entries.
/// Clamp infinities to the endpoints; a NaN (not emitted by the renderer) maps
/// safely to the background. Alpha is always 255, including an all-black skin.
pub fn map_artwork_palette(index: f32, palette: &ArtworkPalette) -> u32 {
    let index = if index.is_nan() {
        0.0
    } else {
        index.clamp(0.0, MAX_PALETTE_INDEX)
    };
    let low = index.floor() as usize;
    let high = (low + 1).min(23);
    let fraction = index - low as f32;
    let channel = |n: usize| {
        let a = f32::from(palette[low][n]);
        let b = f32::from(palette[high][n]);
        (a + (b - a) * fraction).round() as u32
    };
    0xff00_0000 | (channel(0) << 16) | (channel(1) << 8) | channel(2)
}

pub fn render_artwork(spec: &ArtworkSpec) -> Result<ArtworkImage, ArtworkError> {
    render_artwork_cancellable(spec, || false)
}

/// Cancellation is checked before allocations and at row/stage boundaries.
/// A worker must still validate request identity before publishing the result.
pub fn render_artwork_cancellable(
    spec: &ArtworkSpec,
    cancelled: impl Fn() -> bool,
) -> Result<ArtworkImage, ArtworkError> {
    let field = render_artwork_field_cancellable(spec, &cancelled)?;
    color_artwork_field_cancellable(&field, &spec.palette, &cancelled)
}

pub fn render_artwork_field(spec: &ArtworkSpec) -> Result<ArtworkField, ArtworkError> {
    render_artwork_field_cancellable(spec, &|| false)
}

pub fn color_artwork_field(
    field: &ArtworkField,
    palette: &ArtworkPalette,
) -> Result<ArtworkImage, ArtworkError> {
    color_artwork_field_cancellable(field, palette, &|| false)
}

fn check_cancelled(cancelled: &impl Fn() -> bool) -> Result<(), ArtworkError> {
    if cancelled() {
        Err(ArtworkError::Cancelled)
    } else {
        Ok(())
    }
}

fn color_artwork_field_cancellable(
    field: &ArtworkField,
    palette: &ArtworkPalette,
    cancelled: &impl Fn() -> bool,
) -> Result<ArtworkImage, ArtworkError> {
    check_cancelled(cancelled)?;
    let mut pixels = Vec::with_capacity(field.indices.len());
    for row in field.indices.chunks_exact(field.width as usize) {
        check_cancelled(cancelled)?;
        pixels.extend(row.iter().map(|index| map_artwork_palette(*index, palette)));
    }
    check_cancelled(cancelled)?;
    Ok(ArtworkImage {
        width: field.width,
        height: field.height,
        pixels,
    })
}

// SplitMix64 only chooses fixed bounded geometric parameters. Its bits never
// choose colors. Unlike a mutable/global PRNG, it is repeatable on all targets.
struct Parameters {
    state: u64,
}

impl Parameters {
    fn fraction(&mut self) -> f32 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut bits = self.state;
        bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        bits ^= bits >> 31;
        ((bits >> 40) as u32) as f32 / 16_777_216.0
    }
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn render_artwork_field_cancellable(
    spec: &ArtworkSpec,
    cancelled: &impl Fn() -> bool,
) -> Result<ArtworkField, ArtworkError> {
    let pixel_count = spec.pixel_count()?;
    check_cancelled(cancelled)?;
    let width = spec.width as usize;
    let mut indices = vec![0.0; pixel_count];
    let mut parameters = Parameters {
        state: spec.stable_track_seed,
    };
    let phase = parameters.fraction() * TAU;
    let rotation = parameters.fraction() * TAU;
    let lobes = 3.0 + (parameters.fraction() * 5.0).floor();
    let tunnel_frequency = 10.0 + parameters.fraction() * 8.0;
    let plasma_frequency = 4.0 + parameters.fraction() * 4.0;
    let wave_frequency = 3.0 + parameters.fraction() * 3.0;
    let wave_offset = parameters.fraction() * 0.18 - 0.09;
    let (sin_rotation, cos_rotation) = rotation.sin_cos();
    let coordinate = |pixel: usize| (pixel as f32 + 0.5) * 2.0 / width as f32 - 1.0;

    // Two bounded analytic layers: warped tunnel and plasma. A radial envelope
    // leaves corner/background pixels at palette index 0, with detail inset from
    // native-control cropping. All arithmetic inputs have bounded finite ranges.
    for y in 0..width {
        check_cancelled(cancelled)?;
        let ny = coordinate(y);
        for x in 0..width {
            let nx = coordinate(x);
            let rx = nx * cos_rotation - ny * sin_rotation;
            let ry = nx * sin_rotation + ny * cos_rotation;
            let radius = nx.hypot(ny);
            let envelope = smoothstep((1.12 - radius) / 0.48);
            if envelope == 0.0 {
                continue;
            }
            let angle = ry.atan2(rx);
            let warp =
                (angle * lobes + phase).sin() * 0.12 + (radius * 8.0 - phase * 0.8).sin() * 0.10;
            let tunnel = ((radius + warp) * tunnel_frequency - phase).sin();
            let plasma = ((rx - ry) * plasma_frequency + phase).sin()
                * ((rx + ry) * (plasma_frequency * 0.8) - phase).cos();
            indices[y * width + x] =
                ((9.0 + tunnel * 5.0 + plasma * 3.5) * envelope).clamp(0.0, 18.0);
        }
    }

    // Wave ribbons: add only scalar envelopes. The soft halo precedes the core
    // and is inset horizontally; it cannot inject a non-palette RGB glow.
    let pixel_step = 2.0 / width as f32;
    for x in 0..width {
        check_cancelled(cancelled)?;
        let nx = coordinate(x);
        let envelope = smoothstep((0.88 - nx.abs()) / 0.18);
        if envelope == 0.0 {
            continue;
        }
        for ribbon in 0..2 {
            let sign = if ribbon == 0 { 1.0 } else { -1.0 };
            let wave_y = sign
                * ((nx * wave_frequency + phase).sin() * 0.22
                    + (nx * 2.0 - phase).cos() * 0.07
                    + wave_offset);
            let center = ((wave_y + 1.0) * 0.5 * width as f32 - 0.5).round() as isize;
            let halo_radius = (0.055 / pixel_step).ceil() as isize;
            let start = (center - halo_radius).max(0) as usize;
            let end = (center + halo_radius).min(width as isize - 1) as usize;
            for y in start..=end {
                let distance = (coordinate(y) - wave_y).abs();
                let halo = smoothstep(1.0 - distance / 0.055) * 14.0;
                let core = smoothstep(1.0 - distance / 0.014) * 23.0;
                let contribution = halo.max(core) * envelope;
                indices[y * width + x] = indices[y * width + x].max(contribution);
            }
        }
    }

    // Exactly 12 particles, each visited only in a bounded pixel box. Avoid an
    // expensive all-pixels × particles pass and heap allocation per particle.
    for particle in 0..PARTICLE_COUNT {
        check_cancelled(cancelled)?;
        let angle =
            rotation + particle as f32 * TAU / PARTICLE_COUNT as f32 + parameters.fraction() * 0.25;
        let radius = 0.35 + parameters.fraction() * 0.38;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let cx = cos_angle * radius;
        let cy = sin_angle * radius;
        let halo_size = 0.035 + parameters.fraction() * 0.02;
        let pixel_bound = |value: f32| {
            (((value + 1.0) * 0.5 * width as f32).floor() as isize).clamp(0, width as isize - 1)
                as usize
        };
        for y in pixel_bound(cy - halo_size)..=pixel_bound(cy + halo_size) {
            for x in pixel_bound(cx - halo_size)..=pixel_bound(cx + halo_size) {
                let distance_squared = (coordinate(x) - cx).powi(2) + (coordinate(y) - cy).powi(2);
                let halo = smoothstep(1.0 - distance_squared / halo_size.powi(2)) * 17.0;
                let core = smoothstep(1.0 - distance_squared / 0.012_f32.powi(2)) * 23.0;
                indices[y * width + x] = indices[y * width + x].max(halo.max(core));
            }
        }
    }

    smooth_field(&mut indices, width, cancelled)?;
    check_cancelled(cancelled)?;
    Ok(ArtworkField {
        width: spec.width,
        height: spec.height,
        indices,
    })
}

/// A three-row separable [1,2,1] smoothing window, mixed with the original
/// scalar field. Scratch is O(width), not a second image-sized buffer. Retain
/// exact background zeros so the corners remain the skin background color.
fn smooth_field(
    field: &mut [f32],
    width: usize,
    cancelled: &impl Fn() -> bool,
) -> Result<(), ArtworkError> {
    check_cancelled(cancelled)?;
    let horizontal = |row: &[f32], output: &mut [f32]| {
        for x in 0..width {
            output[x] =
                (row[x.saturating_sub(1)] + row[x] * 2.0 + row[(x + 1).min(width - 1)]) * 0.25;
        }
    };
    let mut previous = vec![0.0; width];
    let mut current = vec![0.0; width];
    let mut next = vec![0.0; width];
    horizontal(&field[..width], &mut current);
    previous.copy_from_slice(&current);
    horizontal(&field[width..width * 2], &mut next);
    for y in 0..width {
        check_cancelled(cancelled)?;
        for x in 0..width {
            let index = y * width + x;
            if field[index] != 0.0 {
                let smoothed = (previous[x] + current[x] * 2.0 + next[x]) * 0.25;
                field[index] = (field[index] * 0.65 + smoothed * 0.35).clamp(0.0, 23.0);
            }
        }
        if y + 1 < width {
            std::mem::swap(&mut previous, &mut current);
            std::mem::swap(&mut current, &mut next);
            if y + 2 < width {
                horizontal(&field[(y + 2) * width..(y + 3) * width], &mut next);
            } else {
                next.copy_from_slice(&current);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
