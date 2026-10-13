//! Reproducible offline previews; PNG/filesystem work stays outside the renderer.
//!
//! Usage:
//! cargo run --locked --no-default-features --example artwork_preview -- OUTPUT_DIR [SKIN_PATH ...]
//! The output manifest records exact palettes, seeds and dimensions. A sampled
//! geometry fixture is emitted for explicit review, never updated by unit tests.

use std::error::Error;
use std::path::{Path, PathBuf};

use image::{imageops::FilterType, Rgb, RgbImage};
use xmms_renascene::artwork::{ArtworkKey, ARTWORK_RENDERER_VERSION};
use xmms_renascene::render::artwork::{color_artwork_field, render_artwork_field, ArtworkImage};
use xmms_renascene::skin::DefaultSkin;

fn rgb_image(image: &ArtworkImage) -> RgbImage {
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let pixel = image.pixels()[(y * image.width() + x) as usize];
        Rgb([(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])
    })
}

fn label(path: &Path, index: usize) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("skin");
    let safe = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    format!("skin-{index}-{safe}")
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testoutput/android-artwork/previews"));
    std::fs::create_dir_all(&output)?;
    let mut palettes = vec![(
        "bundled".to_owned(),
        *DefaultSkin::load_bundled()?.vis_colors(),
    )];
    for (index, path) in args.map(PathBuf::from).enumerate() {
        palettes.push((
            label(&path, index),
            *DefaultSkin::load_from_path(&path)?.vis_colors(),
        ));
    }
    let mut entries = Vec::new();
    for (skin, palette) in palettes {
        for track in ["/music/demo.wav", "/music/second.wav"] {
            let track_label = Path::new(track).file_stem().unwrap().to_str().unwrap();
            for size in [320, 512] {
                let key = ArtworkKey {
                    normalized_track_identity: track.into(),
                    palette,
                    size,
                    renderer_version: ARTWORK_RENDERER_VERSION,
                };
                let field = render_artwork_field(&key.spec())?;
                let rendered = color_artwork_field(&field, &palette)?;
                let name = format!("{skin}-{track_label}-{size}.png");
                let rgb = rgb_image(&rendered);
                rgb.save(output.join(&name))?;
                if size == 320 {
                    image::imageops::resize(&rgb, 160, 160, FilterType::Triangle)
                        .save(output.join(format!("{skin}-{track_label}-thumbnail.png")))?;
                }
                entries.push(serde_json::json!({ "file": name, "skin": skin, "track": track, "size": size, "seed": key.spec().stable_track_seed.to_string(), "palette": palette }));
            }
        }
    }
    let golden_key = ArtworkKey {
        normalized_track_identity: "/music/golden.wav".into(),
        palette: [[0; 3]; 24],
        size: 320,
        renderer_version: ARTWORK_RENDERER_VERSION,
    };
    let field = render_artwork_field(&golden_key.spec())?;
    let samples = (0..9)
        .flat_map(|y| {
            let field = &field;
            (0..9).map(move |x| {
                let xx = (x * 2 + 1) * 320 / 18;
                let yy = (y * 2 + 1) * 320 / 18;
                field.indices()[yy * 320 + xx]
            })
        })
        .collect::<Vec<_>>();
    let golden = serde_json::json!({ "renderer_version": ARTWORK_RENDERER_VERSION, "track": golden_key.normalized_track_identity, "size": 320, "grid": 9, "samples": samples });
    std::fs::write(
        output.join("field-v1.json"),
        serde_json::to_string_pretty(&golden)? + "\n",
    )?;
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_string_pretty(&entries)? + "\n",
    )?;
    println!("Wrote {} source previews, thumbnails, palette manifest and a candidate geometry fixture to {}", entries.len(), output.display());
    Ok(())
}
