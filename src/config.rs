use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum TlsMode {
    Starttls,
    Tls,
    Plain,
}

impl std::fmt::Display for TlsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsMode::Starttls => write!(f, "starttls"),
            TlsMode::Tls => write!(f, "tls"),
            TlsMode::Plain => write!(f, "plain"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BaseConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub account: String,
    pub tls_mode: TlsMode,
    /// Number of emails to send before pausing
    pub timeout_count: Option<u64>,
    /// Duration string e.g. "5", "5min", "30s", "1h"
    pub timeout_duration: Option<String>,
}

impl BaseConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read base config: {}", path.display()))?;
        let cfg: BaseConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse base config: {}", path.display()))?;
        Ok(cfg)
    }

    /// Returns the parsed timeout duration, if configured.
    pub fn parsed_timeout(&self) -> Result<Option<Duration>> {
        match &self.timeout_duration {
            None => Ok(None),
            Some(s) => Ok(Some(parse_duration(s)?)),
        }
    }
}

/// Parse a duration string. Bare numbers are treated as minutes.
/// Supports: s, sec, min, m, h, d
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    // Try to split at the boundary between digits and letters
    let split_pos = s.find(|c: char| c.is_alphabetic());
    let (num_part, unit_part) = match split_pos {
        None => (s, "min"),
        Some(i) => (&s[..i], s[i..].trim()),
    };
    let num: u64 = num_part
        .trim()
        .parse()
        .with_context(|| format!("Invalid duration number: '{}'", num_part))?;
    let secs = match unit_part.to_lowercase().as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => num,
        "m" | "min" | "mins" | "minute" | "minutes" => num * 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => num * 3600,
        "d" | "day" | "days" => num * 86400,
        other => anyhow::bail!("Unknown duration unit: '{}'", other),
    };
    Ok(Duration::from_secs(secs))
}