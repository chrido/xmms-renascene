pub mod widget;
pub mod xpm;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use xpm::XpmImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkinPixmapKind {
    Main,
    CButtons,
    Titlebar,
    ShufRep,
    Text,
    Volume,
    Balance,
    MonoStereo,
    PlayPause,
    Numbers,
    PosBar,
    PlEdit,
    EqMain,
    EqEx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinPixmapInfo {
    pub file_stem: &'static str,
    pub width: usize,
    pub height: usize,
}

impl SkinPixmapKind {
    pub const ALL: [SkinPixmapKind; 14] = [
        SkinPixmapKind::Main,
        SkinPixmapKind::CButtons,
        SkinPixmapKind::Titlebar,
        SkinPixmapKind::ShufRep,
        SkinPixmapKind::Text,
        SkinPixmapKind::Volume,
        SkinPixmapKind::Balance,
        SkinPixmapKind::MonoStereo,
        SkinPixmapKind::PlayPause,
        SkinPixmapKind::Numbers,
        SkinPixmapKind::PosBar,
        SkinPixmapKind::PlEdit,
        SkinPixmapKind::EqMain,
        SkinPixmapKind::EqEx,
    ];

    pub fn info(self) -> SkinPixmapInfo {
        match self {
            SkinPixmapKind::Main => SkinPixmapInfo {
                file_stem: "main",
                width: 275,
                height: 116,
            },
            SkinPixmapKind::CButtons => SkinPixmapInfo {
                file_stem: "cbuttons",
                width: 136,
                height: 36,
            },
            SkinPixmapKind::Titlebar => SkinPixmapInfo {
                file_stem: "titlebar",
                width: 275,
                height: 116,
            },
            SkinPixmapKind::ShufRep => SkinPixmapInfo {
                file_stem: "shufrep",
                width: 28,
                height: 60,
            },
            SkinPixmapKind::Text => SkinPixmapInfo {
                file_stem: "text",
                width: 155,
                height: 18,
            },
            SkinPixmapKind::Volume => SkinPixmapInfo {
                file_stem: "volume",
                width: 68,
                height: 421,
            },
            SkinPixmapKind::Balance => SkinPixmapInfo {
                file_stem: "balance",
                width: 38,
                height: 421,
            },
            SkinPixmapKind::MonoStereo => SkinPixmapInfo {
                file_stem: "monoster",
                width: 56,
                height: 12,
            },
            SkinPixmapKind::PlayPause => SkinPixmapInfo {
                file_stem: "playpaus",
                width: 11,
                height: 9,
            },
            SkinPixmapKind::Numbers => SkinPixmapInfo {
                file_stem: "nums_ex",
                width: 108,
                height: 13,
            },
            SkinPixmapKind::PosBar => SkinPixmapInfo {
                file_stem: "posbar",
                width: 248,
                height: 10,
            },
            SkinPixmapKind::PlEdit => SkinPixmapInfo {
                file_stem: "pledit",
                width: 150,
                height: 18,
            },
            SkinPixmapKind::EqMain => SkinPixmapInfo {
                file_stem: "eqmain",
                width: 275,
                height: 116,
            },
            SkinPixmapKind::EqEx => SkinPixmapInfo {
                file_stem: "eq_ex",
                width: 275,
                height: 50,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultSkin {
    pixmaps: BTreeMap<SkinPixmapKind, XpmImage>,
}

impl DefaultSkin {
    pub fn load_from_dir(dir: &Path) -> io::Result<Self> {
        let mut pixmaps = BTreeMap::new();

        for kind in SkinPixmapKind::ALL {
            if kind == SkinPixmapKind::Balance {
                continue;
            }

            let info = kind.info();
            let path = dir.join(format!("{}.xpm", info.file_stem));
            if !path.exists() {
                continue;
            }

            let contents = fs::read_to_string(&path)?;
            let image = XpmImage::parse(&contents).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: {err}", path.display()),
                )
            })?;
            pixmaps.insert(kind, image);
        }

        if !pixmaps.contains_key(&SkinPixmapKind::Balance) {
            if let Some(volume) = pixmaps.get(&SkinPixmapKind::Volume).cloned() {
                pixmaps.insert(SkinPixmapKind::Balance, volume);
            }
        }

        Ok(Self { pixmaps })
    }

    pub fn loaded_pixmap_count(&self) -> usize {
        self.pixmaps.len()
    }

    pub fn get(&self, kind: SkinPixmapKind) -> Option<&XpmImage> {
        self.pixmaps.get(&kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixmap_info_matches_original_xmms_dimensions() {
        assert_eq!(SkinPixmapKind::Main.info().width, 275);
        assert_eq!(SkinPixmapKind::EqEx.info().height, 50);
        assert_eq!(SkinPixmapKind::Numbers.info().file_stem, "nums_ex");
    }
}
