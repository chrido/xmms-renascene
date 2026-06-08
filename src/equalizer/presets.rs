use super::EqualizerPreset;

pub fn winamp_original_presets() -> Vec<EqualizerPreset> {
    vec![
        EqualizerPreset::new(
            "Classical",
            0.625,
            [
                0.625, 0.625, 0.625, 0.625, 0.625, 0.625, -7.5, -7.5, -7.5, -10.0,
            ],
        ),
        EqualizerPreset::new(
            "Club",
            0.625,
            [
                0.625, 0.625, 3.75, 6.25, 6.25, 6.25, 3.75, 0.625, 0.625, 0.625,
            ],
        ),
        EqualizerPreset::new(
            "Dance",
            0.625,
            [10.0, 7.5, 2.5, 0.0, 0.0, -6.25, -7.5, -7.5, 0.0, 0.0],
        ),
        EqualizerPreset::new("Flat", 0.625, [0.625; 10]),
        EqualizerPreset::new(
            "Laptop speakers/headphones",
            0.625,
            [
                5.0, 11.25, 5.625, -3.75, -2.5, 1.875, 5.0, 10.0, 13.125, 15.0,
            ],
        ),
        EqualizerPreset::new(
            "Large hall",
            0.625,
            [
                10.625, 10.625, 6.25, 6.25, 0.625, -5.0, -5.0, -5.0, 0.625, 0.625,
            ],
        ),
        EqualizerPreset::new(
            "Party",
            0.625,
            [7.5, 7.5, 0.625, 0.625, 0.625, 0.625, 0.625, 0.625, 7.5, 7.5],
        ),
        EqualizerPreset::new(
            "Pop",
            0.625,
            [
                -1.875, 5.0, 7.5, 8.125, 5.625, -1.25, -2.5, -2.5, -1.875, -1.875,
            ],
        ),
        EqualizerPreset::new(
            "Reggae",
            0.625,
            [
                0.625, 0.625, -0.625, -6.25, 0.625, 6.875, 6.875, 0.625, 0.625, 0.625,
            ],
        ),
        EqualizerPreset::new(
            "Rock",
            0.625,
            [
                8.125, 5.0, -5.625, -8.125, -3.75, 4.375, 9.375, 11.25, 11.25, 11.25,
            ],
        ),
        EqualizerPreset::new(
            "Soft",
            0.625,
            [
                5.0, 1.875, -1.25, -2.5, -1.25, 4.375, 8.75, 10.0, 11.25, 12.5,
            ],
        ),
        EqualizerPreset::new(
            "Ska",
            0.625,
            [
                -2.5, -5.0, -4.375, -0.625, 4.375, 6.25, 9.375, 10.0, 11.25, 10.0,
            ],
        ),
        EqualizerPreset::new(
            "Full Bass",
            0.625,
            [
                10.0, 10.0, 10.0, 6.25, 1.875, -4.375, -8.75, -10.625, -11.25, -11.25,
            ],
        ),
        EqualizerPreset::new(
            "Soft Rock",
            0.625,
            [
                4.375, 4.375, 2.5, -0.625, -4.375, -5.625, -3.75, -0.625, 3.125, 9.375,
            ],
        ),
        EqualizerPreset::new(
            "Full Treble",
            0.625,
            [
                -10.0, -10.0, -10.0, -4.375, 3.125, 11.25, 16.25, 16.25, 16.25, 17.5,
            ],
        ),
        EqualizerPreset::new(
            "Full Bass & Treble",
            0.625,
            [7.5, 6.25, 0.625, -7.5, -5.0, 1.875, 8.75, 11.25, 12.5, 12.5],
        ),
        EqualizerPreset::new(
            "Live",
            0.625,
            [
                -5.0, 0.625, 4.375, 5.625, 6.25, 6.25, 4.375, 3.125, 3.125, 2.5,
            ],
        ),
        EqualizerPreset::new(
            "Techno",
            0.625,
            [
                8.125, 6.25, 0.625, -5.625, -5.0, 0.625, 8.125, 10.0, 10.0, 9.375,
            ],
        ),
        EqualizerPreset::new("Preamp +12dB (Flat)", 20.0, [0.625; 10]),
        EqualizerPreset::new("Preamp -12dB (Flat)", -19.375, [0.625; 10]),
    ]
}
