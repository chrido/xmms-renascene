//! Frontend-neutral rendering strategy and draw-command model.
//!
//! Frontends display pure Rust software-rendered bitmaps today and can consume
//! draw commands directly in the future if/when that becomes useful.

use crate::skin::layout::SkinRect;
use crate::skin::SkinPixmapKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderingStrategy {
    SoftwareBitmapFirst,
    DrawCommandsLater,
}

pub const MOBILE_RENDERING_STRATEGY: RenderingStrategy = RenderingStrategy::SoftwareBitmapFirst;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCommand {
    Blit {
        pixmap: SkinPixmapKind,
        source: SkinRect,
        dest: SkinRect,
    },
    Text {
        text: String,
        position: (i32, i32),
        color_rgb: [u8; 3],
    },
    FillRect {
        rect: SkinRect,
        color_rgb: [u8; 3],
    },
}

pub fn selected_rendering_strategy() -> RenderingStrategy {
    MOBILE_RENDERING_STRATEGY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_rendering_strategy_starts_with_software_bitmap_parity() {
        assert_eq!(
            selected_rendering_strategy(),
            RenderingStrategy::SoftwareBitmapFirst
        );
    }
}
