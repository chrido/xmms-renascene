#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisMode {
    Analyzer = 0,
    Scope = 1,
    Off = 2,
    Milkdrop = 3,
}

impl VisMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Scope,
            2 => Self::Off,
            3 => Self::Milkdrop,
            _ => Self::Analyzer,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisAnalyzerStyle {
    Bars = 0,
    Lines = 1,
}

impl VisAnalyzerStyle {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Lines,
            _ => Self::Bars,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisAnalyzerMode {
    Normal = 0,
    Fire = 1,
    VerticalLines = 2,
}

impl VisAnalyzerMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Fire,
            2 => Self::VerticalLines,
            _ => Self::Normal,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisScopeMode {
    Dot = 0,
    Line = 1,
    Solid = 2,
}

impl VisScopeMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Dot,
            2 => Self::Solid,
            _ => Self::Line,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisFalloffSpeed {
    Slowest = 0,
    Slow = 1,
    Medium = 2,
    Fast = 3,
    Fastest = 4,
}

impl VisFalloffSpeed {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Slowest,
            1 => Self::Slow,
            3 => Self::Fast,
            4 => Self::Fastest,
            _ => Self::Medium,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisVuMode {
    Normal = 0,
    Smooth = 1,
}

impl VisVuMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Smooth,
            _ => Self::Normal,
        }
    }
}
