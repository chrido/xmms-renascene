use std::cell::Cell;

use super::*;
use crate::artwork::{stable_track_seed, ARTWORK_RENDERER_VERSION};
use crate::skin::DEFAULT_VIS_COLORS;

fn spec(size: u32, track: &str) -> ArtworkSpec {
    ArtworkSpec {
        renderer_version: ARTWORK_RENDERER_VERSION,
        stable_track_seed: stable_track_seed(track),
        palette: DEFAULT_VIS_COLORS,
        width: size,
        height: size,
    }
}

#[test]
fn supported_sizes_produce_opaque_argb_and_skin_background_corners() {
    for size in [320, 512] {
        let spec = spec(size, "/music/demo.wav");
        let image = render_artwork(&spec).unwrap();
        assert_eq!((image.width(), image.height()), (size, size));
        assert_eq!(image.pixels().len(), (size * size) as usize);
        assert!(image.pixels().iter().all(|pixel| pixel >> 24 == 255));
        let background = map_artwork_palette(0.0, &spec.palette);
        let width = size as usize;
        for corner in [0, width - 1, width * (width - 1), width * width - 1] {
            assert_eq!(image.pixels()[corner], background);
        }
        assert!(image.pixels().iter().any(|pixel| *pixel != background));
        assert_eq!(image.clone().into_argb(), image.pixels());
    }
}

#[test]
fn palette_map_obeys_adjacent_entry_interpolation_rounding_and_alpha() {
    let mut palette = [[0; 3]; 24];
    palette[0] = [2, 4, 8];
    palette[1] = [5, 9, 17];
    palette[23] = [101, 201, 251];
    assert_eq!(map_artwork_palette(0.0, &palette), 0xff02_0408);
    assert_eq!(map_artwork_palette(0.5, &palette), 0xff04_070d);
    assert_eq!(map_artwork_palette(1.0, &palette), 0xff05_0911);
    assert_eq!(map_artwork_palette(-5.0, &palette), 0xff02_0408);
    assert_eq!(map_artwork_palette(23.0, &palette), 0xff65_c9fb);
    assert_eq!(map_artwork_palette(200.0, &palette), 0xff65_c9fb);
    assert_eq!(
        map_artwork_palette(f32::NEG_INFINITY, &palette),
        0xff02_0408
    );
    assert_eq!(map_artwork_palette(f32::INFINITY, &palette), 0xff65_c9fb);
    assert_eq!(map_artwork_palette(f32::NAN, &palette), 0xff02_0408);
}

#[test]
fn each_pixel_matches_the_documented_map_for_contrasting_palettes() {
    let original = spec(320, "/music/demo.wav");
    let field = render_artwork_field(&original).unwrap();
    let red = std::array::from_fn(|i| [i as u8 * 11, 0, 0]);
    let blue = std::array::from_fn(|i| [0, 0, i as u8 * 11]);
    let dark = std::array::from_fn(|i| [i as u8 / 4; 3]);
    let monochrome = std::array::from_fn(|i| [i as u8 * 11; 3]);
    for palette in [
        red,
        blue,
        dark,
        monochrome,
        DEFAULT_VIS_COLORS,
        [[0; 3]; 24],
    ] {
        let mut colored_spec = original.clone();
        colored_spec.palette = palette;
        assert_eq!(render_artwork_field(&colored_spec).unwrap(), field);
        let image = color_artwork_field(&field, &palette).unwrap();
        for (pixel, index) in image.pixels().iter().zip(field.indices()) {
            // Independent reference for the externally documented map.
            let low = index.floor() as usize;
            let high = (low + 1).min(23);
            let fraction = index - low as f32;
            let rgb = std::array::from_fn::<_, 3, _>(|channel| {
                (f32::from(palette[low][channel]) * (1.0 - fraction)
                    + f32::from(palette[high][channel]) * fraction)
                    .round() as i32
            });
            for (shift, expected) in [16, 8, 0].into_iter().zip(rgb) {
                let actual = ((pixel >> shift) & 255) as i32;
                assert!((actual - expected).abs() <= 1);
            }
            assert_eq!(pixel >> 24, 255);
        }
        if palette == [[0; 3]; 24] {
            assert!(image.pixels().iter().all(|pixel| *pixel == 0xff00_0000));
        }
        // The convenience path and separate field-mapping path agree exactly.
        assert_eq!(render_artwork(&colored_spec).unwrap(), image);
    }
}

#[test]
fn geometry_is_repeatable_and_distinct_tracks_change_patterns() {
    let a = render_artwork_field(&spec(320, "/music/demo.wav")).unwrap();
    assert_eq!(
        a,
        render_artwork_field(&spec(320, "/music/demo.wav")).unwrap()
    );
    let b = render_artwork_field(&spec(320, "/other/demo.wav")).unwrap();
    let changed = a
        .indices()
        .iter()
        .zip(b.indices())
        .filter(|(a, b)| (*a - *b).abs() > 0.1)
        .count();
    assert!(
        changed > a.indices().len() / 2,
        "different seeds barely changed geometry: {changed}"
    );
}

#[test]
fn extreme_and_varied_seeds_keep_finite_bounded_fields() {
    for seed in [0, 1, 2, u64::MAX, u64::MAX / 2, 0x8000_0000_0000_0000] {
        let mut spec = spec(320, "seed-overridden");
        spec.stable_track_seed = seed;
        let field = render_artwork_field(&spec).unwrap();
        assert!(field
            .indices()
            .iter()
            .all(|index| index.is_finite() && (0.0..=23.0).contains(index)));
        assert!(
            field.indices().iter().any(|index| *index > 18.0),
            "missing wave/particle accents"
        );
    }
}

#[test]
fn invalid_spec_is_rejected_before_cancellation_callback_or_allocation() {
    let calls = Cell::new(0);
    let mut spec = spec(320, "a");
    for (width, height) in [(0, 0), (320, 512), (u32::MAX, u32::MAX)] {
        spec.width = width;
        spec.height = height;
        assert_eq!(
            render_artwork_cancellable(&spec, || {
                calls.set(calls.get() + 1);
                false
            }),
            Err(ArtworkError::UnsupportedDimensions)
        );
    }
    spec.width = 320;
    spec.height = 320;
    spec.renderer_version += 1;
    assert_eq!(
        render_artwork_cancellable(&spec, || {
            calls.set(calls.get() + 1);
            false
        }),
        Err(ArtworkError::UnsupportedRenderer)
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn cancellation_before_allocation_and_during_geometry_returns_no_image() {
    let spec = spec(320, "a");
    assert_eq!(
        render_artwork_cancellable(&spec, || true),
        Err(ArtworkError::Cancelled)
    );
    let calls = Cell::new(0);
    assert_eq!(
        render_artwork_cancellable(&spec, || {
            calls.set(calls.get() + 1);
            calls.get() >= 10
        }),
        Err(ArtworkError::Cancelled)
    );
    assert_eq!(calls.get(), 10);
}

#[test]
fn cancellation_is_checked_during_scalar_smoothing_and_color_mapping() {
    let calls = Cell::new(0);
    let mut values = vec![10.0; 320 * 320];
    assert_eq!(
        smooth_field(&mut values, 320, &|| {
            calls.set(calls.get() + 1);
            calls.get() == 4
        }),
        Err(ArtworkError::Cancelled)
    );
    assert_eq!(calls.get(), 4);
    let field = render_artwork_field(&spec(320, "a")).unwrap();
    calls.set(0);
    assert_eq!(
        color_artwork_field_cancellable(&field, &DEFAULT_VIS_COLORS, &|| {
            calls.set(calls.get() + 1);
            calls.get() == 4
        }),
        Err(ArtworkError::Cancelled)
    );
    assert_eq!(calls.get(), 4);
}

#[test]
fn sampled_geometry_matches_v1_fixture_with_numeric_tolerance() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/artwork/field-v1.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["renderer_version"].as_u64().unwrap(),
        u64::from(ARTWORK_RENDERER_VERSION)
    );
    let size = fixture["size"].as_u64().unwrap() as u32;
    let grid = fixture["grid"].as_u64().unwrap() as usize;
    let field = render_artwork_field(&spec(size, fixture["track"].as_str().unwrap())).unwrap();
    let samples = fixture["samples"].as_array().unwrap();
    assert_eq!(samples.len(), grid * grid);
    for (i, expected) in samples.iter().enumerate() {
        let x = ((i % grid) * 2 + 1) * size as usize / (grid * 2);
        let y = ((i / grid) * 2 + 1) * size as usize / (grid * 2);
        let actual = field.indices()[y * size as usize + x];
        assert!(
            (f64::from(actual) - expected.as_f64().unwrap()).abs() <= 0.02,
            "geometry sample {i} at {x},{y} changed: {actual} != {expected}"
        );
    }
}

#[test]
fn row_window_smoothing_matches_full_reference_with_clamped_edges() {
    let width = 5;
    let original = (0..width * width)
        .map(|i| if i % 4 == 0 { 0.0 } else { (i % 23) as f32 })
        .collect::<Vec<_>>();
    let mut actual = original.clone();
    smooth_field(&mut actual, width, &|| false).unwrap();
    for y in 0..width {
        for x in 0..width {
            let index = y * width + x;
            if original[index] == 0.0 {
                assert_eq!(actual[index], 0.0);
                continue;
            }
            let mut weighted = 0.0;
            for (dy, wy) in [(-1, 1.0), (0, 2.0), (1, 1.0)] {
                for (dx, wx) in [(-1, 1.0), (0, 2.0), (1, 1.0)] {
                    let yy = (y as isize + dy).clamp(0, width as isize - 1) as usize;
                    let xx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                    weighted += original[yy * width + xx] * wy * wx;
                }
            }
            let expected = (original[index] * 0.65 + weighted / 16.0 * 0.35).clamp(0.0, 23.0);
            assert!(
                (actual[index] - expected).abs() < 0.00001,
                "at {x},{y}: {} != {expected}",
                actual[index]
            );
        }
    }
}
