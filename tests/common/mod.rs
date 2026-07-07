#![allow(dead_code)]

use std::fs;
use std::fs::File;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use xmms_renascene::e2e::{PlayerSettings, UiE2e};
use xmms_renascene::render::{MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH};

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn temp_dir(prefix: &str) -> TempDir {
    TempDir::new(prefix)
}

pub fn one_pixel_xpm(color: &str) -> String {
    format!(
        r#"/* XPM */
static char * main_xpm[] = {{
"1 1 1 1",
". c {color}",
"."}};
"#
    )
}

pub fn write_one_pixel_skin(dir: &Path, color: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("main.xpm"), one_pixel_xpm(color)).unwrap();
}

pub fn write_one_pixel_wsz(path: &Path, color: &str) {
    let file = File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    archive.start_file("base-2.9.1/main.xpm", options).unwrap();
    archive.write_all(one_pixel_xpm(color).as_bytes()).unwrap();
    archive.finish().unwrap();
}

pub fn write_solid_main_png_skin(dir: &Path, color: [u8; 3]) {
    fs::create_dir_all(dir).unwrap();
    let mut image = image::RgbaImage::new(MAIN_WINDOW_WIDTH as u32, MAIN_WINDOW_HEIGHT as u32);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([color[0], color[1], color[2], 0xff]);
    }
    image.save(dir.join("main.png")).unwrap();
}

pub fn file_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

pub fn app() -> UiE2e {
    UiE2e::start_player(PlayerSettings::default())
}

pub fn playlist_app() -> UiE2e {
    UiE2e::start_player(PlayerSettings::default().with_playlist_visible(true))
}

pub fn equalizer_app() -> UiE2e {
    UiE2e::start_player(PlayerSettings::default().with_equalizer_visible(true))
}

pub fn seed_playlist<'a, I>(app: &'a mut UiE2e, entries: I) -> &'a mut UiE2e
where
    I: IntoIterator<Item = (&'a str, &'a str, i64)>,
{
    for (uri, title, duration_ms) in entries {
        app.add_timed_entry(uri, title, duration_ms);
    }
    app
}
