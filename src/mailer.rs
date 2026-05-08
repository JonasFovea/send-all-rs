use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub struct MailLog {
    path: std::path::PathBuf,
}

impl MailLog {
    pub fn new(job_path: &Path) -> Self {
        let log_path = job_path.with_extension("log");
        Self { path: log_path }
    }

    pub fn write(&self, email: &str, status: &str, detail: Option<&str>) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let ts = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%z");
        match detail {
            Some(d) => writeln!(file, "[{}] {} | {} | {}", ts, email, status, d)?,
            None => writeln!(file, "[{}] {} | {}", ts, email, status)?,
        }
        Ok(())
    }
}