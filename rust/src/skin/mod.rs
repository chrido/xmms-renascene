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
    pub fn load_bundled() -> io::Result<Self> {
        const BUNDLED_XPMS: &[(SkinPixmapKind, &str)] = &[
            (
                SkinPixmapKind::Main,
                include_str!("../../../data/defskin/main.xpm"),
            ),
            (
                SkinPixmapKind::CButtons,
                include_str!("../../../data/defskin/cbuttons.xpm"),
            ),
            (
                SkinPixmapKind::Titlebar,
                include_str!("../../../data/defskin/titlebar.xpm"),
            ),
            (
                SkinPixmapKind::ShufRep,
                include_str!("../../../data/defskin/shufrep.xpm"),
            ),
            (
                SkinPixmapKind::Text,
                include_str!("../../../data/defskin/text.xpm"),
            ),
            (
                SkinPixmapKind::Volume,
                include_str!("../../../data/defskin/volume.xpm"),
            ),
            (
                SkinPixmapKind::MonoStereo,
                include_str!("../../../data/defskin/monoster.xpm"),
            ),
            (
                SkinPixmapKind::PlayPause,
                include_str!("../../../data/defskin/playpaus.xpm"),
            ),
            (
                SkinPixmapKind::Numbers,
                include_str!("../../../data/defskin/nums_ex.xpm"),
            ),
            (
                SkinPixmapKind::PosBar,
                include_str!("../../../data/defskin/posbar.xpm"),
            ),
            (
                SkinPixmapKind::PlEdit,
                include_str!("../../../data/defskin/pledit.xpm"),
            ),
            (
                SkinPixmapKind::EqMain,
                include_str!("../../../data/defskin/eqmain.xpm"),
            ),
            (
                SkinPixmapKind::EqEx,
                include_str!("../../../data/defskin/eq_ex.xpm"),
            ),
        ];

        let mut pixmaps = BTreeMap::new();
        for (kind, contents) in BUNDLED_XPMS {
            let image = XpmImage::parse(contents).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("bundled {}.xpm: {err}", kind.info().file_stem),
                )
            })?;
            pixmaps.insert(*kind, image);
        }
        apply_balance_fallback(&mut pixmaps);
        Ok(Self { pixmaps })
    }

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

        apply_balance_fallback(&mut pixmaps);

        Ok(Self { pixmaps })
    }

    pub fn loaded_pixmap_count(&self) -> usize {
        self.pixmaps.len()
    }

    pub fn get(&self, kind: SkinPixmapKind) -> Option<&XpmImage> {
        self.pixmaps.get(&kind)
    }
}

fn apply_balance_fallback(pixmaps: &mut BTreeMap<SkinPixmapKind, XpmImage>) {
    if !pixmaps.contains_key(&SkinPixmapKind::Balance) {
        if let Some(volume) = pixmaps.get(&SkinPixmapKind::Volume).cloned() {
            pixmaps.insert(SkinPixmapKind::Balance, volume);
        }
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

    #[test]
    fn bundled_default_skin_loads_without_filesystem_lookup() {
        let skin = DefaultSkin::load_bundled().unwrap();
        assert_eq!(skin.loaded_pixmap_count(), SkinPixmapKind::ALL.len());
        assert_eq!(skin.get(SkinPixmapKind::Main).unwrap().width(), 275);
        assert!(skin.get(SkinPixmapKind::Balance).is_some());
    }
}
