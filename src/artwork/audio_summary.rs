//! Bounded worker-side WAV summary for static artwork, never an audio-callback
//! tap or system-audio capture. Unsupported formats retain the track seed.
//! Analyze at most 16K frames from the first 256KiB of a PCM WAV container.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub const AUDIO_SUMMARY_VERSION: u32 = 1;
const MAX_READ: u64 = 256 * 1024;
const MAX_FRAMES: usize = 16384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSummary {
    pub rms_level: u8,
    pub peak_level: u8,
    pub crossing_rate: u8,
}

impl AudioSummary {
    pub fn geometry_seed(self, track_seed: u64) -> u64 {
        let features = u64::from(self.rms_level)
            | (u64::from(self.peak_level) << 8)
            | (u64::from(self.crossing_rate) << 16)
            | (u64::from(AUDIO_SUMMARY_VERSION) << 32);
        track_seed ^ features.wrapping_mul(0x9e3779b97f4a7c15)
    }
}

pub fn summarize_wav(path: &Path) -> io::Result<Option<AudioSummary>> {
    if !std::fs::metadata(path)?.is_file() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(MAX_READ).read_to_end(&mut bytes)?;
    Ok(summarize_bytes(&bytes))
}

fn summarize_bytes(bytes: &[u8]) -> Option<AudioSummary> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let declared = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
    if declared < 4 {
        return None;
    }
    let bytes = &bytes[..bytes.len().min(declared.checked_add(8)?)];
    let mut position = 12usize;
    let mut format = None;
    while position.checked_add(8)? <= bytes.len() {
        let kind = &bytes[position..position + 4];
        let size = u32::from_le_bytes(bytes[position + 4..position + 8].try_into().ok()?) as usize;
        position += 8;
        let available = size.min(bytes.len() - position);
        let chunk = &bytes[position..position + available];
        if kind == b"fmt " && chunk.len() >= 16 {
            let encoding = u16::from_le_bytes(chunk[..2].try_into().ok()?);
            let channels = u16::from_le_bytes(chunk[2..4].try_into().ok()?);
            let bits = u16::from_le_bytes(chunk[14..16].try_into().ok()?);
            let rate = u32::from_le_bytes(chunk[4..8].try_into().ok()?);
            let byte_rate = u32::from_le_bytes(chunk[8..12].try_into().ok()?);
            let block = u16::from_le_bytes(chunk[12..14].try_into().ok()?);
            if rate == 0
                || rate > 384000
                || block != channels.checked_mul(bits / 8)?
                || byte_rate != rate.checked_mul(u32::from(block))?
            {
                return None;
            }
            if encoding != 1 || !(1..=2).contains(&channels) || ![8, 16].contains(&bits) {
                return None;
            }
            format = Some((usize::from(channels), usize::from(bits / 8)));
        }
        if kind == b"data" {
            // Partial prefix is valid only when the bounded read cap was hit;
            // a shorter truncated file should not acquire fabricated features.
            if available != size && bytes.len() < MAX_READ as usize {
                return None;
            }
            let (channels, sample_bytes) = format?;
            let frame_size = channels * sample_bytes;
            let mut sum = 0.0f64;
            let mut peak = 0.0f64;
            let mut previous = 0.0;
            let mut crossings = 0usize;
            let mut count = 0usize;
            for frame in chunk.chunks_exact(frame_size).take(MAX_FRAMES) {
                let mut mono = 0.0;
                for sample in frame.chunks_exact(sample_bytes) {
                    mono += if sample_bytes == 1 {
                        (f64::from(sample[0]) - 128.0) / 128.0
                    } else {
                        f64::from(i16::from_le_bytes(sample.try_into().ok()?)) / 32768.0
                    };
                }
                mono /= channels as f64;
                sum += mono * mono;
                peak = peak.max(mono.abs());
                if count > 0 && (mono >= 0.0) != (previous >= 0.0) {
                    crossings += 1;
                }
                previous = mono;
                count += 1;
            }
            if count == 0 {
                return None;
            }
            return Some(AudioSummary {
                rms_level: ((sum / count as f64).sqrt() * 255.0).round() as u8,
                peak_level: (peak * 255.0).round() as u8,
                crossing_rate: (crossings * 255 / count) as u8,
            });
        }
        if available != size {
            return None;
        }
        position = position.checked_add(size)?.checked_add(size % 2)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav(samples: &[i16]) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&(36 + samples.len() as u32 * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(b"\x02\0\x10\0data");
        bytes.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }
    #[test]
    fn silence_and_tone_have_deterministic_distinct_features_and_seeds() {
        let silence = summarize_bytes(&wav(&[0; 100])).unwrap();
        assert_eq!(
            silence,
            AudioSummary {
                rms_level: 0,
                peak_level: 0,
                crossing_rate: 0
            }
        );
        let tone = summarize_bytes(&wav(&[-16000, 16000].repeat(50))).unwrap();
        assert!(tone.rms_level > 100 && tone.peak_level > 100 && tone.crossing_rate > 200);
        assert_ne!(tone.geometry_seed(42), silence.geometry_seed(42));
        assert_eq!(
            tone,
            summarize_bytes(&wav(&[-16000, 16000].repeat(50))).unwrap()
        );
    }
    #[test]
    fn malformed_truncated_and_unsupported_audio_returns_fallback() {
        for bytes in [b"not-wave".to_vec(), wav(&[]), wav(&[1, 2])[..20].to_vec()] {
            assert!(summarize_bytes(&bytes).is_none());
        }
        let mut unsupported = wav(&[1, 2]);
        unsupported[20] = 3;
        assert!(summarize_bytes(&unsupported).is_none());
    }
    #[test]
    fn invalid_header_rates_alignments_and_truncated_data_use_fallback() {
        for offset in [4, 24, 28, 32] {
            let mut bytes = wav(&[100; 16]);
            bytes[offset] = 0;
            bytes[offset + 1] = 0;
            assert!(summarize_bytes(&bytes).is_none());
        }
        let mut bytes = wav(&[100; 16]);
        bytes.pop();
        assert!(summarize_bytes(&bytes).is_none());
        let bytes = wav(&[100; 16]);
        assert_eq!(
            summarize_bytes(&bytes),
            summarize_bytes(&[bytes.clone(), b"ignored-trailing-junk".to_vec()].concat())
        );
    }

    #[test]
    fn sample_count_is_bounded_even_if_wav_contains_more_data() {
        let first = vec![0; MAX_FRAMES];
        let mut long = first.clone();
        long.extend([32767; 100]);
        assert_eq!(summarize_bytes(&wav(&first)), summarize_bytes(&wav(&long)));
    }
}
