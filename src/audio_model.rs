pub const SPECTRUM_BANDS: usize = 75;
pub const ANALYZER_FFT_FRAMES: usize = 512;
pub const ANALYZER_BAR_COUNT: usize = 19;
pub const ANALYZER_FREQUENCY_BIN_COUNT: usize = 184;

const ANALYZER_LINE_TAIL: [usize; 17] = [
    61, 66, 71, 76, 81, 87, 93, 100, 107, 114, 122, 131, 140, 150, 161, 172, 184,
];
const ANALYZER_BAR_BOUNDARIES: [usize; ANALYZER_BAR_COUNT + 1] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 15, 20, 27, 36, 47, 62, 82, 107, 141, 184,
];
pub const EQUALIZER_BANDS: usize = 10;

pub type SpectrumData = [f32; SPECTRUM_BANDS];
pub type EqualizerBandPositions = [i32; EQUALIZER_BANDS];
pub type EqualizerBandDb = [f64; EQUALIZER_BANDS];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectrumLayout {
    Lines,
    AnalyzerBars,
}

pub(crate) fn analyzer_spectrum_level(amplitude: f32) -> f32 {
    let scaled = (amplitude.max(0.0) * 256.0).floor();
    if scaled < 1.0 {
        return 0.0;
    }
    ((scaled.ln() * (20.0 / 256.0_f32.ln())).floor().min(15.0) / 16.0).max(0.0)
}

pub(crate) fn analyzer_spectrum_from_bins(
    frequency_bins: &[f32],
) -> (SpectrumData, [f32; ANALYZER_BAR_COUNT]) {
    assert!(frequency_bins.len() >= ANALYZER_FREQUENCY_BIN_COUNT);
    let line_boundary = |index: usize| {
        if index <= 58 {
            index
        } else {
            ANALYZER_LINE_TAIL[index - 59]
        }
    };
    let lines = std::array::from_fn(|band| {
        frequency_bins[line_boundary(band)..line_boundary(band + 1)]
            .iter()
            .copied()
            .fold(0.0, f32::max)
    });
    let bars = std::array::from_fn(|bar| {
        frequency_bins[ANALYZER_BAR_BOUNDARIES[bar]..ANALYZER_BAR_BOUNDARIES[bar + 1]]
            .iter()
            .copied()
            .fold(0.0, f32::max)
    });
    (lines, bars)
}

pub(crate) fn spectrum_data_for_layout(
    lines: SpectrumData,
    bars: [f32; ANALYZER_BAR_COUNT],
    layout: SpectrumLayout,
) -> SpectrumData {
    match layout {
        SpectrumLayout::Lines => lines,
        SpectrumLayout::AnalyzerBars => {
            let mut data = [0.0; SPECTRUM_BANDS];
            data[..ANALYZER_BAR_COUNT].copy_from_slice(&bars);
            data
        }
    }
}

pub fn equalizer_position_to_db(position: i32) -> f64 {
    (50 - position.clamp(0, 100)) as f64 * 20.0 / 50.0
}

pub fn db_to_equalizer_position(db: f64) -> i32 {
    (50.0 - (db.clamp(-20.0, 20.0) * 50.0 / 20.0))
        .round()
        .clamp(0.0, 100.0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzer_boundaries_match_reference_version() {
        assert_eq!(
            ANALYZER_BAR_BOUNDARIES,
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 15, 20, 27, 36, 47, 62, 82, 107, 141, 184,]
        );
        assert_eq!(
            ANALYZER_LINE_TAIL,
            [61, 66, 71, 76, 81, 87, 93, 100, 107, 114, 122, 131, 140, 150, 161, 172, 184,]
        );
    }

    #[test]
    fn analyzer_ignores_reference_unused_trailing_bucket() {
        let mut bins = [0.0; 256];
        bins[183] = 0.5;
        bins[184] = 1.0;
        let (lines, bars) = analyzer_spectrum_from_bins(&bins);
        assert_eq!(lines[74], 0.5);
        assert_eq!(bars[18], 0.5);
        assert!(!lines.contains(&1.0));
        assert!(!bars.contains(&1.0));
    }
}
