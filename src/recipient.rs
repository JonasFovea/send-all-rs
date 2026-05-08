use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// A single recipient row from the CSV. The email field is extracted separately.
#[derive(Debug, Clone)]
pub struct Recipient {
    pub email: String,
    /// All columns from the CSV row, keyed by header name
    pub fields: HashMap<String, String>,
}

/// Load recipients from a CSV file. The email column is identified by `email_col`.
pub fn load_recipients(path: &str, email_col: &str) -> Result<Vec<Recipient>> {
    let mut rdr = csv::Reader::from_path(path)
        .with_context(|| format!("Failed to open recipient list: {}", path))?;

    let headers: Vec<String> = rdr
        .headers()
        .with_context(|| "Failed to read CSV headers")?
        .iter()
        .map(|h| h.to_string())
        .collect();

    if !headers.contains(&email_col.to_string()) {
        anyhow::bail!(
            "Email column '{}' not found in CSV. Available columns: {}",
            email_col,
            headers.join(", ")
        );
    }

    let mut recipients = Vec::new();
    for (i, result) in rdr.records().enumerate() {
        let record = result.with_context(|| format!("Failed to read CSV row {}", i + 2))?;
        let fields: HashMap<String, String> = headers
            .iter()
            .zip(record.iter())
            .map(|(h, v)| (h.clone(), v.to_string()))
            .collect();

        let email = match fields.get(email_col) {
            Some(e) if !e.trim().is_empty() => e.trim().to_string(),
            _ => {
                log::warn!("Row {}: missing email address, skipping", i + 2);
                continue;
            }
        };

        recipients.push(Recipient { email, fields });
    }
    Ok(recipients)
}

/// Load a blacklist file — one email address per line, case-insensitive.
pub fn load_blacklist(path: &str) -> Result<HashSet<String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read blacklist: {}", path))?;
    Ok(content
        .lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Replace $Placeholder$ tokens in a template with values from the recipient's fields.
/// Warns (but continues) if a placeholder has no matching column.
pub fn personalize(template: &str, recipient: &Recipient) -> String {
    let mut result = template.to_string();
    // Collect all $...$ tokens
    let mut i = 0;
    let bytes = template.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'$' {
            if let Some(end) = template[i + 1..].find('$') {
                let placeholder = &template[i + 1..i + 1 + end];
                if !placeholder.is_empty() {
                    if let Some(val) = recipient.fields.get(placeholder) {
                        result = result.replace(&format!("${}$", placeholder), val);
                    } else {
                        log::warn!(
                            "Placeholder '${}$' has no matching column for recipient '{}'",
                            placeholder,
                            recipient.email
                        );
                    }
                }
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    result
}