use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct JobConfig {
    pub subject: String,
    pub recipient_list: String,
    pub body_html: String,
    pub body_txt: String,
    pub personalize: bool,
    #[serde(default)]
    pub attachments: Vec<String>,
    pub blacklist: Option<String>,
}

impl JobConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read job config: {}", path.display()))?;
        let cfg: JobConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse job config: {}", path.display()))?;
        Ok(cfg)
    }
}