//! egui layout helpers.

pub fn scaled_size(width: i32, height: i32, scale: f32) -> (f32, f32) {
    (width as f32 * scale, height as f32 * scale)
}
