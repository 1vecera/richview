//! The command line: decodes arguments, calls the operation layer (ARC-08) and prints the result.
use crate::{
    ops::{self, Failure, FolderRun, GenerateRequest, Input},
    resolver::{self, Target},
    server::{AppState, Preview},
    storage::FsStorage,
};
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use richview_core::document::{Diagnostic, RenderOptions};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Pretty,
    Json,
}
/// What `generate` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum To {
    /// The interactive HTML report.
    Report,
    /// A PDF printed by a local Chromium-family browser.
    Pdf,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PageSize {
    A4,
    Letter,
}
impl From<PageSize> for richview_core::document::Page {
    fn from(page: PageSize) -> Self {
        match page {
            PageSize::A4 => Self::A4,
            PageSize::Letter => Self::Letter,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "richview",
    about = "Rich Markdown: render, check and publish reports from your agent",
    // The result contract of TYP-54, where an agent looks first.
    after_help = "init, validate, check, generate, publish and report exit 0 when no finding is an error, 1 on any error (a draft with block errors is still written) and 2 on a usage error. With --format json each prints one object: ok, status (clean or issues), output (the file written, or null) and diagnostics. learn prints text, and serve, server and mcp run until stopped. asset and image print their own JSON and exit 2 on invalid input or configuration, 3 when a transfer or Unsplash fails and, for asset, 4 on a verification mismatch."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// The same command definition used by argument parsing and generated references.
pub(crate) fn definition() -> clap::Command {
    Cli::command()
}

/// How a report command reads a report.
#[derive(Args, Clone)]
struct ReadArgs {
    /// Style (a builtin name or a file in the report folder) replacing the source's `styles` for this call; repeatable
    #[arg(long = "style")]
    styles: Vec<String>,
    /// Trusted `connectors.toml` binding the names in `report.connectors.yaml` to data sources
    #[arg(long, env = "RICHVIEW_CONNECTORS")]
    connectors: Option<PathBuf>,
}
impl ReadArgs {
    fn input(&self, report: &Path) -> Input {
        Input {
            path: report.to_path_buf(),
            styles: self.styles.clone(),
            connectors: self.connectors.clone(),
        }
    }
}

/// The one report `serve` and `publish` read.
#[derive(Args, Clone)]
struct ReportArgs {
    /// The report's `.rmd`, saved in its own folder (a folder holding one `.rmd` also works)
    #[arg(value_parser = one_report)]
    input: PathBuf,
    #[command(flatten)]
    read: ReadArgs,
}

/// The report, or the folder of reports, that `generate` and `check` read.
#[derive(Args, Clone)]
struct ReportsArgs {
    /// The report's `.rmd` or the folder holding it; a folder of report folders stands for every report below it
    #[arg(value_parser = report_target)]
    input: Target,
    #[command(flatten)]
    read: ReadArgs,
}

#[derive(Args, Clone)]
struct FormatArg {
    /// `pretty` for people, `json` for agents
    #[arg(long, value_enum, default_value = "pretty")]
    format: OutputFormat,
}

/// Where an owner action goes and who is asking.
#[derive(Args)]
struct OwnerArgs {
    /// Server URL (https, or http on loopback)
    #[arg(long, env = "RICHVIEW_SERVER")]
    server: Option<String>,
    /// Bearer token of the server
    #[arg(long, env = "RICHVIEW_TOKEN", hide_env_values = true)]
    token: Option<String>,
    #[command(flatten)]
    format: FormatArg,
}

#[derive(Subcommand)]
enum ReportCommand {
    /// Delete a version for good; its address answers 410 and it can never be published again
    Delete {
        slug: String,
        version: String,
        /// Delete without asking
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        owner: OwnerArgs,
    },
    /// Make a version the one the report address serves
    Pin {
        slug: String,
        version: String,
        #[command(flatten)]
        owner: OwnerArgs,
    },
    /// Serve the highest release version again
    Unpin {
        slug: String,
        #[command(flatten)]
        owner: OwnerArgs,
    },
    /// Re-render a stored version with this engine and say whether the HTML changes; stores nothing
    Regenerate {
        slug: String,
        version: String,
        #[command(flatten)]
        owner: OwnerArgs,
    },
}

impl ReportCommand {
    fn owner(&self) -> &OwnerArgs {
        match self {
            ReportCommand::Delete { owner, .. }
            | ReportCommand::Pin { owner, .. }
            | ReportCommand::Unpin { owner, .. }
            | ReportCommand::Regenerate { owner, .. } => owner,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Write the self-contained report, or a PDF, into the report folder's Rendered/
    #[command(alias = "g")]
    Generate {
        #[command(flatten)]
        args: ReportsArgs,
        /// File name inside Rendered/; a name ending in .pdf implies --to pdf
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write the interactive report (default) or a PDF printed by a local Chromium-family browser
        #[arg(long, value_enum)]
        to: Option<To>,
        /// Paper size of a PDF
        #[arg(long, value_enum, default_value = "a4")]
        page: PageSize,
        /// Chromium, Google Chrome or Microsoft Edge executable for PDF output (or RICHVIEW_BROWSER)
        #[arg(long)]
        browser: Option<PathBuf>,
        /// Seconds a PDF waits for fonts and charts to settle
        #[arg(long, default_value_t = 30)]
        settle_timeout: u64,
        #[command(flatten)]
        format: FormatArg,
    },
    /// Check the structure of a report without rendering it
    Validate {
        /// The report's `.rmd` or the folder holding it; a folder of report folders stands for every report below it
        #[arg(value_parser = report_target)]
        input: Target,
        /// Also apply the publish rules: title, description, version and a valid slug
        #[arg(long)]
        publish: bool,
        #[command(flatten)]
        format: FormatArg,
    },
    /// Render the report and return every diagnostic and each chart's geometry
    Check {
        #[command(flatten)]
        args: ReportsArgs,
        /// Measure charts at this width in pixels instead of the widths readers get; repeatable
        #[arg(long)]
        chart_width: Vec<f64>,
        #[command(flatten)]
        format: FormatArg,
    },
    /// Start a report folder from the template, never overwriting
    Init {
        /// Folder to create; defaults to the report name
        dir: Option<PathBuf>,
        /// Name of the report file
        #[arg(long, default_value = "report")]
        name: String,
        #[command(flatten)]
        format: FormatArg,
    },
    /// Print the authoring guide, or one section of it
    Learn {
        /// grammar, chart-contract, style, workflow or anti-slop
        section: Option<String>,
    },
    /// Serve the RichView tools to an agent over MCP (stdio)
    Mcp,
    /// Publish a version of the report to a RichView server
    Publish {
        #[command(flatten)]
        args: ReportArgs,
        /// Server URL (https, or http on loopback)
        #[arg(long, env = "RICHVIEW_SERVER")]
        server: Option<String>,
        /// Choose and remember a publish server again (without RICHVIEW_SERVER)
        #[arg(long)]
        choose_server: bool,
        /// Bearer token of the server
        #[arg(long, env = "RICHVIEW_TOKEN", hide_env_values = true)]
        token: Option<String>,
        /// Address of the report; defaults to the frontmatter slug, then the title
        #[arg(long)]
        slug: Option<String>,
        /// Create a report address the server has not stored before
        #[arg(long)]
        new: bool,
        /// Who can open the report: private (you and the people you name), org (every member
        /// of your organization) or public (anyone with the link). Left as it is when omitted
        #[arg(long, value_parser = ["private", "org", "public"])]
        visibility: Option<String>,
        #[command(flatten)]
        format: FormatArg,
    },
    /// Preview the report in a browser and reload it when the folder changes
    Serve {
        #[command(flatten)]
        args: ReportArgs,
        #[arg(short, long, default_value_t = 4711)]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },
    /// Delete, pin, unpin or regenerate a stored report on a server
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
    /// Run RichView Cowork, the report server
    Server {
        #[command(flatten)]
        args: ServerArgs,
    },
    /// Upload, verify and abort integrity-pinned remote data and media
    Asset {
        #[command(subcommand)]
        command: crate::asset_upload::AssetCommand,
    },
    /// Search Unsplash and place a credited photo in a report
    Image {
        #[command(subcommand)]
        command: crate::unsplash::ImageCommand,
    },
}

#[derive(Parser, Clone)]
#[command(name = "richview server")]
pub struct ServerArgs {
    /// Run the multi-organization hosted server with organization-scoped report addresses.
    #[arg(long, env = "RICHVIEW_HOSTED")]
    pub hosted: bool,
    #[arg(short, long, env = "PORT", default_value_t = 8080)]
    pub port: u16,
    #[arg(long, env = "RICHVIEW_BIND", default_value = "127.0.0.1")]
    pub bind: String,
    #[arg(long, env = "RICHVIEW_DATA_DIR", default_value = "./data")]
    pub data_dir: PathBuf,
    #[arg(long, env = "RICHVIEW_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
    #[arg(long, env = "RICHVIEW_PUBLIC_URL")]
    pub public_url: Option<String>,
    /// The origin that serves report pages, apart from the app's (ARC-15). Defaults to this host
    /// on --report-port.
    #[arg(long, env = "RICHVIEW_REPORT_ORIGIN")]
    pub report_origin: Option<String>,
    /// The port of the report listener; defaults to the port after --port (any free port for 0).
    #[arg(long, env = "RICHVIEW_REPORT_PORT")]
    pub report_port: Option<u16>,
    /// Most bytes the report store may hold; a publication past it fails with `publish.quota`.
    #[arg(long, env = "RICHVIEW_QUOTA_BYTES")]
    pub quota_bytes: Option<u64>,
    /// Maximum stored bytes per hosted organization.
    #[arg(long, env = "RICHVIEW_ORG_QUOTA_BYTES", default_value_t = 50 * 1024 * 1024)]
    pub org_quota_bytes: u64,
    /// Maximum live reports per hosted organization.
    #[arg(long, env = "RICHVIEW_ORG_REPORT_LIMIT", default_value_t = 50)]
    pub org_report_limit: u64,
    /// Local delivery outbox for hosted verification mail.
    #[arg(long, env = "RICHVIEW_MAIL_OUTBOX")]
    pub mail_outbox: Option<PathBuf>,
    /// TLS SMTP relay for hosted verification mail.
    #[arg(long, env = "RICHVIEW_SMTP_HOST")]
    pub smtp_host: Option<String>,
    /// SMTP submission port.
    #[arg(long, env = "RICHVIEW_SMTP_PORT", default_value_t = 587)]
    pub smtp_port: u16,
    /// TLS mode for a nonstandard SMTP port: implicit or starttls.
    #[arg(long, env = "RICHVIEW_SMTP_TLS", value_enum)]
    pub smtp_tls: Option<crate::mail::SmtpTls>,
    /// SMTP login; the password comes only from RICHVIEW_SMTP_PASSWORD.
    #[arg(long, env = "RICHVIEW_SMTP_USER", hide_env_values = true)]
    pub smtp_user: Option<String>,
    /// Sender mailbox for hosted mail.
    #[arg(long, env = "RICHVIEW_MAIL_FROM")]
    pub mail_from: Option<String>,
    /// Maximum hosted messages accepted per UTC day.
    #[arg(long, env = "RICHVIEW_MAIL_DAILY_LIMIT", default_value_t = 1000)]
    pub mail_daily_limit: u64,
    /// Maximum hosted accounts; raising it requires restarting the server.
    #[arg(long, env = "RICHVIEW_SIGNUP_ACCOUNT_LIMIT", default_value_t = 500)]
    pub signup_account_limit: usize,
    /// Maximum organizations created by one hosted person; raising it requires a restart.
    #[arg(long, env = "RICHVIEW_ORG_CREATION_LIMIT", default_value_t = 1)]
    pub org_creation_limit: usize,
    /// Port for the separate loopback-only operator listener; its token is in RICHVIEW_OPERATOR_TOKEN.
    #[arg(long, env = "RICHVIEW_OPERATOR_PORT")]
    pub operator_port: Option<u16>,
    /// Who can read a report until its owner shares it: private, org or public.
    #[arg(long, env = "RICHVIEW_DEFAULT_VISIBILITY", default_value = "private")]
    pub default_visibility: String,
    /// Creates this admin on first start when the server has no people yet; the password comes
    /// from RICHVIEW_ADMIN_PASSWORD.
    #[arg(long, env = "RICHVIEW_ADMIN_EMAIL")]
    pub admin_email: Option<String>,
}

async fn server_command(args: ServerArgs) -> Result<(), String> {
    for (flag, origin) in [
        ("--public-url", &args.public_url),
        ("--report-origin", &args.report_origin),
    ] {
        let Some(origin) = origin else { continue };
        let url = reqwest::Url::parse(origin).map_err(|e| e.to_string())?;
        if !["http", "https"].contains(&url.scheme())
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(format!(
                "{flag} must be an HTTP(S) origin without credentials, path, query or fragment"
            ));
        }
    }
    // The two addresses belong together: each names where the other's redirects must go.
    match (&args.public_url, &args.report_origin) {
        (Some(_), None) => {
            return Err("--public-url needs --report-origin: report pages are served from an origin of their own".into());
        }
        (None, Some(_)) => {
            return Err("--report-origin needs --public-url: sign-in sends readers back to the app's own address".into());
        }
        (Some(app), Some(report)) => {
            // Parsed above, so host case and a trailing slash cannot make one origin look like two.
            let same = reqwest::Url::parse(app).ok().map(|u| u.origin())
                == reqwest::Url::parse(report).ok().map(|u| u.origin());
            if same {
                return Err("--report-origin must differ from --public-url: report pages need an origin of their own".into());
            }
            // Cookies are shared between the ports of one host, so another port is not enough.
            let host = |url: &str| {
                reqwest::Url::parse(url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
            };
            if host(app) == host(report) {
                return Err("--report-origin needs a different host name from --public-url: browsers share cookies between the ports of one host".into());
            }
        }
        (None, None) => {}
    }
    let visibility = crate::access::Visibility::parse(&args.default_visibility)
        .ok_or("--default-visibility must be private, org or public".to_owned())?;
    let report_port = args.report_port.unwrap_or(if args.port == 0 {
        0
    } else {
        args.port.wrapping_add(1)
    });
    if args.hosted {
        if !matches!(args.bind.as_str(), "127.0.0.1" | "::1" | "localhost") {
            return Err("Hosted mode must listen on loopback behind its trusted proxy".into());
        }
        if args.token.as_ref().is_some_and(|t| !t.is_empty()) || args.admin_email.is_some() {
            return Err(
                "Hosted mode refuses a service token and server-wide admin bootstrap".into(),
            );
        }
        if !args
            .public_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"))
            || !args
                .report_origin
                .as_deref()
                .is_some_and(|url| url.starts_with("https://"))
        {
            return Err("Hosted mode requires HTTPS app and separate report origins".into());
        }
        if visibility != crate::access::Visibility::Private {
            return Err("Hosted mode requires private default visibility".into());
        }
        if args
            .quota_bytes
            .is_none_or(|quota| quota == 0 || quota < args.org_quota_bytes)
            || args.org_quota_bytes == 0
            || args.org_report_limit == 0
        {
            return Err(
                "Hosted mode requires finite nonzero server and organization limits".into(),
            );
        }
        let outbox = args
            .mail_outbox
            .as_ref()
            .is_some_and(|path| !path.as_os_str().is_empty());
        let smtp = args.smtp_host.as_ref().is_some_and(|host| !host.is_empty());
        if outbox == smtp || args.mail_daily_limit == 0 {
            return Err(
                "Hosted mode requires one mail transport and a positive daily limit".into(),
            );
        }
        if smtp
            && (args.smtp_port == 0
                || args.smtp_user.as_ref().is_none_or(|s| s.is_empty())
                || args.mail_from.as_ref().is_none_or(|s| s.is_empty())
                || std::env::var("RICHVIEW_SMTP_PASSWORD")
                    .ok()
                    .is_none_or(|s| s.is_empty()))
        {
            return Err("SMTP requires a port, user, sender and RICHVIEW_SMTP_PASSWORD".into());
        }
        if smtp {
            crate::mail::SmtpTls::for_port(args.smtp_port, args.smtp_tls)?;
        }
        if args.signup_account_limit == 0 || args.signup_account_limit > 1024 {
            return Err("Hosted account limit must be between 1 and 1024".into());
        }
        if args.operator_port.is_none_or(|port| port == 0) {
            return Err("Hosted mode requires a nonzero --operator-port".into());
        }
        if args
            .operator_port
            .is_some_and(|port| port == args.port || port == report_port)
        {
            return Err("Operator port must differ from both public listener ports".into());
        }
        let operator_token = std::env::var("RICHVIEW_OPERATOR_TOKEN").unwrap_or_default();
        if operator_token.len() < 32
            || !operator_token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(
                "RICHVIEW_OPERATOR_TOKEN must contain at least 32 URL-safe characters".into(),
            );
        }
        if !crate::runtime::docs_available() {
            return Err("Hosted mode needs the staged docs runtime embedded in the binary".into());
        }
    }
    let storage = FsStorage::new(args.data_dir)?
        .with_quota(args.quota_bytes)
        .with_hosted(args.hosted)
        .with_hosted_limits(args.org_quota_bytes, args.org_report_limit);
    let token: Option<Arc<str>> = args.token.filter(|s| !s.is_empty()).map(Arc::from);
    let access = if args.hosted {
        crate::access::Access::open_hosted(storage.dir())?
    } else {
        crate::access::Access::open(storage.dir(), visibility, token.clone())?
    };
    if args.hosted {
        access.set_signup_account_limit(args.signup_account_limit)?;
        access.set_org_creation_limit(args.org_creation_limit)?;
        let manual_outbox = args
            .mail_outbox
            .as_ref()
            .is_some_and(|path| !path.as_os_str().is_empty());
        let transport: Arc<dyn crate::mail::Transport> = if let Some(path) = args
            .mail_outbox
            .as_ref()
            .filter(|path| !path.as_os_str().is_empty())
        {
            Arc::new(crate::mail::Outbox::open(path)?)
        } else {
            Arc::new(crate::mail::Smtp::new(
                args.smtp_host.as_deref().ok_or("Missing SMTP host")?,
                args.smtp_port,
                args.smtp_user.clone().ok_or("Missing SMTP user")?,
                std::env::var("RICHVIEW_SMTP_PASSWORD").map_err(|_| "Missing SMTP password")?,
                args.mail_from.clone().ok_or("Missing sender")?,
                args.smtp_tls,
            )?)
        };
        access.configure_mail(transport, args.mail_daily_limit, manual_outbox)?;
    }
    if let Some(email) = args.admin_email.as_deref().filter(|e| !e.is_empty())
        && !access.has_users()
    {
        let password = std::env::var("RICHVIEW_ADMIN_PASSWORD").unwrap_or_default();
        access
            .create_user(
                "Admin",
                email,
                crate::access::OrgRole::Admin,
                Some(&password),
            )
            .map_err(|e| format!("Cannot create the admin from RICHVIEW_ADMIN_EMAIL: {e:?}"))?;
        eprintln!("richview: created the admin {email}.");
    }
    // Reports stored before access control have no record: they follow the default visibility
    // until an owner or admin shares them, which is worth saying once at start.
    if let Ok(rows) = storage.list() {
        let unclaimed = rows
            .iter()
            .filter_map(|row| row["slug"].as_str())
            .filter(|slug| !access.has_record(slug))
            .count();
        if unclaimed > 0 {
            eprintln!(
                "richview: {unclaimed} stored report(s) have no access record and are {} until someone with the right shares them.",
                args.default_visibility
            );
        }
    }
    if token.is_none() && !args.hosted {
        eprintln!(
            "richview: no token configured; publishing is off until you set RICHVIEW_TOKEN or sign in as a person who may publish."
        );
    }
    if !args.hosted && !access.has_users() && visibility != crate::access::Visibility::Public {
        eprintln!(
            "richview: nobody can sign in yet and reports are {} by default; set RICHVIEW_ADMIN_EMAIL and RICHVIEW_ADMIN_PASSWORD, or pass --default-visibility public.",
            args.default_visibility
        );
    }
    let state = AppState {
        storage,
        access,
        preview: None,
        public_origin: args.public_url,
        report_origin: None,
    };
    crate::server::start_process_shutdown_watchdog(std::time::Duration::from_secs(25))?;
    if args.hosted {
        let operator_port = args.operator_port.ok_or("Missing operator port")?;
        let token: Arc<str> = Arc::from(
            std::env::var("RICHVIEW_OPERATOR_TOKEN").map_err(|_| "Missing operator token")?,
        );
        let queue = state.access.mail_queue().cloned();
        let servers = async {
            tokio::try_join!(
                crate::server::serve_operator(operator_port, token, state.clone()),
                crate::server::serve_split(
                    state,
                    &args.bind,
                    args.port,
                    report_port,
                    args.report_origin
                ),
            )?;
            Ok(())
        };
        tokio::pin!(servers);
        tokio::select! {
            result = &mut servers => result,
            _ = crate::server::shutdown_signal() => {
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(25);
                if let Some(queue) = &queue { queue.begin_shutdown(); }
                let result = tokio::time::timeout_at(deadline, &mut servers).await;
                if let Some(queue) = &queue { queue.finish_shutdown(deadline).await; }
                match result { Ok(result) => result, Err(_) => Ok(()) }
            }
        }
    } else {
        crate::server::serve_split(
            state,
            &args.bind,
            args.port,
            report_port,
            args.report_origin,
        )
        .await
    }
}

pub async fn run() -> Result<(), String> {
    // `--version` names the build commit and the embedded runtime's hash (ARC-07).
    let version: &'static str = Box::leak(crate::runtime::version_text().into_boxed_str());
    let matches = Cli::command().version(version).get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let format = match &cli.command {
        Command::Generate { format, .. }
        | Command::Validate { format, .. }
        | Command::Check { format, .. }
        | Command::Publish { format, .. }
        | Command::Init { format, .. } => format.format,
        Command::Report { command } => command.owner().format.format,
        _ => OutputFormat::Pretty,
    };
    // The report a failure's findings are about; a folder run names its reports itself.
    let context = match &cli.command {
        Command::Publish { args, .. } => Some(args.input.clone()),
        Command::Generate {
            args: ReportsArgs { input, .. },
            ..
        }
        | Command::Check {
            args: ReportsArgs { input, .. },
            ..
        }
        | Command::Validate { input, .. } => match input {
            Target::Report(report) => Some(report.clone()),
            Target::Folder { .. } => None,
        },
        _ => None,
    };
    match execute(cli.command).await {
        Ok(()) => Ok(()),
        Err(failure) => {
            match format {
                OutputFormat::Json => println!("{}", failure.to_json()),
                _ => eprintln!("{}", failure_text(&failure, context.as_deref())),
            }
            std::process::exit(failure.exit_code());
        }
    }
}

/// Asks a yes/no question on the terminal; anything but `y` or `yes` is no. Without a terminal
/// the answer is no, so a script must say `--yes`.
fn confirm(question: &str) -> Result<bool, Failure> {
    use std::io::{BufRead, IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    eprint!("{question}");
    std::io::stderr().flush().ok();
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|e| Failure::host(e.to_string()))?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Marks an error as a usage error where host services still return strings.
pub fn usage(message: impl std::fmt::Display) -> String {
    format!("\u{1}usage:{message}")
}

/// Prints a result: JSON for agents, or the result line on standard output with the diagnostics
/// on standard error (FEA-SF-07). Exits 1 when the result says `ok: false`.
fn finish(result: &Value, summary: String, format: OutputFormat, input: &Path) {
    match format {
        OutputFormat::Json => println!("{result}"),
        OutputFormat::Pretty => {
            let diagnostics: Vec<Diagnostic> =
                serde_json::from_value(result["diagnostics"].clone()).unwrap_or_default();
            let text = diagnostics_text(&diagnostics, Some(input));
            if !text.is_empty() {
                eprintln!("{text}");
            }
            println!("{summary}");
        }
    }
    if !ops::is_ok(result) {
        std::process::exit(1)
    }
}

/// Runs a report command on what it was pointed at and prints the result: one report as ever, or
/// every report of a folder in turn.
async fn run_on(
    target: &Target,
    command: impl AsyncFn(&Path) -> ops::Outcome,
    summary: impl Fn(&Value) -> String,
    format: OutputFormat,
) -> Result<(), Failure> {
    match target {
        Target::Report(report) => {
            let result = command(report).await?;
            finish(&result, summary(&result), format, report);
        }
        Target::Folder { reports, .. } => {
            let mut run = FolderRun::default();
            for report in reports {
                run.keep(report, command(report).await)?;
            }
            finish_folder(&run, summary, format);
        }
    }
    Ok(())
}

/// Prints a folder run: the one JSON object, or each report's findings on standard error and its
/// result line on standard output, both led by the report's path, then the count. Exits 1 when any
/// report has an error.
fn finish_folder(run: &FolderRun, summary: impl Fn(&Value) -> String, format: OutputFormat) {
    let result = run.result();
    match format {
        OutputFormat::Json => println!("{result}"),
        OutputFormat::Pretty => {
            for (report, outcome) in run.outcomes() {
                let lines = |diagnostics: &[Diagnostic]| {
                    diagnostics_lines(diagnostics, Some(report), report.parent())
                };
                let (findings, line) = match outcome {
                    Ok(value) => {
                        let diagnostics: Vec<Diagnostic> =
                            serde_json::from_value(value["diagnostics"].clone())
                                .unwrap_or_default();
                        (lines(&diagnostics), summary(value))
                    }
                    Err(Failure::Diagnostics(diagnostics)) => (lines(diagnostics), "Failed".into()),
                    Err(failure) => (
                        format!("{}: {}", report.display(), failure.message()),
                        "Failed".into(),
                    ),
                };
                if !findings.is_empty() {
                    eprintln!("{findings}");
                }
                println!("{}: {line}", report.display());
            }
            let reports = result["reports"].as_array().map_or(&[][..], Vec::as_slice);
            println!(
                "{} reports, {} with errors",
                reports.len(),
                reports.iter().filter(|report| !ops::is_ok(report)).count()
            );
        }
    }
    if !ops::is_ok(&result) {
        std::process::exit(1)
    }
}

/// Quotes an executable or report path in the human-readable next-step commands.
fn command_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        format!("\"{text}\"")
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

fn publish_summary(result: &Value, server: &str) -> String {
    let action = match result["created"].as_bool() {
        Some(true) => "Published to",
        Some(false) => "Already published at",
        None => "Server response from",
    };
    let operator = if result["operator"].as_str() == Some("RichView") {
        " (operated by RichView)"
    } else {
        ""
    };
    format!(
        "{action} {server}{operator}\nReport: {}\nVersion: {}\nVisibility: {}\nPermalink: {}",
        result["url"].as_str().unwrap_or_default(),
        result["version"].as_str().unwrap_or_default(),
        match result["visibility"].as_str() {
            Some("private") => "private (you and the people you name)",
            Some("org") => "org (every member of your organization)",
            Some("public") => "public (anyone with the link)",
            _ => "unknown",
        },
        result["permalink"].as_str().unwrap_or_default()
    )
}

async fn execute(command: Command) -> Result<(), Failure> {
    match command {
        Command::Generate {
            args,
            output,
            to,
            page,
            browser,
            settle_timeout,
            format,
        } => {
            let pdf = to.map_or_else(
                || output.as_deref().is_some_and(names_pdf),
                |to| to == To::Pdf,
            );
            let request = |report: &Path| GenerateRequest {
                input: args.read.input(report),
                output: output.clone(),
                pdf,
                page: page.into(),
                browser: browser.clone(),
                settle_timeout: std::time::Duration::from_secs(settle_timeout),
            };
            let summary = |result: &Value| {
                let target = result["output"].as_str().unwrap_or_default();
                if pdf {
                    format!(
                        "Generated {target} ({} {} pages, {} bytes, printed by {})",
                        result["pages"],
                        result["page"].as_str().unwrap_or("A4").to_uppercase(),
                        result["bytes"],
                        result["browser"]["product"]
                            .as_str()
                            .unwrap_or("the browser")
                    )
                } else {
                    format!("Generated {target} ({} rich blocks)", result["blocks"])
                }
            };
            let generate = async |report: &Path| ops::generate(&request(report)).await;
            run_on(&args.input, generate, summary, format.format).await?;
        }
        Command::Validate {
            input,
            publish,
            format,
        } => {
            let validate = async |report: &Path| {
                ops::validate(
                    &Input {
                        path: report.to_path_buf(),
                        ..Default::default()
                    },
                    publish,
                )
            };
            let summary = |result: &Value| {
                format!(
                    "{}, {} rich blocks",
                    if ops::is_ok(result) {
                        "Valid"
                    } else {
                        "Invalid"
                    },
                    result["blocks"]
                )
            };
            run_on(&input, validate, summary, format.format).await?;
        }
        Command::Check {
            args,
            chart_width,
            format,
        } => {
            let check = async |report: &Path| ops::check(&args.read.input(report), &chart_width);
            let summary = |result: &Value| {
                format!(
                    "Checked {} charts: {}",
                    result["charts"].as_array().map_or(0, Vec::len),
                    result["status"].as_str().unwrap_or("issues")
                )
            };
            run_on(&args.input, check, summary, format.format).await?;
        }
        Command::Init { dir, name, format } => {
            let file = ops::init(&dir.unwrap_or_else(|| PathBuf::from(&name)), &name)?;
            // `output` is the report written (TYP-54).
            let result = ops::envelope(vec![], Some(&file), serde_json::json!({}));
            let executable = std::env::current_exe()
                .ok()
                .or_else(|| std::env::args_os().next().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("richview"));
            let summary = format!(
                "Created {}\nTry: {} serve {}  ·  {} check {}",
                file.display(),
                command_path(&executable),
                command_path(&file),
                command_path(&executable),
                command_path(&file)
            );
            finish(&result, summary, format.format, &file);
        }
        Command::Learn { section } => {
            // The bytes MCP returns, with no line added (FEA-SF-47).
            print!("{}", ops::learn(section.as_deref())?);
        }
        Command::Mcp => crate::mcp::serve_stdio().await?,
        Command::Server { args } => server_command(args).await?,
        Command::Asset { command } => {
            let exit = crate::asset_upload::run(command).await;
            if exit != 0 {
                std::process::exit(exit);
            }
        }
        Command::Image { command } => {
            let exit = crate::unsplash::run(command).await;
            if exit != 0 {
                std::process::exit(exit);
            }
        }
        Command::Serve { args, port, bind } => {
            // A broken source does not stop the preview: its banner appears and clears when the
            // source is fixed. Only a missing file ends the command.
            read_source(&args.input)?;
            let preview = Preview {
                source: absolute(&args.input)?,
                styles: args.read.styles,
                allow_network: false,
                last_good: Default::default(),
            };
            // The preview writes nothing beside the report: its scratch state lives in the system
            // temporary folder and goes when the command ends (ARC-10).
            let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
            let storage = FsStorage::new(scratch.path().to_path_buf())?;
            let access = crate::access::Access::open(
                storage.dir(),
                crate::access::Visibility::Public,
                None,
            )?;
            crate::server::serve(
                AppState {
                    storage,
                    access,
                    preview: Some(preview),
                    public_origin: None,
                    report_origin: None,
                },
                &bind,
                port,
            )
            .await?;
            drop(scratch);
        }
        Command::Report { command } => {
            let (owner, action, summary) = match &command {
                ReportCommand::Delete {
                    slug,
                    version,
                    yes,
                    owner,
                } => {
                    if !yes
                        && !confirm(&format!(
                            "Delete {slug} {version}? Its address will answer 410 and it can never be published again. [y/N] "
                        ))?
                    {
                        return Err(Failure::usage(
                            "Nothing was deleted. Pass --yes to delete without asking.",
                        ));
                    }
                    (
                        owner,
                        ops::OwnerAction::Delete { slug, version },
                        format!("Deleted {slug} {version}."),
                    )
                }
                ReportCommand::Pin {
                    slug,
                    version,
                    owner,
                } => (
                    owner,
                    ops::OwnerAction::Pin { slug, version },
                    format!("Pinned {slug} {version}."),
                ),
                ReportCommand::Unpin { slug, owner } => (
                    owner,
                    ops::OwnerAction::Unpin { slug },
                    format!("Unpinned {slug}."),
                ),
                ReportCommand::Regenerate {
                    slug,
                    version,
                    owner,
                } => (
                    owner,
                    ops::OwnerAction::Regenerate { slug, version },
                    format!("Regenerated {slug} {version}."),
                ),
            };
            let server = owner
                .server
                .clone()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    Failure::usage("Missing server. Pass --server or set RICHVIEW_SERVER.")
                })?;
            let token = owner
                .token
                .clone()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    Failure::usage("Missing API token. Set RICHVIEW_TOKEN or pass --token.")
                })?;
            bound_token(&server, &token)?;
            let result = ops::report(&server, &token, action).await?;
            let summary = match result["changed"].as_bool() {
                Some(true) => format!("{summary} The HTML changes."),
                Some(false) => format!("{summary} The HTML is unchanged."),
                None => summary,
            };
            finish(&result, summary, owner.format.format, Path::new(""));
        }
        Command::Publish {
            args,
            server,
            choose_server,
            token,
            slug,
            new,
            visibility,
            format,
        } => {
            let server = crate::publish_destination::resolve(server, choose_server, format.format)?;
            let token = token.filter(|s| !s.is_empty()).ok_or_else(|| {
                Failure::usage(crate::publish_destination::missing_token(&server))
            })?;
            bound_token(&server, &token)?;
            let result = ops::publish_as(
                &args.read.input(&args.input),
                &server,
                &token,
                slug.as_deref(),
                new,
                visibility.as_deref(),
            )
            .await?;
            let summary = publish_summary(&result, &server);
            finish(&result, summary, format.format, &args.input);
        }
    }
    Ok(())
}

pub async fn publish(
    source: &str,
    options: &RenderOptions,
    server: &str,
    token: &str,
    slug: Option<&str>,
    new: bool,
) -> Result<Value, String> {
    publish_as(source, options, server, token, slug, new, None).await
}

/// Publish and, when `visibility` is given, set who can open the report.
pub async fn publish_as(
    source: &str,
    options: &RenderOptions,
    server: &str,
    token: &str,
    slug: Option<&str>,
    new: bool,
    visibility: Option<&str>,
) -> Result<Value, String> {
    if token.is_empty() {
        return Err("Missing API token".into());
    }
    let publish_diagnostics = richview_core::document::validate_publish(source);
    if hard_error(&publish_diagnostics) {
        return Err(diag_error(publish_diagnostics));
    }
    richview_core::document::validate_report_document(
        &richview_core::document::parse(source).map_err(diag_error)?,
        options.report_folder.as_ref(),
    )
    .map_err(diag_error)?;
    let request = richview_core::create_publish_request(source, slug).map_err(diag_error)?;
    // The files the source links to travel with it, so the links work once published (FEA-SF-52).
    let document = richview_core::document::parse(source).map_err(diag_error)?;
    let files = match options
        .report_folder
        .as_ref()
        .map(|f| PathBuf::from(&f.root))
    {
        Some(root) if root.is_dir() => resolver::read_linked_files(&root, &document)?,
        _ => Default::default(),
    };
    // The author's absolute path stays on the author's machine: the server gets the folder's name
    // and its inventory (TYP-65).
    let wire_folder = options.report_folder.as_ref().map(|folder| {
        let mut folder = folder.clone();
        folder.root = Path::new(&folder.root)
            .file_name()
            .map_or_else(|| "report".to_owned(), |n| n.to_string_lossy().into_owned());
        folder
    });
    let server = crate::remote::server_url(server)?;
    let body = serde_json::to_value(crate::server::PublishRequest {
        org: None,
        slug: request.slug.clone(),
        source: request.source,
        report_folder: wire_folder,
        styles: options.styles.clone(),
        data: options.data.clone(),
        files,
        new,
        visibility: visibility
            .map(|text| {
                crate::access::Visibility::parse(text).ok_or("Visibility is private, org or public")
            })
            .transpose()?,
    })
    .map_err(|error| error.to_string())?;
    let mut data = crate::remote::send(
        &server,
        token,
        reqwest::Method::POST,
        "/api/reports",
        Some(&body),
    )
    .await?;
    // The selected server remains the URL origin; hosted servers qualify the requested slug
    // with an organization. Validate that response before using it as a path.
    let returned_slug = data
        .get("slug")
        .and_then(Value::as_str)
        .unwrap_or(&request.slug);
    let valid = if let Some((org, slug)) = returned_slug.split_once('/') {
        slug == request.slug && crate::storage::ReportKey::hosted(org, slug).is_ok()
    } else {
        returned_slug == request.slug
    };
    if !valid {
        return Err("Server returned an invalid report address".into());
    }
    data["url"] = Value::String(
        server
            .join(&format!("/r/{returned_slug}"))
            .map_err(|e| e.to_string())?
            .to_string(),
    );
    data["server"] = Value::String(server.as_str().trim_end_matches('/').to_owned());
    if server.as_str().trim_end_matches('/') == "https://app.richview.ai" {
        data["operator"] = Value::String("RichView".into());
    }
    Ok(data)
}

pub fn read_source(path: &Path) -> Result<String, String> {
    resolver::report_folder(path)?;
    String::from_utf8(resolver::bounded_read(path)?).map_err(|e| e.to_string())
}
/// `RICHVIEW_TOKEN` belongs to `RICHVIEW_SERVER` and goes nowhere else (FEA-SF-12): another server
/// needs its own token, passed with `--token`.
fn bound_token(server: &str, token: &str) -> Result<(), Failure> {
    let from_env = std::env::var("RICHVIEW_TOKEN").is_ok_and(|t| t == token);
    let configured = std::env::var("RICHVIEW_SERVER").unwrap_or_default();
    let same = |a: &str, b: &str| a.trim_end_matches('/') == b.trim_end_matches('/');
    if from_env && !same(server, &configured) {
        return Err(Failure::usage(
            "RICHVIEW_TOKEN is only sent to RICHVIEW_SERVER. For another server, pass that server's own token with --token.",
        ));
    }
    Ok(())
}

/// The report, or the folder of reports, an input names.
fn report_target(text: &str) -> Result<Target, String> {
    Target::of(Path::new(text))
}
/// The one report an input names, for the commands that read exactly one.
fn one_report(text: &str) -> Result<PathBuf, String> {
    report_target(text)?.one("this command")
}
pub fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path))
    }
}
fn diag_error(d: Vec<Diagnostic>) -> String {
    serde_json::to_string(&d).unwrap_or_default()
}
/// Findings about how RichView rendered, with nothing for the author to do. They stay in the JSON
/// output and out of the lines a person reads.
const DEBUG_ONLY: &[&str] = &["assets.runtime-external"];

/// One line per diagnostic: `<file>:<line> <level> <code>: <message>`, or the block path when
/// there is no line. Identical `info` diagnostics collapse into one line with a count
/// (FEA-SF-07). An error is followed by its fix on an indented line, so the remedy the JSON
/// carries is not lost (INT-21).
pub fn diagnostics_text(diagnostics: &[Diagnostic], input: Option<&Path>) -> String {
    diagnostics_lines(diagnostics, input, None)
}

/// [`diagnostics_text`] with every file named from `folder`, the report's own, so the lines of
/// several reports stay apart.
fn diagnostics_lines(
    diagnostics: &[Diagnostic],
    input: Option<&Path>,
    folder: Option<&Path>,
) -> String {
    let default_file = input
        .map(|p| {
            p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        })
        .unwrap_or_default();
    let mut lines: Vec<(String, usize)> = Vec::new();
    for d in diagnostics {
        if DEBUG_ONLY.contains(&d.code.as_str()) {
            continue;
        }
        let file = d.file.clone().unwrap_or_else(|| default_file.clone());
        let file = match folder {
            Some(folder) if !file.is_empty() => folder.join(&file).display().to_string(),
            _ => file,
        };
        let mut line = match d.line {
            Some(line) => format!("{file}:{line} {} {}: {}", d.level, d.code, d.message),
            None if file.is_empty() => {
                format!("{} {}: {} ({})", d.level, d.code, d.message, d.path)
            }
            None => format!("{file} {} {}: {} ({})", d.level, d.code, d.message, d.path),
        };
        if let Some(fix) = d.fix.as_deref().filter(|_| d.level == "error") {
            line.push_str("\n  fix: ");
            line.push_str(fix);
        }
        match lines
            .iter_mut()
            .find(|(l, _)| *l == line && d.level == "info")
        {
            Some((_, count)) => *count += 1,
            None => lines.push((line, 1)),
        }
    }
    lines
        .into_iter()
        .map(|(line, count)| {
            if count > 1 {
                format!("{line} (×{count})")
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A failure in words: its diagnostics, or one sentence.
fn failure_text(failure: &Failure, input: Option<&Path>) -> String {
    match failure {
        Failure::Diagnostics(diagnostics) => diagnostics_text(diagnostics, input),
        other => format!("richview: {}", other.message()),
    }
}

pub fn hard_error(diagnostics: &[Diagnostic]) -> bool {
    ops::has_error(diagnostics)
}
/// Whether an output name asks for a PDF.
pub fn names_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn publish_summary_distinguishes_an_unchanged_republish() {
        let mut result = json!({"created": false, "url": "https://server.test/r/a", "version": "1.0.0", "visibility": "private", "permalink": "https://server.test/r/a/v/1.0.0"});
        assert!(
            publish_summary(&result, "https://server.test")
                .starts_with("Already published at https://server.test")
        );
        result["created"] = json!(true);
        assert!(
            publish_summary(&result, "https://server.test")
                .starts_with("Published to https://server.test")
        );
        result.as_object_mut().unwrap().remove("created");
        assert!(
            publish_summary(&result, "https://server.test")
                .starts_with("Server response from https://server.test")
        );
        result["operator"] = json!("RichView");
        assert!(
            publish_summary(&result, "https://app.richview.ai").contains("operated by RichView")
        );
    }

    #[test]
    fn human_lines_name_file_line_level_and_code_and_collapse_identical_infos() {
        let at = |line| {
            let mut d = Diagnostic::new("warning", "chart.clipping", "Clipped.", "chart");
            d.line = Some(line);
            d
        };
        let info = Diagnostic::new("info", "chart.binning", "Binned.", "chart.binning");
        let text = diagnostics_text(
            &[at(7), info.clone(), info],
            Some(Path::new("/x/report.rmd")),
        );
        assert_eq!(
            text,
            "report.rmd:7 warning chart.clipping: Clipped.\nreport.rmd info chart.binning: Binned. (chart.binning) (×2)"
        );
    }

    #[test]
    fn human_lines_give_an_error_its_fix_and_leave_out_render_notes() {
        let refused = Diagnostic::new("error", "publish.new-slug", "Nothing was stored.", "slug")
            .fix("Publish again with --new.");
        let note = Diagnostic::new(
            "info",
            "assets.runtime-external",
            "Preview uses a cached runtime.",
            "options.runtime_url",
        );
        assert_eq!(
            diagnostics_text(&[note.clone(), refused], Some(Path::new("report.rmd"))),
            "report.rmd error publish.new-slug: Nothing was stored. (slug)\n  fix: Publish again with --new."
        );
        assert_eq!(diagnostics_text(&[note], Some(Path::new("report.rmd"))), "");
    }
    use axum::{Json, Router, http::HeaderMap, routing::post};
    use std::sync::Mutex;

    #[tokio::test]
    async fn publish_uses_the_server_qualified_hosted_address() {
        let app = Router::new().route("/api/reports", post(|| async {
            Json(json!({"created":true,"slug":"aaaa0001/example","url":"https://untrusted.test/r/wrong"}))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let source = "---\ntitle: Example\ndescription: Test\nversion: 1.0.0\n---\n\n# Example\n";
        let options = RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "fixture".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        };
        let result = publish(
            source,
            &options,
            &server,
            "local-token",
            Some("example"),
            true,
        )
        .await
        .unwrap();
        assert_eq!(result["url"], format!("{server}/r/aaaa0001/example"));
        worker.abort();
    }

    #[tokio::test]
    async fn publish_preflight_preserves_shared_slug_and_resolved_source_request() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let captured = received.clone();
        let app = Router::new().route(
            "/api/reports",
            post(move |headers: HeaderMap, Json(request): Json<Value>| {
                let captured = captured.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer local-token");
                    captured.lock().unwrap().push(request);
                    Json(json!({"created":true}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let source = "---\r\ntitle: İstanbul grows\r\ndescription: The city's visits\r\nversion: 0.0.1\r\nslug: authored\r\n---\r\n\r\n```chart\r\ntype: line\r\ntitle: Visits grew\r\ndata: {csv: visits.csv}\r\nencoding: {x: month, y: value}\r\n```\r\n";
        let options = RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "publish-fixture".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into(), "visits.csv".into()],
            }),
            styles: vec!["scientific".into()],
            data: [("visits".into(), "month,value\n2026-01,42\n".into())].into(),
            // Runtime code belongs to the publishing host and is never sent by this client.
            runtime_js: Some("client-runtime-must-not-be-sent".into()),
            ..Default::default()
        };
        for (override_slug, expected) in [(None, "authored"), (Some("explicit"), "explicit")] {
            let response = publish(
                source,
                &options,
                &server,
                "local-token",
                override_slug,
                true,
            )
            .await
            .unwrap();
            assert_eq!(response["url"], format!("{server}/r/{expected}"));
        }
        let derived_source = source.replace("slug: authored\r\n", "");
        let derived = publish(
            &derived_source,
            &options,
            &server,
            "local-token",
            None,
            true,
        )
        .await
        .unwrap();
        assert_eq!(derived["url"], format!("{server}/r/istanbul-grows"));
        assert!(
            publish("# draft", &options, &server, "local-token", None, true)
                .await
                .is_err()
        );
        assert!(
            publish(
                source,
                &options,
                &server,
                "local-token",
                Some("../escape"),
                true
            )
            .await
            .is_err()
        );
        let received = received.lock().unwrap();
        assert_eq!(
            received.len(),
            3,
            "invalid metadata and slugs never reach the host"
        );
        assert_eq!(
            received[0],
            json!({"slug":"authored","source":source,"styles":options.styles,"data":options.data,"reportFolder":options.report_folder,"files":{},"new":true})
        );
        assert_eq!(received[1]["slug"], "explicit");
        assert_eq!(received[2]["slug"], "istanbul-grows");
        worker.abort();
    }
}
