use crate::config::{BaseConfig, TlsMode};
use crate::job::JobConfig;
use crate::logger::MailLog;
use crate::recipient::{self, Recipient};
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use lettre::message::header::ContentType;
use lettre::message::{Attachment, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{Message, SmtpTransport, Transport};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;

pub struct SendStats {
    pub sent: u64,
    pub failed: u64,
}

pub async fn run(
    base: &BaseConfig,
    job: &JobConfig,
    job_path: &Path,
    email_col: &str,
    password: String,
    dry_run: bool,
) -> Result<SendStats> {
    let log = MailLog::new(job_path);

    // Load templates
    let body_html = std::fs::read_to_string(&job.body_html)
        .with_context(|| format!("Cannot read HTML body: {}", job.body_html))?;
    let body_txt = std::fs::read_to_string(&job.body_txt)
        .with_context(|| format!("Cannot read TXT body: {}", job.body_txt))?;

    // Load recipients
    let all_recipients = recipient::load_recipients(&job.recipient_list, email_col)?;
    log::info!("Loaded {} recipient(s) from CSV", all_recipients.len());

    // Load blacklist
    let blacklist = match &job.blacklist {
        Some(p) => {
            let bl = recipient::load_blacklist(p)?;
            log::info!("Loaded {} blacklisted address(es)", bl.len());
            bl
        }
        None => std::collections::HashSet::new(),
    };

    // Filter recipients
    let recipients: Vec<Recipient> = all_recipients
        .into_iter()
        .filter(|r| {
            if blacklist.contains(&r.email.to_lowercase()) {
                log::info!("Skipping blacklisted address: {}", r.email);
                false
            } else {
                true
            }
        })
        .collect();
    log::info!(
        "{} recipient(s) remain after blacklist filtering",
        recipients.len()
    );

    // Build SMTP transport
    let transport = build_transport(base, password)?;

    // Resolve attachment paths once
    let attachments: Vec<PathBuf> = job.attachments.iter().map(PathBuf::from).collect();

    let timeout_count = base.timeout_count;
    let timeout_dur = base.parsed_timeout()?;

    let progress = ProgressBar::new(recipients.len() as u64);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} Sending [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({percent}%) - sent: {msg}",
        )
        .context("Failed to build progress bar style")?
        .progress_chars("█▉▊▌▍▎▏"),
    );
    progress.set_message("0, failed: 0");

    let mut sent = 0u64;
    let mut failed = 0u64;

    for (idx, r) in recipients.iter().enumerate() {
        progress.set_message(format!("{sent} failed: {failed} next: {:30}", r.email));
        // Throttle
        if let (Some(n), Some(dur)) = (timeout_count, timeout_dur) {
            if idx > 0 && idx as u64 % n == 0 {
                progress.println(format!(
                    "Sent {} mails, pausing for {}s...",
                    idx,
                    dur.as_secs()
                ));
                sleep(dur).await;
            }
        }

        match send_one(&transport, base, job, r, &body_html, &body_txt, &attachments, dry_run).await {
            Ok(()) => {
                log.write(&r.email, "OK", None).ok();
                sent += 1;
            }
            Err(e) => {
                let err_str = e.to_string();
                log::error!("Failed to send to {}: {}", r.email, err_str);
                log.write(&r.email, "FAILED", Some(&err_str)).ok();

                if is_retryable(&e) {
                    if retry_send(
                        &transport,
                        base,
                        job,
                        r,
                        &body_html,
                        &body_txt,
                        &attachments,
                        &log,
                    )
                    .await
                    {
                        sent += 1;
                    } else {
                        failed += 1;
                    }
                } else {
                    failed += 1;
                }
            }
        }
        progress.inc(1);
    }

    progress.finish_with_message(format!("done - sent: {}, failed: {}", sent, failed));

    Ok(SendStats { sent, failed })
}

async fn retry_send(
    transport: &SmtpTransport,
    base: &BaseConfig,
    job: &JobConfig,
    r: &Recipient,
    body_html: &str,
    body_txt: &str,
    attachments: &[PathBuf],
    log: &MailLog,
) -> bool {
    const MAX_WAIT: Duration = Duration::from_secs(600);
    let mut wait = Duration::from_secs(10);
    let max_attempts = 6;

    for attempt in 1..=max_attempts {
        log::info!(
            "Retry {}/{} for {} in {}s...",
            attempt,
            max_attempts,
            r.email,
            wait.as_secs()
        );
        sleep(wait).await;

        match send_one(transport, base, job, r, body_html, body_txt, attachments, false).await {
            Ok(()) => {
                log::info!("Retry {}: sent to {}", attempt, r.email);
                log.write(&r.email, "OK (retry)", Some(&format!("attempt {}", attempt)))
                    .ok();
                return true;
            }
            Err(e) => {
                let err_str = e.to_string();
                log::warn!("Retry {} failed for {}: {}", attempt, r.email, err_str);
                log.write(&r.email, &format!("RETRY {} FAILED", attempt), Some(&err_str))
                    .ok();
                if !is_retryable(&e) {
                    log::info!("Non-retryable error on retry, giving up on {}", r.email);
                    return false;
                }
                wait = (wait * 2).min(MAX_WAIT);
            }
        }
    }
    log::error!("All retries exhausted for {}", r.email);
    false
}

async fn send_one(
    transport: &SmtpTransport,
    base: &BaseConfig,
    job: &JobConfig,
    r: &Recipient,
    body_html: &str,
    body_txt: &str,
    attachments: &[PathBuf],
    dry_run: bool,
) -> Result<()> {
    let html = if job.personalize {
        recipient::personalize(body_html, r)
    } else {
        body_html.to_string()
    };
    let txt = if job.personalize {
        recipient::personalize(body_txt, r)
    } else {
        body_txt.to_string()
    };
    let subject = if job.personalize {
        recipient::personalize(&job.subject, r)
    } else {
        job.subject.clone()
    };

    // Build alternative (txt + html) part
    let alternative = MultiPart::alternative()
        .singlepart(
            SinglePart::builder()
                .header(lettre::message::header::ContentType::TEXT_PLAIN)
                .body(txt),
        )
        .singlepart(
            SinglePart::builder()
                .header(lettre::message::header::ContentType::TEXT_HTML)
                .body(html),
        );

    // Wrap in mixed multipart only when there are attachments
    let body = if attachments.is_empty() {
        alternative
    } else {
        let mut mixed = MultiPart::mixed().multipart(alternative);
        for path in attachments {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("attachment")
                .to_string();
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let content_type = ContentType::parse(&mime.to_string())
                .unwrap_or_else(|_| ContentType::parse("application/octet-stream").unwrap());
            let data = std::fs::read(path)
                .with_context(|| format!("Cannot read attachment: {}", path.display()))?;
            mixed = mixed.singlepart(Attachment::new(filename).body(data, content_type));
        }
        mixed
    };

    let email = Message::builder()
        .from(base.account.parse().context("Invalid sender address")?)
        .to(r
            .email
            .parse()
            .with_context(|| format!("Invalid recipient address: {}", r.email))?)
        .subject(subject)
        .multipart(body)
        .context("Failed to build email message")?;

    if dry_run {
        sleep(Duration::from_millis(100)).await; // simulate sending
        return Ok(()); // return early to skip sending
    }
    transport
        .send(&email)
        .with_context(|| format!("SMTP error for {}", r.email))?;
    Ok(())
}

fn build_transport(base: &BaseConfig, password: String) -> Result<SmtpTransport> {
    let creds = Credentials::new(base.account.clone(), password);

    let transport = match base.tls_mode {
        TlsMode::Starttls => {
            let tls = TlsParameters::new(base.smtp_server.clone())
                .context("Failed to build TLS parameters")?;
            SmtpTransport::starttls_relay(&base.smtp_server)
                .context("Failed to create STARTTLS transport")?
                .port(base.smtp_port)
                .credentials(creds)
                .tls(Tls::Required(tls))
                .build()
        }
        TlsMode::Tls => {
            let tls = TlsParameters::new(base.smtp_server.clone())
                .context("Failed to build TLS parameters")?;
            SmtpTransport::relay(&base.smtp_server)
                .context("Failed to create TLS transport")?
                .port(base.smtp_port)
                .credentials(creds)
                .tls(Tls::Wrapper(tls))
                .build()
        }
        TlsMode::Plain => SmtpTransport::builder_dangerous(&base.smtp_server)
            .port(base.smtp_port)
            .credentials(creds)
            .build(),
    };

    Ok(transport)
}

/// Heuristic: treat connection-level and temporary server errors as retryable.
fn is_retryable(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("connection")
        || msg.contains("timeout")
        || msg.contains("temporarily")
        || msg.contains("try again")
        || msg.contains("421")
        || msg.contains("450")
        || msg.contains("451")
        || msg.contains("452")
}