//! Lightweight console logging shared by frontends and app orchestration.

use std::fmt;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConsoleLogLevel {
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl ConsoleLogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "false" | "0" => Some(Self::Off),
            "error" | "err" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" | "all" | "true" | "1" => Some(Self::Trace),
            _ => None,
        }
    }
}

impl fmt::Display for ConsoleLogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn console_log(level: ConsoleLogLevel, args: fmt::Arguments<'_>) {
    if console_log_enabled(level) {
        eprintln!("xmms-rs {level}: {args}");
    }
}

fn console_log_enabled(level: ConsoleLogLevel) -> bool {
    level != ConsoleLogLevel::Off && level <= configured_console_log_level()
}

fn configured_console_log_level() -> ConsoleLogLevel {
    static LEVEL: OnceLock<ConsoleLogLevel> = OnceLock::new();
    *LEVEL.get_or_init(|| {
        std::env::var("XMMS_RS_LOG")
            .ok()
            .or_else(|| std::env::var("XMMS_LOG").ok())
            .and_then(|value| ConsoleLogLevel::parse(&value))
            .unwrap_or(ConsoleLogLevel::Info)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_console_log_levels() {
        assert_eq!(ConsoleLogLevel::parse("off"), Some(ConsoleLogLevel::Off));
        assert_eq!(ConsoleLogLevel::parse("info"), Some(ConsoleLogLevel::Info));
        assert_eq!(ConsoleLogLevel::parse("all"), Some(ConsoleLogLevel::Trace));
        assert_eq!(ConsoleLogLevel::parse("unknown"), None);
    }

    #[test]
    fn console_log_levels_display_lowercase() {
        assert_eq!(ConsoleLogLevel::Info.to_string(), "info");
        assert_eq!(ConsoleLogLevel::Trace.to_string(), "trace");
    }

    #[test]
    fn default_console_log_level_is_info() {
        if std::env::var_os("XMMS_RS_LOG").is_none() && std::env::var_os("XMMS_LOG").is_none() {
            assert_eq!(configured_console_log_level(), ConsoleLogLevel::Info);
        }
    }
}
