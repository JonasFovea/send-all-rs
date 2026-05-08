mod config;
mod job;
mod logger;
mod mailer;
mod recipient;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "send-all-rs", about = "Send batches of personalized emails")]
struct Cli {
    /// Path to the job JSON file
    job: PathBuf,

    /// Override the default base config path (~/.sendallconf)
    #[arg(long, value_name = "PATH")]
    baseconfig: Option<PathBuf>,

    /// Override the CSV column name used as the email address (default: E-Mail)
    #[arg(long, value_name = "COLUMN", default_value = "E-Mail")]
    email_column: String,

    /// Perform a dry run without actually sending messages
    #[arg(long, value_name = "DRYRUN", default_value = "false")]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    // Load base config
    let base_cfg_path = cli.baseconfig.unwrap_or_else(|| {
        dirs::home_dir()
            .expect("Cannot determine home directory")
            .join(".sendallconf")
    });
    let base_cfg = config::BaseConfig::load(&base_cfg_path)?;

    // Load job config
    let job_cfg = job::JobConfig::load(&cli.job)?;

    // Print summary
    print_summary(
        &base_cfg,
        &job_cfg,
        &base_cfg_path,
        &cli.job,
        &cli.email_column,
    );

    // Prompt for password
    let pw_config = rpassword::ConfigBuilder::new()
        .password_feedback_mask('🤫')
        .build();
    let password = rpassword::prompt_password_with_config("SMTP password: ",pw_config)?;

    // Run the mailer
    let stats = mailer::run(
        &base_cfg,
        &job_cfg,
        &cli.job,
        &cli.email_column,
        password,
        cli.dry_run,
    )
    .await?;

    let now = chrono::Local::now();
    println!("\n========================================");
    println!("Done! Sent {} mail(s) successfully.", stats.sent);
    if stats.failed > 0 {
        println!("Failed / skipped: {}", stats.failed);
    }
    println!("Finished at: {}", now.format("%Y-%m-%d %H:%M:%S"));
    println!("========================================");

    Ok(())
}

fn print_summary(
    base: &config::BaseConfig,
    job: &job::JobConfig,
    base_path: &PathBuf,
    job_path: &PathBuf,
    email_col: &str,
) {
    println!("========================================");
    println!("  send-all-rs — Job Summary");
    println!("========================================");
    println!("Base config:    {}", base_path.display());
    println!("SMTP server:    {}:{}", base.smtp_server, base.smtp_port);
    println!("Account:        {}", base.account);
    println!("TLS mode:       {}", base.tls_mode);
    if let (Some(n), Some(d)) = (base.timeout_count, &base.timeout_duration) {
        println!("Throttle:       pause after every {} mails for {}", n, d);
    } else {
        println!("Throttle:       none");
    }
    println!("----------------------------------------");
    println!("Job file:       {}", job_path.display());
    println!("Subject:        {}", job.subject);
    println!("Recipients:     {}", job.recipient_list);
    println!("Body (HTML):    {}", job.body_html);
    println!("Body (TXT):     {}", job.body_txt);
    println!("Personalize:    {}", job.personalize);
    println!("Email column:   {}", email_col);
    if let Some(bl) = &job.blacklist {
        println!("Blacklist:      {}", bl);
    } else {
        println!("Blacklist:      none");
    }
    if job.attachments.is_empty() {
        println!("Attachments:    none");
    } else {
        for a in &job.attachments {
            println!("Attachment:     {}", a);
        }
    }
    println!("========================================\n");
}
