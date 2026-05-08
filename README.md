# send-all-rs

A fast, personalized bulk email sender written in Rust.

## Install

```bash
cargo install --git https://github.com/JonasFovea/send-all-rs
```

## Setup

Create `~/.sendallconf`:

```toml
smtp_server = "smtp.example.com"
smtp_port = 587
account = "you@example.com"
tls_mode = "starttls"   # "starttls" | "tls" | "plain"

# Optional throttling
timeout_count = 50           # pause after every N mails
timeout_duration = "5min"       # bare number = minutes; s, min, h, d supported
```

## Usage

```bash
send-all-rs job_example.json
```

```
Usage: send-all-rs [OPTIONS] <JOB>

Arguments:
  <JOB>  Path to the job JSON file

Options:
      --baseconfig <PATH>      Override the default base config path (~/.sendallconf)
      --email-column <COLUMN>  Override the CSV column name used as the email address (default: E-Mail) [default: E-Mail]
      --dry-run                Perform a dry run without actually sending messages
  -h, --help                   Print help
```

## Job file (JSON)

```json
{
  "subject": "Hello $FirstName$, your invoice is ready",
  "recipient_list": "recipients.csv",
  "body_html": "body.html",
  "body_txt": "body.txt",
  "personalize": true,
  "attachments": [
    "invoice.pdf"
  ],
  "blacklist": "blacklist.txt"
}
```

## Recipient CSV

The CSV must have a header row. The default email column name is `E-Mail`.

```
E-Mail,FirstName,LastName
alice@example.com,Alice,Smith
```

## Personalization

If `"personalize": true`, placeholders like `$FirstName$` in the subject, HTML body, and text body are replaced with the
matching CSV column value. Missing columns produce a warning but do not stop the run.

## Blacklist

A plain-text file with one email address per line (case-insensitive).

## Log file

A `.log` file is written alongside the job JSON, e.g. `job_example.log`:

```
[2026-05-08T14:32:01+0200] alice@example.com | OK
[2026-05-08T14:32:02+0200] bob@example.com | FAILED | SMTP error: ...
[2026-05-08T14:32:45+0200] bob@example.com | OK (retry) | attempt 1
```

## Throttling

Set `timeout_count` and `timeout_duration` in the base config to pause after every N emails. This helps avoid SMTP rate
limits.

Duration formats: `30s`, `5min`, `2h`, `1d` — bare numbers are treated as minutes.