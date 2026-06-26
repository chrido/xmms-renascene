//! eframe application lifecycle for the egui frontend.

#[derive(Debug, Default)]
pub struct EguiFrontendState {
    pub preferences_open: bool,
    pub scale_factor: f32,
}
