#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisMode {
    Analyzer,
    Scope,
    Off,
    Milkdrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisAnalyzerStyle {
    Bars,
    Lines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisAnalyzerMode {
    Normal,
    Fire,
    VerticalLines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisScopeMode {
    Dot,
    Line,
    Solid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisFalloffSpeed {
    Slowest,
    Slow,
    Medium,
    Fast,
    Fastest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisVuMode {
    Normal,
    Smooth,
}
