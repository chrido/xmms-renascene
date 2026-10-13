use super::*;
use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "xmms-artwork-palette-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_archive(path: &Path, files: &[(&str, &[u8])]) {
    let format = archive::classify_path(path).unwrap().format;
    if format == archive::ArchiveFormat::Zip {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    } else {
        let mut tar = tar::Builder::new(Vec::new());
        for (name, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, Cursor::new(bytes))
                .unwrap();
        }
        let raw = tar.into_inner().unwrap();
        match format {
            archive::ArchiveFormat::Tar => fs::write(path, raw).unwrap(),
            archive::ArchiveFormat::TarGz => {
                let mut writer = flate2::write::GzEncoder::new(
                    File::create(path).unwrap(),
                    flate2::Compression::default(),
                );
                writer.write_all(&raw).unwrap();
                writer.finish().unwrap();
            }
            archive::ArchiveFormat::TarBz2 => {
                let mut writer = bzip2::write::BzEncoder::new(
                    File::create(path).unwrap(),
                    bzip2::Compression::default(),
                );
                writer.write_all(&raw).unwrap();
                writer.finish().unwrap();
            }
            archive::ArchiveFormat::Zip => unreachable!(),
        }
    }
}

#[test]
fn directory_palette_matches_full_loader_for_missing_partial_and_malformed_text() {
    let dir = TestDir::new();
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&dir.0).unwrap(),
        *DefaultSkin::load_from_path(&dir.0).unwrap().vis_colors()
    );
    let complete = (0..24)
        .map(|i| format!("{i},{},{}\n", i + 1, i + 2))
        .collect::<String>();
    for bytes in [
        b"".as_slice(),
        b"1,2,3\n",
        b"not-a-color\n-8,300,12\n1 2 3\n",
        complete.as_bytes(),
    ] {
        fs::write(dir.0.join("VISCOLOR.TXT"), bytes).unwrap();
        assert_eq!(
            DefaultSkin::load_vis_colors_from_path(&dir.0).unwrap(),
            *DefaultSkin::load_from_path(&dir.0).unwrap().vis_colors()
        );
    }
}

#[test]
fn bounded_directory_lookup_keeps_root_before_nested_and_dfs_precedence() {
    let dir = TestDir::new();
    for child in ["first/deep", "second"] {
        fs::create_dir_all(dir.0.join(child)).unwrap();
    }
    fs::write(dir.0.join("first/deep/viscolor.txt"), b"4,5,6\n").unwrap();
    fs::write(dir.0.join("second/VisColor.txt"), b"7,8,9\n").unwrap();
    let expected = *DefaultSkin::load_from_path(&dir.0).unwrap().vis_colors();
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&dir.0).unwrap(),
        expected
    );
    fs::write(dir.0.join("VISCOLOR.txt"), b"1,2,3\n").unwrap();
    let palette = DefaultSkin::load_vis_colors_from_path(&dir.0).unwrap();
    assert_eq!(palette[0], [1, 2, 3]);
    assert_eq!(
        palette,
        *DefaultSkin::load_from_path(&dir.0).unwrap().vis_colors()
    );
}

#[test]
fn all_supported_archive_aliases_match_full_loader_and_first_member_precedence() {
    let dir = TestDir::new();
    for extension in ["wsz", "ZIP", "tar", "TAR.GZ", "tgz", "tar.bz2", "tbz2"] {
        let path = dir.0.join(format!("skin.{extension}"));
        write_archive(
            &path,
            &[
                ("Nested/VISCOLOR.TXT", b"1,2,3\ninvalid\n4 5 6\n"),
                ("viscolor.txt", b"9,8,7\n"),
            ],
        );
        let palette = DefaultSkin::load_vis_colors_from_path(&path).unwrap();
        assert_eq!(palette[0], [1, 2, 3]);
        assert_eq!(palette[1], DEFAULT_VIS_COLORS[1]);
        assert_eq!(palette[2], [4, 5, 6]);
        assert_eq!(
            palette,
            *DefaultSkin::load_from_path(&path).unwrap().vis_colors()
        );
        write_archive(&path, &[("ignored.bin", b"stuff")]);
        assert_eq!(
            DefaultSkin::load_vis_colors_from_path(&path).unwrap(),
            DEFAULT_VIS_COLORS
        );
    }
}

#[test]
fn palette_loader_never_decodes_directory_or_archive_bitmaps() {
    let dir = TestDir::new();
    fs::write(dir.0.join("main.png"), b"deliberately corrupt image").unwrap();
    fs::write(dir.0.join("viscolor.txt"), b"1,2,3\n").unwrap();
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&dir.0).unwrap()[0],
        [1, 2, 3]
    );
    assert!(DefaultSkin::load_from_path(&dir.0).is_err());
    let path = dir.0.join("skin.wsz");
    write_archive(
        &path,
        &[
            ("Main.png", b"deliberately corrupt image"),
            ("viscolor.txt", b"1,2,3\n"),
        ],
    );
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&path).unwrap()[0],
        [1, 2, 3]
    );
    assert!(DefaultSkin::load_from_path(&path).is_err());
}

#[test]
fn palette_text_limits_and_invalid_utf8_are_enforced_in_both_sources() {
    let dir = TestDir::new();
    let mut bytes = vec![b'\n'; MAX_VIS_COLOR_TEXT_BYTES as usize];
    let text = dir.0.join("viscolor.txt");
    fs::write(&text, &bytes).unwrap();
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&dir.0).unwrap(),
        DEFAULT_VIS_COLORS
    );
    let archive = dir.0.join("skin.wsz");
    write_archive(&archive, &[("viscolor.txt", &bytes)]);
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&archive).unwrap(),
        DEFAULT_VIS_COLORS
    );
    bytes.push(b'\n');
    fs::write(&text, &bytes).unwrap();
    write_archive(&archive, &[("viscolor.txt", &bytes)]);
    for path in [&dir.0, &archive] {
        assert_eq!(
            DefaultSkin::load_vis_colors_from_path(path)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
    fs::write(&text, [0xff]).unwrap();
    write_archive(&archive, &[("viscolor.txt", &[0xff])]);
    for path in [&dir.0, &archive] {
        assert_eq!(
            DefaultSkin::load_vis_colors_from_path(path)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            DefaultSkin::load_from_path(path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn directory_count_and_depth_limits_fail_before_unbounded_traversal() {
    let dir = TestDir::new();
    for i in 0..MAX_PALETTE_DIRECTORY_ENTRIES {
        fs::write(dir.0.join(format!("ignored-{i}")), b"").unwrap();
    }
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&dir.0).unwrap(),
        DEFAULT_VIS_COLORS
    );
    fs::write(dir.0.join("one-too-many"), b"").unwrap();
    assert!(DefaultSkin::load_vis_colors_from_path(&dir.0)
        .unwrap_err()
        .to_string()
        .contains("entry count"));
    let deep = TestDir::new();
    let mut path = deep.0.clone();
    for _ in 0..MAX_PALETTE_DIRECTORY_DEPTH {
        path = path.join("child");
        fs::create_dir(&path).unwrap();
    }
    fs::write(path.join("viscolor.txt"), b"1,2,3\n").unwrap();
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&deep.0).unwrap()[0],
        [1, 2, 3]
    );
    fs::create_dir(path.join("too-deep")).unwrap();
    assert!(DefaultSkin::load_vis_colors_from_path(&deep.0)
        .unwrap_err()
        .to_string()
        .contains("depth"));
}

#[test]
fn archive_input_preflight_and_missing_or_corrupt_sources_return_errors() {
    let dir = TestDir::new();
    let path = dir.0.join("oversized.wsz");
    File::create(&path)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(DefaultSkin::load_vis_colors_from_path(&path)
        .unwrap_err()
        .to_string()
        .contains("input exceeds"));
    fs::write(&path, b"not a ZIP").unwrap();
    assert!(DefaultSkin::load_vis_colors_from_path(&path).is_err());
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&dir.0.join("missing"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
}

#[cfg(unix)]
#[test]
fn directory_palette_lookup_ignores_symlinks_like_full_loader() {
    let dir = TestDir::new();
    fs::write(dir.0.join("other.txt"), b"1,2,3\n").unwrap();
    std::os::unix::fs::symlink(dir.0.join("other.txt"), dir.0.join("viscolor.txt")).unwrap();
    std::os::unix::fs::symlink(&dir.0, dir.0.join("loop")).unwrap();
    assert_eq!(
        DefaultSkin::load_vis_colors_from_path(&dir.0).unwrap(),
        DEFAULT_VIS_COLORS
    );
    assert_eq!(
        *DefaultSkin::load_from_path(&dir.0).unwrap().vis_colors(),
        DEFAULT_VIS_COLORS
    );
}
