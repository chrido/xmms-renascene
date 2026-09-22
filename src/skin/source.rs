//! Indexed skin asset lookup. Directory images prefer root files, then nested
//! files, with bmp/png/xpm priority at each level. Archive images prefer the
//! extension first, then original entry order. Named text uses directory DFS
//! (files before children) or archive entry order.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::SkinPixmapKind;

pub(super) enum AssetSource<'a> {
    Directory(DirectoryInventory),
    Archive(ArchiveInventory<'a>),
}

pub(super) struct DirectoryInventory {
    root: FileIndex<PathBuf>,
    nested: FileIndex<PathBuf>,
}

pub(super) struct ArchiveInventory<'a> {
    path: &'a Path,
    entries: &'a [(String, Vec<u8>)],
    files: FileIndex<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ImageExtension {
    Bmp,
    Png,
    Xpm,
}

impl ImageExtension {
    const PRIORITY: [Self; 3] = [Self::Bmp, Self::Png, Self::Xpm];

    fn from_str(extension: &str) -> Option<Self> {
        if extension.eq_ignore_ascii_case("bmp") {
            Some(Self::Bmp)
        } else if extension.eq_ignore_ascii_case("png") {
            Some(Self::Png)
        } else if extension.eq_ignore_ascii_case("xpm") {
            Some(Self::Xpm)
        } else {
            None
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
struct ImageKey {
    stem: String,
    extension: ImageExtension,
}

impl ImageKey {
    fn from_file_name(file_name: &str) -> Option<Self> {
        let path = Path::new(file_name);
        Some(Self {
            stem: path.file_stem()?.to_str()?.to_ascii_lowercase(),
            extension: ImageExtension::from_str(path.extension()?.to_str()?)?,
        })
    }
}

/// A supported archive asset's case-folded basename, including its extension.
/// This shares image extension parsing with the directory/archive indexes.
pub(super) fn archive_asset_key(name: &str) -> Option<String> {
    let file_name = Path::new(name).file_name()?.to_str()?;
    let is_image = ImageKey::from_file_name(file_name).is_some_and(|key| {
        key.stem == "numbers"
            || SkinPixmapKind::ALL
                .iter()
                .any(|kind| key.stem == kind.info().file_stem)
    });
    let is_text = ["viscolor.txt", "pledit.txt", "region.txt"]
        .iter()
        .any(|text| file_name.eq_ignore_ascii_case(text));
    (is_image || is_text).then(|| file_name.to_ascii_lowercase())
}

struct FileIndex<T> {
    images: HashMap<ImageKey, T>,
    named: HashMap<String, T>,
}

impl<T> Default for FileIndex<T> {
    fn default() -> Self {
        Self {
            images: HashMap::new(),
            named: HashMap::new(),
        }
    }
}

impl<T: Clone> FileIndex<T> {
    fn insert_first(&mut self, file_name: &str, value: T) {
        if let Some(key) = ImageKey::from_file_name(file_name) {
            self.images.entry(key).or_insert_with(|| value.clone());
        }
        self.named
            .entry(file_name.to_ascii_lowercase())
            .or_insert(value);
    }
}

impl<T> FileIndex<T> {
    fn image(&self, stem: &str, extension: ImageExtension) -> Option<&T> {
        self.images.get(&ImageKey {
            stem: stem.to_ascii_lowercase(),
            extension,
        })
    }

    fn named(&self, name: &str) -> Option<&T> {
        self.named.get(&name.to_ascii_lowercase())
    }
}

pub(super) struct Asset<'a> {
    pub path: PathBuf,
    pub label: String,
    pub contents: Cow<'a, [u8]>,
}

impl<'a> AssetSource<'a> {
    pub fn directory(dir: &Path) -> io::Result<Self> {
        let mut inventory = DirectoryInventory {
            root: FileIndex::default(),
            nested: FileIndex::default(),
        };
        visit_directory(dir, &mut inventory, true)?;
        Ok(Self::Directory(inventory))
    }

    pub fn archive(path: &'a Path, entries: &'a [(String, Vec<u8>)]) -> Self {
        let mut files = FileIndex::default();
        for (index, (name, _)) in entries.iter().enumerate() {
            if let Some(file_name) = Path::new(name).file_name().and_then(|name| name.to_str()) {
                files.insert_first(file_name, index);
            }
        }
        Self::Archive(ArchiveInventory {
            path,
            entries,
            files,
        })
    }

    pub fn image(&self, stem: &str) -> io::Result<Option<Asset<'_>>> {
        match self {
            Self::Directory(inventory) => {
                inventory.image(stem).map(read_directory_asset).transpose()
            }
            Self::Archive(inventory) => {
                Ok(inventory.image(stem).map(|index| inventory.asset(index)))
            }
        }
    }

    pub fn text(&self, name: &str) -> io::Result<Option<String>> {
        let asset = match self {
            Self::Directory(inventory) => inventory
                .root
                .named(name)
                .or_else(|| inventory.nested.named(name))
                .map(|path| read_directory_asset(path))
                .transpose()?,
            Self::Archive(inventory) => inventory
                .files
                .named(name)
                .map(|index| inventory.asset(*index)),
        };
        asset
            .map(|asset| {
                std::str::from_utf8(&asset.contents)
                    .map(str::to_owned)
                    .map_err(|err| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("decode text asset {}: {err}", asset.label),
                        )
                    })
            })
            .transpose()
    }
}

impl DirectoryInventory {
    fn image(&self, stem: &str) -> Option<&Path> {
        // Root beats nested files, even if the nested file has a preferred extension.
        for files in [&self.root, &self.nested] {
            for extension in ImageExtension::PRIORITY {
                if let Some(path) = files.image(stem, extension) {
                    return Some(path);
                }
            }
        }
        None
    }
}

impl ArchiveInventory<'_> {
    fn image(&self, stem: &str) -> Option<usize> {
        for extension in ImageExtension::PRIORITY {
            if let Some(index) = self.files.image(stem, extension) {
                return Some(*index);
            }
        }
        None
    }

    fn asset(&self, index: usize) -> Asset<'_> {
        let (name, contents) = &self.entries[index];
        Asset {
            path: PathBuf::from(name),
            label: format!("{}:{name}", self.path.display()),
            contents: Cow::Borrowed(contents),
        }
    }
}

fn with_path(err: io::Error, operation: &str, path: &Path) -> io::Error {
    io::Error::new(err.kind(), format!("{operation} {}: {err}", path.display()))
}

fn read_directory_asset(path: &Path) -> io::Result<Asset<'static>> {
    let contents = fs::read(path).map_err(|err| with_path(err, "read skin asset", path))?;
    Ok(Asset {
        label: path.display().to_string(),
        path: path.to_path_buf(),
        contents: Cow::Owned(contents),
    })
}

fn visit_directory(
    dir: &Path,
    inventory: &mut DirectoryInventory,
    is_root: bool,
) -> io::Result<()> {
    // Retain read_dir order: examine every file in this directory before
    // descending into its children, without sorting either group.
    let entries = fs::read_dir(dir).map_err(|err| with_path(err, "read_dir", dir))?;
    let mut children = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| with_path(err, "read_dir entry in", dir))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| with_path(err, "file_type", &path))?;
        if file_type.is_file() {
            let name = entry.file_name();
            if let Some(name) = name.to_str() {
                let files = if is_root {
                    &mut inventory.root
                } else {
                    &mut inventory.nested
                };
                files.insert_first(name, path);
            }
        } else if file_type.is_dir() {
            children.push(path);
        }
    }
    for child in children {
        visit_directory(&child, inventory, false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "xmms-skin-inventory-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn directory_inventory_is_a_snapshot_of_names_not_a_rescan() {
        let dir = test_dir();
        fs::write(dir.join("Main.png"), b"original").unwrap();
        let source = AssetSource::directory(&dir).unwrap();

        // A later preferred file or text asset is invisible to this inventory.
        fs::write(dir.join("Main.bmp"), b"new bmp").unwrap();
        fs::write(dir.join("viscolor.txt"), b"1,2,3\n").unwrap();
        assert_eq!(
            source.image("MAIN").unwrap().unwrap().contents.as_ref(),
            b"original"
        );
        assert_eq!(
            source.image("main").unwrap().unwrap().path,
            dir.join("Main.png")
        );
        assert_eq!(source.text("viscolor.txt").unwrap(), None);

        // Payloads themselves are still read lazily from the indexed path.
        fs::write(dir.join("Main.png"), b"updated").unwrap();
        assert_eq!(
            source.image("main").unwrap().unwrap().contents.as_ref(),
            b"updated"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removed_or_replaced_indexed_files_report_read_errors() {
        let dir = test_dir();
        let image = dir.join("Main.png");
        let text = dir.join("viscolor.txt");
        fs::write(&image, b"image").unwrap();
        fs::write(&text, b"1,2,3\n").unwrap();
        let source = AssetSource::directory(&dir).unwrap();

        fs::remove_file(&image).unwrap();
        let err = source
            .image("main")
            .err()
            .expect("removed image should fail");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err
            .to_string()
            .contains(&format!("read skin asset {}", image.display())));

        fs::create_dir(&image).unwrap();
        let err = source
            .image("main")
            .err()
            .expect("replacement directory should fail");
        assert!(err
            .to_string()
            .contains(&format!("read skin asset {}", image.display())));

        fs::remove_file(&text).unwrap();
        let err = source.text("viscolor.txt").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err
            .to_string()
            .contains(&format!("read skin asset {}", text.display())));
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_nested_directory_fails_when_permissions_are_enforced() {
        use std::os::unix::fs::PermissionsExt;

        let dir = test_dir();
        let nested = dir.join("Nested");
        fs::create_dir(&nested).unwrap();
        fs::write(dir.join("Main.xpm"), b"present").unwrap();
        let original = fs::metadata(&nested).unwrap().permissions();
        fs::set_permissions(&nested, fs::Permissions::from_mode(0o000)).unwrap();
        let denied = fs::read_dir(&nested).err();
        let result = AssetSource::directory(&dir);
        fs::set_permissions(&nested, original).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        if let Some(expected) = denied {
            let err = result
                .err()
                .expect("nested read_dir failure must propagate");
            assert_eq!(err.kind(), expected.kind());
            assert!(err
                .to_string()
                .contains(&format!("read_dir {}", nested.display())));
        } else {
            eprintln!("skipping nested permission assertion: read_dir succeeds despite mode 000");
        }
    }
}
