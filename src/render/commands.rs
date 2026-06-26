//! Frontend-neutral rendering strategy and draw-command model.
//!
//! The current GTK frontend continues to use the Cairo renderer for pixel
//! parity. Future frontends can either display Cairo-rendered bitmaps or consume
//! draw commands if/when the renderer is migrated.

use crate::skin::layout::SkinRect;
use crate::skin::SkinPixmapKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderingStrategy {
    CairoBitmapFirst,
    DrawCommandsLater,
}

pub const MOBILE_RENDERING_STRATEGY: RenderingStrategy = RenderingStrategy::CairoBitmapFirst;

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
    fn mobile_rendering_strategy_starts_with_cairo_bitmap_parity() {
        assert_eq!(selected_rendering_strategy(), RenderingStrategy::CairoBitmapFirst);
    }
}
