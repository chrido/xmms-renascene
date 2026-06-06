use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XpmImage {
    width: usize,
    height: usize,
    argb: Vec<u32>,
}

impl XpmImage {
    pub fn parse(contents: &str) -> Result<Self, XpmError> {
        let strings = extract_strings(contents);
        if strings.is_empty() {
            return Err(XpmError::MissingHeader);
        }

        let mut header = strings[0].split_whitespace();
        let width = parse_header_field(header.next(), "width")?;
        let height = parse_header_field(header.next(), "height")?;
        let ncolors = parse_header_field(header.next(), "color count")?;
        let cpp = parse_header_field(header.next(), "chars per pixel")?;

        if width == 0 || height == 0 || ncolors == 0 || cpp == 0 {
            return Err(XpmError::InvalidHeader(strings[0].clone()));
        }

        if strings.len() < 1 + ncolors + height {
            return Err(XpmError::Truncated {
                expected: 1 + ncolors + height,
                actual: strings.len(),
            });
        }

        let mut colors = HashMap::with_capacity(ncolors);
        for line in &strings[1..1 + ncolors] {
            if line.len() < cpp {
                continue;
            }
            let key = line[..cpp].to_string();
            let value = color_attr_value(line, cpp);
            colors.insert(key, parse_color_value(value.as_deref()));
        }

        let mut argb = Vec::with_capacity(width * height);
        for row in &strings[1 + ncolors..1 + ncolors + height] {
            for x in 0..width {
                let start = x * cpp;
                let end = start + cpp;
                let pixel = if end <= row.len() {
                    colors.get(&row[start..end]).copied().unwrap_or(0xff00_0000)
                } else {
                    0xff00_0000
                };
                argb.push(pixel);
            }
        }

        Ok(Self {
            width,
            height,
            argb,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn pixels_argb(&self) -> &[u32] {
        &self.argb
    }

    pub fn pixel_argb(&self, x: usize, y: usize) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.argb.get(y * self.width + x).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XpmError {
    MissingHeader,
    InvalidHeader(String),
    InvalidHeaderField {
        name: &'static str,
        value: Option<String>,
    },
    Truncated {
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for XpmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XpmError::MissingHeader => write!(f, "missing XPM header"),
            XpmError::InvalidHeader(header) => write!(f, "invalid XPM header: {header}"),
            XpmError::InvalidHeaderField { name, value } => {
                write!(
                    f,
                    "invalid XPM {name}: {}",
                    value.as_deref().unwrap_or("(missing)")
                )
            }
            XpmError::Truncated { expected, actual } => {
                write!(
                    f,
                    "truncated XPM data: expected at least {expected} strings, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for XpmError {}

fn parse_header_field(value: Option<&str>, name: &'static str) -> Result<usize, XpmError> {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| XpmError::InvalidHeaderField {
            name,
            value: value.map(ToOwned::to_owned),
        })
}

fn extract_strings(contents: &str) -> Vec<String> {
    let bytes = contents.as_bytes();
    let mut strings = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }

        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        if bytes[i] == b'"' {
            i += 1;
            let (string, next) = parse_c_string(bytes, i);
            strings.push(string);
            i = next;
            continue;
        }

        i += 1;
    }

    strings
}

fn parse_c_string(bytes: &[u8], mut i: usize) -> (String, usize) {
    let mut out = Vec::new();

    while i < bytes.len() && bytes[i] != b'"' {
        if bytes[i] == b'\\' {
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'n' => {
                    out.push(b'\n');
                    i += 1;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 1;
                }
                b't' => {
                    out.push(b'\t');
                    i += 1;
                }
                b'b' => {
                    out.push(0x08);
                    i += 1;
                }
                b'f' => {
                    out.push(0x0c);
                    i += 1;
                }
                b'v' => {
                    out.push(0x0b);
                    i += 1;
                }
                b'0'..=b'7' => {
                    let mut value = 0_u8;
                    let mut count = 0;
                    while count < 3 && i < bytes.len() && matches!(bytes[i], b'0'..=b'7') {
                        value = value.saturating_mul(8).saturating_add(bytes[i] - b'0');
                        i += 1;
                        count += 1;
                    }
                    out.push(value);
                }
                other => {
                    out.push(other);
                    i += 1;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }

    if i < bytes.len() && bytes[i] == b'"' {
        i += 1;
    }

    (String::from_utf8_lossy(&out).into_owned(), i)
}

fn color_attr_value(line: &str, cpp: usize) -> Option<String> {
    let mut parts = line[cpp..].split_whitespace();
    while let Some(token) = parts.next() {
        let value = parts.next()?;
        if token == "c" {
            return Some(value.to_string());
        }
    }
    None
}

fn parse_color_value(value: Option<&str>) -> u32 {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return 0xff00_0000;
    };

    parse_hex_color(value)
        .or_else(|| parse_named_color(value))
        .unwrap_or(0xff00_0000)
}

fn parse_hex_color(value: &str) -> Option<u32> {
    let digits = match value.len() {
        4 if value.starts_with('#') => 1,
        7 if value.starts_with('#') => 2,
        13 if value.starts_with('#') => 4,
        _ => return None,
    };

    let r = parse_hex_component(&value[1..1 + digits], digits)?;
    let g = parse_hex_component(&value[1 + digits..1 + digits * 2], digits)?;
    let b = parse_hex_component(&value[1 + digits * 2..1 + digits * 3], digits)?;
    Some(0xff00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32)
}

fn parse_hex_component(value: &str, digits: usize) -> Option<u8> {
    let parsed = u32::from_str_radix(value, 16).ok()?;
    let max = (1_u32 << (digits * 4)) - 1;
    Some(((parsed * 255 + max / 2) / max) as u8)
}

fn parse_named_color(value: &str) -> Option<u32> {
    if value.eq_ignore_ascii_case("None") {
        return Some(0);
    }

    if let Some(gray) = value
        .strip_prefix("Gray")
        .or_else(|| value.strip_prefix("Grey"))
        .and_then(|suffix| suffix.parse::<u32>().ok())
        .filter(|gray| *gray <= 100)
    {
        let channel = ((gray * 255 + 50) / 100) as u32;
        return Some(0xff00_0000 | (channel << 16) | (channel << 8) | channel);
    }

    if value.eq_ignore_ascii_case("Green") {
        return Some(0xff00_ff00);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_xpm_and_c_escapes() {
        let image = XpmImage::parse(
            r#"/* comment */
            static char *x[] = {
            "2 2 2 1",
            "a c None",
            "b c #0f0",
            "ab",
            "ba"
            };"#,
        )
        .unwrap();

        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(image.pixel_argb(0, 0), Some(0));
        assert_eq!(image.pixel_argb(1, 0), Some(0xff00_ff00));
        assert_eq!(image.pixel_argb(2, 0), None);
    }

    #[test]
    fn unsupported_or_missing_colors_fall_back_to_black_like_c_parser() {
        let image = XpmImage::parse(
            r#"static char *x[] = {
            "1 2 2 1",
            "a c chartreuse",
            "b s symbolic",
            "a",
            "b"
            };"#,
        )
        .unwrap();

        assert_eq!(image.pixel_argb(0, 0), Some(0xff00_0000));
        assert_eq!(image.pixel_argb(0, 1), Some(0xff00_0000));
    }

    #[test]
    fn parses_wide_hex_and_gray_names() {
        assert_eq!(parse_color_value(Some("#ffff00000000")), 0xffff_0000);
        assert_eq!(parse_color_value(Some("Gray100")), 0xffff_ffff);
        assert_eq!(parse_color_value(Some("Grey0")), 0xff00_0000);
    }
}
