use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::audio_model::{
    db_to_equalizer_position, equalizer_position_to_db, EqualizerBandDb, EqualizerBandPositions,
    EQUALIZER_BANDS,
};

mod presets;

pub use presets::winamp_original_presets;

pub fn built_in_equalizer_presets() -> Vec<EqualizerPreset> {
    let mut presets = vec![EqualizerPreset::zero("Default")];
    presets.extend(winamp_original_presets());
    presets
}

#[derive(Debug, Clone, PartialEq)]
pub struct EqualizerPreset {
    pub name: String,
    pub preamp: f64,
    pub bands: EqualizerBandDb,
}

impl EqualizerPreset {
    pub fn new(name: impl Into<String>, preamp: f64, bands: EqualizerBandDb) -> Self {
        Self {
            name: name.into(),
            preamp,
            bands,
        }
    }

    pub fn zero(name: impl Into<String>) -> Self {
        Self::new(name, 0.0, [0.0; EQUALIZER_BANDS])
    }

    pub fn from_positions(
        name: impl Into<String>,
        preamp_position: i32,
        band_positions: EqualizerBandPositions,
    ) -> Self {
        Self::new(
            name,
            equalizer_position_to_db(preamp_position),
            band_positions.map(equalizer_position_to_db),
        )
    }

    pub fn preamp_position(&self) -> i32 {
        db_to_equalizer_position(self.preamp)
    }

    pub fn band_positions(&self) -> EqualizerBandPositions {
        self.bands.map(db_to_equalizer_position)
    }
}

pub fn default_preset_file() -> &'static str {
    "dir_default.preset"
}

pub fn default_preset_extension() -> &'static str {
    "preset"
}

pub fn default_equalizer_presets() -> Vec<EqualizerPreset> {
    vec![EqualizerPreset::zero("Default")]
}

pub fn preset_store_path(config_dir: &Path, file_name: &str) -> PathBuf {
    config_dir.join(file_name)
}

pub fn load_preset_store(path: &Path) -> io::Result<Vec<EqualizerPreset>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(parse_preset_store(&contents)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

pub fn save_preset_store(path: &Path, presets: &[EqualizerPreset]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serialize_preset_store(presets))
}

pub fn load_xmms_preset_file(path: &Path) -> io::Result<Option<EqualizerPreset>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(parse_xmms_preset_file(&contents)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn save_xmms_preset_file(path: &Path, preset: &EqualizerPreset) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serialize_xmms_preset_file(preset))
}

pub fn load_winamp_eqf_first(path: &Path) -> io::Result<Option<EqualizerPreset>> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(parse_winamp_eqf(&bytes).into_iter().next())
}

pub fn import_winamp_eqf(path: &Path) -> io::Result<Vec<EqualizerPreset>> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(parse_winamp_eqf(&bytes))
}

pub fn save_winamp_eqf(path: &Path, preset: &EqualizerPreset) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(&serialize_winamp_eqf(preset))
}

pub fn upsert_preset(presets: &mut Vec<EqualizerPreset>, preset: EqualizerPreset) {
    if let Some(existing) = presets
        .iter_mut()
        .find(|existing| existing.name.eq_ignore_ascii_case(&preset.name))
    {
        *existing = preset;
    } else {
        presets.push(preset);
    }
}

pub fn remove_presets(presets: &mut Vec<EqualizerPreset>, names: &[String]) {
    presets.retain(|preset| {
        !names
            .iter()
            .any(|name| preset.name.eq_ignore_ascii_case(name))
    });
}

pub fn find_preset<'a>(presets: &'a [EqualizerPreset], name: &str) -> Option<&'a EqualizerPreset> {
    presets
        .iter()
        .find(|preset| preset.name.eq_ignore_ascii_case(name))
}

pub fn sort_presets(presets: &mut [EqualizerPreset]) {
    presets.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
}

pub fn parse_preset_store(contents: &str) -> Vec<EqualizerPreset> {
    let mut presets = Vec::new();
    for (_, name) in sorted_numbered_entries(contents, "Presets", "Preset") {
        if let Some(preset) = parse_named_preset(contents, &name) {
            presets.push(preset);
        }
    }
    presets
}

pub fn serialize_preset_store(presets: &[EqualizerPreset]) -> String {
    let mut out = String::from("[Presets]\n");
    for (index, preset) in presets.iter().enumerate() {
        out.push_str(&format!("Preset{index}={}\n", preset.name));
    }
    for preset in presets {
        out.push('\n');
        out.push_str(&format!("[{}]\n", preset.name));
        out.push_str(&format!("Preamp={}\n", preset.preamp));
        for (index, band) in preset.bands.iter().enumerate() {
            out.push_str(&format!("Band{index}={band}\n"));
        }
    }
    out
}

pub fn parse_xmms_preset_file(contents: &str) -> Option<EqualizerPreset> {
    let preamp = read_section_f64(contents, "Equalizer preset", "Preamp")?;
    let mut bands = [0.0; 10];
    for (index, band) in bands.iter_mut().enumerate() {
        *band = read_section_f64(contents, "Equalizer preset", &format!("Band{index}"))?;
    }
    Some(EqualizerPreset::new("File", preamp, bands))
}

pub fn serialize_xmms_preset_file(preset: &EqualizerPreset) -> String {
    let mut out = String::from("[Equalizer preset]\n");
    out.push_str(&format!("Preamp={}\n", preset.preamp));
    for (index, band) in preset.bands.iter().enumerate() {
        out.push_str(&format!("Band{index}={band}\n"));
    }
    out
}

pub fn parse_winamp_eqf(bytes: &[u8]) -> Vec<EqualizerPreset> {
    if bytes.len() < 31 || !bytes.starts_with(b"Winamp EQ library file v1.1") {
        return Vec::new();
    }
    let mut offset = 31;
    let mut presets = Vec::new();
    while offset + 257 + 11 <= bytes.len() {
        let name_bytes = &bytes[offset..offset + 257];
        offset += 257;
        let bands = &bytes[offset..offset + 11];
        offset += 11;

        let name_end = name_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(name_bytes.len());
        let name = String::from_utf8_lossy(&name_bytes[..name_end])
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }

        let mut preset_bands = [0.0; 10];
        for index in 0..10 {
            preset_bands[index] = 20.0 - (f64::from(bands[index]) * 40.0 / 64.0);
        }
        let preamp = 20.0 - (f64::from(bands[10]) * 40.0 / 64.0);
        presets.push(EqualizerPreset::new(name, preamp, preset_bands));
    }
    presets
}

pub fn serialize_winamp_eqf(preset: &EqualizerPreset) -> Vec<u8> {
    let mut out = Vec::with_capacity(31 + 257 + 11);
    out.extend_from_slice(b"Winamp EQ library file v1.1\x1a!--");
    let mut name = [0u8; 257];
    let source = if preset.name.is_empty() {
        b"Entry1".as_slice()
    } else {
        preset.name.as_bytes()
    };
    let len = source.len().min(256);
    name[..len].copy_from_slice(&source[..len]);
    out.extend_from_slice(&name);
    for band in preset.bands {
        out.push(db_to_winamp_byte(band));
    }
    out.push(db_to_winamp_byte(preset.preamp));
    out
}

fn parse_named_preset(contents: &str, name: &str) -> Option<EqualizerPreset> {
    let preamp = read_section_f64(contents, name, "Preamp")?;
    let mut bands = [0.0; 10];
    for (index, band) in bands.iter_mut().enumerate() {
        *band = read_section_f64(contents, name, &format!("Band{index}"))?;
    }
    Some(EqualizerPreset::new(name, preamp, bands))
}

fn sorted_numbered_entries(contents: &str, section: &str, prefix: &str) -> Vec<(usize, String)> {
    let mut entries = Vec::new();
    let mut in_section = false;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_section = &line[1..line.len() - 1] == section;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(index) = key
            .trim()
            .strip_prefix(prefix)
            .and_then(|value| value.parse().ok())
        else {
            continue;
        };
        entries.push((index, value.trim().to_string()));
    }
    entries.sort_by_key(|(index, _)| *index);
    entries
}

fn read_section_f64(contents: &str, section: &str, key: &str) -> Option<f64> {
    let mut in_section = false;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_section = &line[1..line.len() - 1] == section;
            continue;
        }
        if in_section {
            let Some((entry_key, value)) = line.split_once('=') else {
                continue;
            };
            if entry_key.trim() == key {
                return value.trim().parse().ok();
            }
        }
    }
    None
}

fn db_to_winamp_byte(db: f64) -> u8 {
    (63.0 - (((db.clamp(-20.0, 20.0) + 20.0) * 63.0) / 40.0))
        .round()
        .clamp(0.0, 63.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preset_store_round_trips() {
        let presets = vec![EqualizerPreset::new("Default", 1.5, [0.0; 10])];
        let serialized = serialize_preset_store(&presets);
        let reparsed = parse_preset_store(&serialized);
        assert_eq!(reparsed, presets);
    }

    #[test]
    fn standalone_preset_round_trips() {
        let preset = EqualizerPreset::new("Song", -2.0, [3.0; 10]);
        let serialized = serialize_xmms_preset_file(&preset);
        let reparsed = parse_xmms_preset_file(&serialized).unwrap();
        assert_eq!(reparsed.preamp, -2.0);
        assert_eq!(reparsed.bands, [3.0; 10]);
    }

    #[test]
    fn winamp_eqf_round_trips_single_entry() {
        let preset = EqualizerPreset::new("Entry1", 0.0, [0.0; 10]);
        let serialized = serialize_winamp_eqf(&preset);
        let reparsed = parse_winamp_eqf(&serialized);
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].name, "Entry1");
        assert!(reparsed[0].preamp.abs() < 0.7);
    }

    #[test]
    fn imports_multi_entry_winamp_q1_file() {
        let mut contents = serialize_winamp_eqf(&EqualizerPreset::new("Bass", 6.0, [9.0; 10]));
        let second = serialize_winamp_eqf(&EqualizerPreset::new("Treble", -3.0, [-6.0; 10]));
        contents.extend_from_slice(&second[31..]);

        let path = std::env::temp_dir().join(format!(
            "xmms-rs-winamp-presets-{}-{}.q1",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, contents).unwrap();

        let imported = import_winamp_eqf(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].name, "Bass");
        assert_eq!(imported[1].name, "Treble");
        assert!(imported[0].preamp > imported[1].preamp);
    }

    #[test]
    fn hardcoded_winamp_original_presets_include_all_upstream_eqf_entries() {
        let presets = winamp_original_presets();
        assert_eq!(presets.len(), 20);
        assert_eq!(presets.first().unwrap().name, "Classical");
        assert!(presets.iter().any(|preset| preset.name == "Full Bass"));
        assert!(presets.iter().any(|preset| preset.name == "Full Treble"));
        assert_eq!(presets.last().unwrap().name, "Preamp -12dB (Flat)");
    }

    #[test]
    fn built_in_menu_presets_are_default_then_all_original_presets() {
        let presets = built_in_equalizer_presets();
        assert_eq!(presets.len(), 21);
        assert_eq!(presets[0], EqualizerPreset::zero("Default"));
        assert_eq!(presets[1..], winamp_original_presets());
    }
}
