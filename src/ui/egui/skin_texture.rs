//! Cairo/image to egui texture helpers.

pub fn cairo_argb_to_egui_rgba(argb: u32) -> [u8; 4] {
    let [b, g, r, a] = argb.to_ne_bytes();
    [r, g, b, a]
}
