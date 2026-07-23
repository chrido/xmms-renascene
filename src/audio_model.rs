pub const SPECTRUM_BANDS: usize = 75;
pub const EQUALIZER_BANDS: usize = 10;

pub type SpectrumData = [f32; SPECTRUM_BANDS];
pub type EqualizerBandPositions = [i32; EQUALIZER_BANDS];
pub type EqualizerBandDb = [f64; EQUALIZER_BANDS];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectrumLayout {
    Lines,
    XmmsBars,
}

pub fn equalizer_position_to_db(position: i32) -> f64 {
    (50 - position.clamp(0, 100)) as f64 * 20.0 / 50.0
}

pub fn db_to_equalizer_position(db: f64) -> i32 {
    (50.0 - (db.clamp(-20.0, 20.0) * 50.0 / 20.0))
        .round()
        .clamp(0.0, 100.0) as i32
}
