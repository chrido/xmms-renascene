//! egui layout helpers.

pub fn scaled_size(width: i32, height: i32, scale: f32) -> (f32, f32) {
    (width as f32 * scale, height as f32 * scale)
}

pub fn clamp_popup_to_rect(
    anchor: egui::Pos2,
    bounds: egui::Rect,
    popup_size: egui::Vec2,
) -> egui::Pos2 {
    let max_x = (bounds.right() - popup_size.x).max(bounds.left());
    let max_y = (bounds.bottom() - popup_size.y).max(bounds.top());
    egui::pos2(
        anchor.x.min(max_x).max(bounds.left()),
        anchor.y.min(max_y).max(bounds.top()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_popup_inside_bounds_when_anchor_overflows_right_or_bottom() {
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(275.0, 232.0));
        let pos = clamp_popup_to_rect(egui::pos2(250.0, 220.0), bounds, egui::vec2(200.0, 80.0));
        assert_eq!(pos, egui::pos2(75.0, 152.0));
    }

    #[test]
    fn keeps_popup_at_anchor_when_it_already_fits() {
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(275.0, 232.0));
        let pos = clamp_popup_to_rect(egui::pos2(10.0, 20.0), bounds, egui::vec2(100.0, 80.0));
        assert_eq!(pos, egui::pos2(10.0, 20.0));
    }
}
