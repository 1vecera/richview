//! Bounded transactional email delivery for hosted verification links.
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::MultiPart,
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    future::Future,
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// The same message as HTML. SMTP sends it beside `body` as `multipart/alternative`; the
    /// manual outbox shows `body` only.
    pub html: Option<String>,
}

struct QueuedMail {
    mail: Mail,
    manual: Option<ManualRequest>,
    deliver: bool,
    journal: Option<JournalEntry>,
}

enum QueueItem {
    Mail(Box<QueuedMail>),
    Barrier(tokio::sync::oneshot::Sender<()>),
}

/// A manual delivery request contains no link. A fresh link is issued at dispatch.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualRequest {
    pub id: String,
    pub to: String,
    pub kind: ManualKind,
    pub created: u64,
    pub expires: u64,
    pub status: ManualStatus,
    /// The invitation an `invite` request delivers; its link is issued at dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation: Option<String>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualKind {
    Signup,
    Reset,
    AlreadyRegistered,
    /// An invitation to an organization or to one shared report.
    Invite,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualStatus {
    Pending,
    Dispatched,
    Expired,
    Decoy,
}
fn seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// Older outbox copies have no sender field; recognize the two hosted mail texts they contain.
fn invitation_from(body: &str, email: &str) -> bool {
    let body = body.to_ascii_lowercase();
    let email = email.to_ascii_lowercase();
    body.contains(&format!("({email}) invited you to join "))
        || body.contains(&format!("({email}) shared “"))
}

pub trait Transport: Send + Sync {
    /// The configured sender, when mail is delivered automatically.
    fn sender(&self) -> Option<String> {
        None
    }
    fn durable(&self) -> bool {
        false
    }
    fn waiting_summary(&self) -> Result<(usize, Option<u64>), String> {
        Ok((0, None))
    }
    fn send<'a>(
        &'a self,
        mail: &'a Mail,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>>;
    fn maintenance(&self) -> Result<(), String> {
        Ok(())
    }
    fn write_manual(&self, _request: &ManualRequest) -> Result<(), String> {
        Err("Manual outbox is not available".into())
    }
    fn write_manual_charged(
        &self,
        request: &ManualRequest,
        charge: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        if request.status == ManualStatus::Pending {
            charge()?;
        }
        self.write_manual(request)
    }
    fn pending_manual(&self) -> Result<Vec<ManualRequest>, String> {
        Err("Manual outbox is not available".into())
    }
    /// Remove local delivery records for an address; network transports retain none.
    fn purge_address(&self, _email: &str) -> Result<(), String> {
        Ok(())
    }
    /// Remove requests for invitations a deleted account issued, and its saved mail copies.
    fn purge_invitations(&self, _email: &str, _invitations: &[String]) -> Result<(), String> {
        Ok(())
    }
    fn dispatch_manual(
        &self,
        _id: &str,
        _issue: &mut dyn FnMut(&ManualRequest) -> Result<Mail, String>,
    ) -> Result<Mail, String> {
        Err("Manual outbox is not available".into())
    }
}

/// Private local transport. Manual requests contain no links; ordinary test mail may contain them.
#[derive(Clone)]
pub struct Outbox {
    dir: PathBuf,
    manual_lock: Arc<Mutex<()>>,
}
impl Outbox {
    pub fn open(dir: &Path) -> Result<Self, String> {
        crate::access::create_durable_directory(dir)?;
        let meta = fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("The outbox must be a real directory".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            dir: dir.to_owned(),
            manual_lock: Arc::new(Mutex::new(())),
        })
    }
    fn manual_path(&self, id: &str) -> Result<PathBuf, String> {
        if id.len() != 32
            || !id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("Invalid manual request id".into());
        }
        Ok(self.dir.join(format!("{id}.request.json")))
    }
    fn save_manual(&self, request: &ManualRequest) -> Result<(), String> {
        let path = self.manual_path(&request.id)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        serde_json::to_writer(&mut file, request).map_err(|e| e.to_string())?;
        file.flush()
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist(path).map_err(|e| e.error.to_string())?;
        crate::access::sync_directory(&self.dir)?;
        Ok(())
    }
    fn read_manual(&self, path: &Path) -> Result<ManualRequest, String> {
        let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !metadata.file_type().is_file() || metadata.len() > 4096 {
            return Err("Invalid manual request file".into());
        }
        let request: ManualRequest =
            serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if self.manual_path(&request.id)? != path {
            return Err("Invalid manual request id".into());
        }
        Ok(request)
    }
    fn records(&self) -> Result<Vec<ManualRequest>, String> {
        let mut rows = Vec::new();
        for item in fs::read_dir(&self.dir).map_err(|e| e.to_string())? {
            let path = item.map_err(|e| e.to_string())?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.ends_with(".txt") || name.starts_with(".tmp") {
                let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if !metadata.file_type().is_file() {
                    return Err("Invalid outbox message".into());
                }
                let modified = metadata.modified().map_err(|e| e.to_string())?;
                if modified.elapsed().unwrap_or_default() >= Duration::from_secs(7 * 86400 - 120) {
                    fs::remove_file(&path).map_err(|e| e.to_string())?;
                    crate::access::sync_directory(&self.dir)?;
                }
                continue;
            }
            if !name.ends_with(".request.json") {
                continue;
            }
            let mut request = self.read_manual(&path)?;
            if request.status == ManualStatus::Pending && request.expires <= seconds() {
                request.status = ManualStatus::Expired;
                self.save_manual(&request)?;
            }
            if request.created <= seconds().saturating_sub(7 * 86400 - 120)
                && request.status != ManualStatus::Pending
            {
                fs::remove_file(&path).map_err(|e| e.to_string())?;
                crate::access::sync_directory(&self.dir)?;
                continue;
            }
            rows.push(request);
        }
        Ok(rows)
    }
    fn write(&self, mail: &Mail) -> Result<(), String> {
        if mail.to.contains(['\r', '\n']) || mail.subject.contains(['\r', '\n']) {
            return Err("Invalid mail header".into());
        }
        let mut file = tempfile::NamedTempFile::new_in(&self.dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        write!(
            file,
            "To: {}\nSubject: {}\n\n{}",
            mail.to, mail.subject, mail.body
        )
        .map_err(|e| e.to_string())?;
        file.flush()
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|e| e.to_string())?;
        let name: String = random.iter().map(|b| format!("{b:02x}")).collect();
        file.persist_noclobber(self.dir.join(format!("{name}.txt")))
            .map_err(|e| e.error.to_string())?;
        Ok(())
    }
}
impl Transport for Outbox {
    fn waiting_summary(&self) -> Result<(usize, Option<u64>), String> {
        let pending = self.pending_manual()?;
        Ok((
            pending.len(),
            pending
                .iter()
                .map(|request| seconds().saturating_sub(request.created))
                .max(),
        ))
    }
    fn maintenance(&self) -> Result<(), String> {
        let _guard = self
            .manual_lock
            .lock()
            .map_err(|_| "Manual outbox lock poisoned")?;
        self.records().map(|_| ())
    }
    fn purge_address(&self, email: &str) -> Result<(), String> {
        let _guard = self
            .manual_lock
            .lock()
            .map_err(|_| "Manual outbox lock poisoned")?;
        for entry in fs::read_dir(&self.dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let matches = if name.ends_with(".request.json") {
                self.read_manual(&path)?.to.eq_ignore_ascii_case(email)
            } else if name.ends_with(".txt") || name.starts_with(".tmp") {
                let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if !metadata.file_type().is_file() || metadata.len() > 1024 * 1024 {
                    return Err("Invalid outbox message".into());
                }
                let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                if name.starts_with(".tmp") {
                    text.to_ascii_lowercase()
                        .contains(&email.to_ascii_lowercase())
                } else {
                    text.lines()
                        .next()
                        .and_then(|line| line.strip_prefix("To: "))
                        .is_some_and(|to| to.eq_ignore_ascii_case(email))
                }
            } else {
                false
            };
            if matches {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    fn purge_invitations(&self, email: &str, invitations: &[String]) -> Result<(), String> {
        let _guard = self
            .manual_lock
            .lock()
            .map_err(|_| "Manual outbox lock poisoned")?;
        let mut removed = false;
        for entry in fs::read_dir(&self.dir).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Invalid outbox name")?;
            let matches = if name.ends_with(".request.json") {
                self.read_manual(&path)?
                    .invitation
                    .as_ref()
                    .is_some_and(|id| invitations.contains(id))
            } else if name.ends_with(".txt") || name.starts_with(".tmp") {
                let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if !metadata.file_type().is_file() || metadata.len() > 1024 * 1024 {
                    return Err("Invalid outbox message".into());
                }
                let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                let mut issued = invitation_from(&text, email);
                if name.starts_with(".tmp") && !issued {
                    issued = serde_json::from_str::<ManualRequest>(&text)
                        .ok()
                        .and_then(|request| request.invitation)
                        .is_some_and(|id| invitations.contains(&id));
                    // A crash can leave JSON cut short after the invitation field was written.
                    for id in invitations {
                        if issued {
                            break;
                        }
                        let encoded = serde_json::to_string(id).map_err(|e| e.to_string())?;
                        issued |= text.contains(&format!("\"invitation\":{encoded}"));
                    }
                }
                issued
            } else {
                false
            };
            if matches {
                fs::remove_file(path).map_err(|e| e.to_string())?;
                removed = true;
            }
        }
        if removed {
            crate::access::sync_directory(&self.dir)?;
        }
        Ok(())
    }
    fn write_manual(&self, request: &ManualRequest) -> Result<(), String> {
        self.write_manual_charged(request, &mut || Ok(()))
    }
    fn write_manual_charged(
        &self,
        request: &ManualRequest,
        charge: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        let _guard = self
            .manual_lock
            .lock()
            .map_err(|_| "Manual outbox lock poisoned")?;
        if crate::email::normalize(&request.to).as_deref() != Some(&request.to) {
            return Err("Invalid email".into());
        }
        if self.manual_path(&request.id)?.exists() {
            // A preceding rename may have succeeded even if its directory sync failed.
            crate::access::sync_directory(&self.dir)?;
            return Err("Manual request already exists".into());
        }
        let at = seconds();
        let rows = self.records()?;
        // Anonymous signup requests and reset decoys may use only the first half of history.
        // Existing-account resets and invitations can use the reserved remainder.
        let anonymous = matches!(
            request.kind,
            ManualKind::Signup | ManualKind::AlreadyRegistered
        ) || request.status == ManualStatus::Decoy;
        // Keep a legacy outbox that already holds 5,000 anonymous requests while reserving the
        // other half for existing accounts; do not delete pending links or their rate history.
        let history_limit = if anonymous { 5_000 } else { 10_000 };
        if rows
            .iter()
            .filter(|r| r.created > at.saturating_sub(86400))
            .count()
            >= history_limit
            || rows
                .iter()
                .filter(|r| r.to == request.to && r.created > at.saturating_sub(86400))
                .count()
                >= 10
            || rows
                .iter()
                .filter(|r| r.to == request.to && r.created > at.saturating_sub(3600))
                .count()
                >= 3
        {
            return Err("Manual request limit reached".into());
        }
        if request.status == ManualStatus::Pending {
            charge()?;
        }
        self.save_manual(request)
    }
    fn pending_manual(&self) -> Result<Vec<ManualRequest>, String> {
        let _guard = self
            .manual_lock
            .lock()
            .map_err(|_| "Manual outbox lock poisoned")?;
        Ok(self
            .records()?
            .into_iter()
            .filter(|r| r.status == ManualStatus::Pending)
            .collect())
    }
    fn dispatch_manual(
        &self,
        id: &str,
        issue: &mut dyn FnMut(&ManualRequest) -> Result<Mail, String>,
    ) -> Result<Mail, String> {
        let _guard = self
            .manual_lock
            .lock()
            .map_err(|_| "Manual outbox lock poisoned")?;
        let mut request = self.read_manual(&self.manual_path(id)?)?;
        if request.status != ManualStatus::Pending {
            return Err("Manual request is no longer pending".into());
        }
        if request.expires <= seconds() {
            request.status = ManualStatus::Expired;
            self.save_manual(&request)?;
            return Err("Manual request expired".into());
        }
        let mail = issue(&request)?;
        request.status = ManualStatus::Dispatched;
        self.save_manual(&request)?;
        Ok(mail)
    }
    fn send<'a>(
        &'a self,
        mail: &'a Mail,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
        let outbox = self.clone();
        let mail = mail.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || outbox.write(&mail).map_err(|_| ()))
                .await
                .map_err(|_| ())?
        })
    }
}

/// SMTP uses either implicit TLS or mandatory STARTTLS; plaintext is never an option.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum SmtpTls {
    Implicit,
    #[value(name = "starttls")]
    StartTls,
}

impl SmtpTls {
    pub fn for_port(port: u16, explicit: Option<Self>) -> Result<Self, String> {
        let standard = match port {
            465 => Some(Self::Implicit),
            587 => Some(Self::StartTls),
            0 => return Err("SMTP port must be nonzero".into()),
            _ => None,
        };
        match (standard, explicit) {
            (Some(expected), Some(given)) if expected != given => {
                Err("SMTP TLS mode conflicts with its standard port".into())
            }
            (Some(mode), _) | (None, Some(mode)) => Ok(mode),
            (None, None) => {
                Err("Set --smtp-tls to implicit or starttls for a nonstandard SMTP port".into())
            }
        }
    }
}

pub struct Smtp {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: lettre::message::Mailbox,
}
impl Smtp {
    pub fn new(
        host: &str,
        port: u16,
        username: String,
        password: String,
        from: String,
        tls: Option<SmtpTls>,
    ) -> Result<Self, String> {
        Self::build(host, port, username, password, from, tls, None)
    }

    fn build(
        host: &str,
        port: u16,
        username: String,
        password: String,
        from: String,
        tls: Option<SmtpTls>,
        parameters: Option<TlsParameters>,
    ) -> Result<Self, String> {
        let mailbox: lettre::message::Mailbox =
            from.parse().map_err(|_| "Invalid sender address")?;
        let from = lettre::message::Mailbox::new(Some("RichView".into()), mailbox.email);
        let mode = SmtpTls::for_port(port, tls)?;
        let mut builder = match mode {
            SmtpTls::Implicit => AsyncSmtpTransport::<Tokio1Executor>::relay(host),
            SmtpTls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host),
        }
        .map_err(|_| "Invalid SMTP relay")?;
        if let Some(parameters) = parameters {
            builder = builder.tls(match mode {
                SmtpTls::Implicit => Tls::Wrapper(parameters),
                SmtpTls::StartTls => Tls::Required(parameters),
            });
        }
        let transport = builder
            .port(port)
            .credentials(Credentials::new(username, password))
            .build();
        Ok(Self { transport, from })
    }
}
impl Transport for Smtp {
    fn sender(&self) -> Option<String> {
        Some(self.from.email.to_string())
    }

    fn durable(&self) -> bool {
        true
    }
    fn send<'a>(
        &'a self,
        mail: &'a Mail,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
        Box::pin(async move {
            let address = crate::email::normalize(&mail.to)
                .filter(|email| email == &mail.to)
                .ok_or(())?;
            let recipient = lettre::message::Mailbox::new(None, address.parse().map_err(|_| ())?);
            let builder = Message::builder()
                .from(self.from.clone())
                .to(recipient)
                .subject(&mail.subject);
            let message = match &mail.html {
                Some(html) => builder.multipart(MultiPart::alternative_plain_html(
                    mail.body.clone(),
                    html.clone(),
                )),
                None => builder.body(mail.body.clone()),
            }
            .map_err(|_| ())?;
            tokio::time::timeout(Duration::from_secs(20), self.transport.send(message))
                .await
                .map_err(|_| ())?
                .map(|_| ())
                .map_err(|_| ())
        })
    }
}

// The journal extends the manual outbox request record with the message and its link.
#[derive(Clone, Serialize, Deserialize)]
struct JournalEntry {
    #[serde(flatten)]
    request: ManualRequest,
    subject: String,
    body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    html: Option<String>,
    token: Option<String>,
}

type LinkCheck = Arc<dyn Fn(&str) -> Option<u64> + Send + Sync>;

#[derive(Clone, Default, Serialize, Deserialize)]
struct SentLedger {
    count: u64,
    settling: Vec<String>,
}
struct Journal {
    outbox: Outbox,
    lock: Mutex<SentLedger>,
    uncertain: AtomicBool,
}
impl Journal {
    fn open(access_dir: &Path) -> Result<Self, String> {
        let outbox = Outbox::open(&access_dir.join("smtp-outbox"))?;
        crate::access::sync_directory(access_dir)?;
        let path = outbox.dir.join("sent.json");
        let ledger = match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_file() && meta.len() <= 1024 * 1024 => {
                serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|_| "Invalid mail sent counter")?
            }
            Ok(_) => return Err("Invalid mail sent counter".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => SentLedger::default(),
            Err(e) => return Err(e.to_string()),
        };
        Ok(Self {
            outbox,
            lock: Mutex::new(ledger),
            uncertain: AtomicBool::new(false),
        })
    }
    fn sync_dir(&self) -> Result<(), String> {
        crate::access::sync_directory(&self.outbox.dir)
    }
    fn save_ledger(&self, ledger: &SentLedger) -> Result<(), String> {
        let mut file =
            tempfile::NamedTempFile::new_in(&self.outbox.dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        serde_json::to_writer(&mut file, ledger).map_err(|e| e.to_string())?;
        file.flush()
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist(self.outbox.dir.join("sent.json"))
            .map_err(|e| e.error.to_string())?;
        self.sync_dir()
    }
    fn settle(&self, id: &str) -> Result<(), String> {
        let mut ledger = self.lock.lock().map_err(|_| "Mail journal lock poisoned")?;
        let mut next = ledger.clone();
        next.count = next.count.checked_add(1).ok_or("Mail counter overflow")?;
        next.settling.push(id.to_owned());
        if let Err(error) = self.save_ledger(&next) {
            self.uncertain.store(true, Ordering::Release);
            return Err(error);
        }
        *ledger = next;
        // The counter and ID are durable first. If removal fails, the marker prevents replay.
        let removed = match fs::remove_file(self.outbox.manual_path(id)?) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => false,
        };
        if removed && self.sync_dir().is_ok() {
            let mut next = ledger.clone();
            next.settling.retain(|settled| settled != id);
            if self.save_ledger(&next).is_ok() {
                *ledger = next;
            }
        }
        Ok(())
    }
    fn save(&self, entry: &JournalEntry) -> Result<(), String> {
        let _lock = self.lock.lock().map_err(|_| "Mail journal lock poisoned")?;
        let path = self.outbox.manual_path(&entry.request.id)?;
        let mut file =
            tempfile::NamedTempFile::new_in(&self.outbox.dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        serde_json::to_writer(&mut file, entry).map_err(|e| e.to_string())?;
        file.flush()
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist_noclobber(path)
            .map_err(|e| e.error.to_string())?;
        self.sync_dir()
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        let _lock = self.lock.lock().map_err(|_| "Mail journal lock poisoned")?;
        match fs::remove_file(self.outbox.manual_path(id)?) {
            Ok(()) => self.sync_dir(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    fn entries(&self) -> Result<Vec<JournalEntry>, String> {
        self.scan().map(|(entries, _)| entries)
    }
    fn scan(&self) -> Result<(Vec<JournalEntry>, u64), String> {
        if self.uncertain.load(Ordering::Acquire) {
            return Err("Mail counter unavailable until restart".into());
        }
        let mut ledger = self.lock.lock().map_err(|_| "Mail journal lock poisoned")?;
        if self.uncertain.load(Ordering::Acquire) {
            return Err("Mail counter unavailable until restart".into());
        }
        let mut entries = Vec::new();
        let mut removed = Vec::new();
        for item in fs::read_dir(&self.outbox.dir).map_err(|e| e.to_string())? {
            let path = item.map_err(|e| e.to_string())?.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return Err("Invalid journal name".into());
            };
            if name.starts_with(".tmp") {
                fs::remove_file(path).map_err(|e| e.to_string())?;
                self.sync_dir()?;
                continue;
            }
            if !name.ends_with(".request.json") {
                continue;
            }
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if !meta.file_type().is_file() || meta.len() > 1024 * 1024 {
                return Err("Invalid journal file".into());
            }
            let entry: JournalEntry =
                serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|_| "Invalid journal record")?;
            if self.outbox.manual_path(&entry.request.id)? != path {
                return Err("Invalid journal id".into());
            }
            if ledger.settling.contains(&entry.request.id) {
                fs::remove_file(&path).map_err(|e| e.to_string())?;
                self.sync_dir()?;
                removed.push(entry.request.id);
            } else {
                entries.push(entry);
            }
        }
        if !removed.is_empty() {
            let mut next = ledger.clone();
            next.settling.retain(|id| !removed.contains(id));
            if self.save_ledger(&next).is_ok() {
                *ledger = next;
            }
        }
        Ok((entries, ledger.count))
    }
    // Expiration must not depend on whether the sent counter is trustworthy.
    fn expire_entries(&self, checker: Option<&LinkCheck>) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|_| "Mail journal lock poisoned")?;
        let mut removed = false;
        for item in fs::read_dir(&self.outbox.dir).map_err(|e| e.to_string())? {
            let path = item.map_err(|e| e.to_string())?.path();
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".request.json"))
            {
                continue;
            }
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if !meta.file_type().is_file() || meta.len() > 1024 * 1024 {
                return Err("Invalid journal file".into());
            }
            let entry: JournalEntry =
                serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|_| "Invalid journal record")?;
            if self.outbox.manual_path(&entry.request.id)? != path {
                return Err("Invalid journal id".into());
            }
            if entry.request.expires <= seconds()
                || entry
                    .token
                    .as_ref()
                    .is_some_and(|token| checker.is_none_or(|check| check(token).is_none()))
            {
                fs::remove_file(path).map_err(|e| e.to_string())?;
                removed = true;
            }
        }
        if removed {
            self.sync_dir()?;
        }
        Ok(())
    }
    fn start_cleanup(journal: &Arc<Self>, checker: Option<LinkCheck>, interval: Duration) {
        let journal = Arc::downgrade(journal);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(journal) = journal.upgrade() else {
                    break;
                };
                let checker = checker.clone();
                let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                    journal.expire_entries(checker.as_ref())?;
                    if !journal.uncertain.load(Ordering::Acquire) {
                        journal.scan()?;
                    }
                    Ok(())
                })
                .await;
                if !matches!(result, Ok(Ok(()))) {
                    eprintln!("richview: SMTP journal cleanup failed");
                }
            }
        });
    }
    fn purge_address(&self, email: &str) -> Result<(), String> {
        for entry in self.entries()? {
            if entry.request.to.eq_ignore_ascii_case(email) || invitation_from(&entry.body, email) {
                self.remove(&entry.request.id)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Budget {
    day: u64,
    sent: u64,
}
fn day() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400
}

pub(crate) type AccessMaintenance = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

pub(crate) fn start_transport_cleanup(
    transport: &Arc<dyn Transport>,
    interval: Duration,
    maintenance: Option<&AccessMaintenance>,
) {
    let transport = Arc::downgrade(transport);
    let maintenance = maintenance.map(Arc::downgrade);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            let Some(transport) = transport.upgrade() else {
                break;
            };
            let maintenance = match &maintenance {
                Some(maintenance) => match maintenance.upgrade() {
                    Some(maintenance) => Some(maintenance),
                    None => break,
                },
                None => None,
            };
            let result = tokio::task::spawn_blocking(move || {
                let mail = transport.maintenance();
                let access = maintenance.map_or(Ok(()), |maintenance| maintenance());
                mail.and(access)
            })
            .await;
            if !matches!(result, Ok(Ok(()))) {
                eprintln!("richview: hosted mail cleanup failed");
            }
        }
    });
}

/// Reserves the queue slot and durable daily budget before accepting a message.
#[derive(Clone)]
pub struct MailQueue {
    tx: mpsc::Sender<QueueItem>,
    budget: Arc<Mutex<Budget>>,
    path: PathBuf,
    limit: u64,
    admission: Arc<tokio::sync::Semaphore>,
    transport: Arc<dyn Transport>,
    _access_maintenance: Option<AccessMaintenance>,
    journal: Option<Arc<Journal>>,
    link_check: Option<LinkCheck>,
    sent_since_start: Arc<AtomicU64>,
    stop: tokio::sync::watch::Sender<bool>,
    worker: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}
impl MailQueue {
    pub(crate) fn sender(&self) -> Option<String> {
        self.transport.sender()
    }

    pub fn start(
        transport: Arc<dyn Transport>,
        access_dir: &Path,
        limit: u64,
    ) -> Result<Self, String> {
        Self::start_with_links(transport, access_dir, limit, None, None)
    }
    pub(crate) fn start_with_links(
        transport: Arc<dyn Transport>,
        access_dir: &Path,
        limit: u64,
        link_check: Option<LinkCheck>,
        access_maintenance: Option<AccessMaintenance>,
    ) -> Result<Self, String> {
        if limit == 0 {
            return Err("Mail budget must be positive".into());
        }
        let journal = if transport.durable() {
            Some(Arc::new(Journal::open(access_dir)?))
        } else {
            None
        };
        let mut replay = Vec::new();
        if let Some(journal) = &journal {
            for entry in journal.entries()? {
                let live = entry.request.expires > seconds()
                    && entry.token.as_ref().is_none_or(|token| {
                        link_check
                            .as_ref()
                            .is_some_and(|check| check(token).is_some())
                    });
                if live && entry.request.status == ManualStatus::Pending {
                    replay.push(entry);
                } else {
                    journal.remove(&entry.request.id)?;
                }
            }
        }
        let path = access_dir.join("mail-budget.json");
        let budget = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "Invalid mail budget")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Budget {
                day: day(),
                sent: 0,
            },
            Err(e) => return Err(e.to_string()),
        };
        transport.maintenance()?;
        start_transport_cleanup(
            &transport,
            Duration::from_secs(60),
            access_maintenance.as_ref(),
        );
        if let Some(journal) = &journal {
            Journal::start_cleanup(journal, link_check.clone(), Duration::from_secs(60));
        }
        let (tx, mut rx) = mpsc::channel::<QueueItem>(128);
        let sender = transport.clone();
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let worker_journal = journal.clone();
        let worker_check = link_check.clone();
        let sent_since_start = Arc::new(AtomicU64::new(0));
        let sent_counter = sent_since_start.clone();
        let worker = tokio::spawn(async move {
            let mut replay = replay.into_iter();
            loop {
                if *stopped.borrow() {
                    break;
                }
                let item = if let Some(entry) = replay.next() {
                    let mail = Mail {
                        to: entry.request.to.clone(),
                        subject: entry.subject.clone(),
                        body: entry.body.clone(),
                        html: entry.html.clone(),
                    };
                    QueueItem::Mail(Box::new(QueuedMail {
                        mail,
                        manual: None,
                        deliver: true,
                        journal: Some(entry),
                    }))
                } else {
                    tokio::select! {
                        biased;
                        _ = stopped.changed() => break,
                        item = rx.recv() => match item { Some(item) => item, None => break },
                    }
                };
                let queued = match item {
                    QueueItem::Mail(mail) => mail,
                    QueueItem::Barrier(done) => {
                        let _ = done.send(());
                        continue;
                    }
                };
                if let Some(entry) = &queued.journal {
                    let live = entry.request.expires > seconds()
                        && entry.token.as_ref().is_none_or(|token| {
                            worker_check
                                .as_ref()
                                .is_some_and(|check| check(token).is_some())
                        });
                    if !live {
                        if let Some(journal) = &worker_journal {
                            let _ = journal.remove(&entry.request.id);
                        }
                        continue;
                    }
                }
                // Manual decoys must retain the same per-address rate history, without appearing
                // in the operator's pending list or generating any message.
                if !queued.deliver && queued.manual.is_none() {
                    if let (Some(journal), Some(entry)) = (&worker_journal, &queued.journal)
                        && journal.remove(&entry.request.id).is_err()
                    {
                        eprintln!("richview: hosted mail journal removal failed");
                    }
                    continue;
                }
                let mut delivered = false;
                for attempt in 0..if queued.journal.is_some() { 1 } else { 3 } {
                    if sender.send(&queued.mail).await.is_ok() {
                        delivered = true;
                        break;
                    }
                    if attempt < 2 && queued.journal.is_none() {
                        tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                    }
                }
                if delivered
                    && let (Some(journal), Some(entry)) = (&worker_journal, &queued.journal)
                {
                    sent_counter.fetch_add(1, Ordering::Relaxed);
                    if journal.settle(&entry.request.id).is_err() {
                        eprintln!("richview: hosted mail settlement failed; restart required");
                        break;
                    }
                }
                if !delivered {
                    if queued.journal.is_some() {
                        eprintln!("richview: hosted mail delivery failed; queued for restart");
                    } else {
                        eprintln!("richview: hosted mail delivery failed after three attempts");
                    }
                }
            }
        });
        Ok(Self {
            tx,
            budget: Arc::new(Mutex::new(budget)),
            path,
            limit,
            admission: Arc::new(tokio::sync::Semaphore::new(8)),
            transport,
            _access_maintenance: access_maintenance,
            journal,
            link_check,
            sent_since_start,
            stop,
            worker: Arc::new(tokio::sync::Mutex::new(Some(worker))),
        })
    }
    pub(crate) fn begin_shutdown(&self) {
        self.admission.close();
        let _ = self.stop.send(true);
    }
    pub(crate) async fn finish_shutdown(&self, deadline: tokio::time::Instant) {
        if let Some(mut worker) = self.worker.lock().await.take()
            && tokio::time::timeout_at(deadline, &mut worker)
                .await
                .is_err()
        {
            worker.abort();
            let _ = worker.await;
        }
    }
    pub(crate) fn sent_since_start(&self) -> u64 {
        self.sent_since_start.load(Ordering::Relaxed)
    }
    pub(crate) fn summary(&self) -> Result<(usize, Option<u64>, u64), String> {
        let Some(journal) = &self.journal else {
            let (waiting, oldest) = self.transport.waiting_summary()?;
            return Ok((waiting, oldest, 0));
        };
        let (entries, total) = journal.scan()?;
        let entries: Vec<_> = entries
            .into_iter()
            .filter(|entry| {
                entry.request.status == ManualStatus::Pending
                    && entry.request.expires > seconds()
                    && entry.token.as_ref().is_none_or(|token| {
                        self.link_check
                            .as_ref()
                            .is_some_and(|check| check(token).is_some())
                    })
            })
            .collect();
        let oldest = entries.iter().map(|entry| entry.request.created).min();
        Ok((
            entries.len(),
            oldest.map(|at| seconds().saturating_sub(at)),
            total,
        ))
    }
    #[cfg(test)]
    fn pending_summary(&self) -> Result<(usize, Option<u64>), String> {
        self.summary().map(|(waiting, oldest, _)| (waiting, oldest))
    }
    pub fn exhausted(&self) -> bool {
        self.budget.lock().map_or(true, |budget| {
            budget.day == day() && budget.sent >= self.limit
        })
    }
    pub fn signup_exhausted(&self) -> bool {
        self.budget.lock().map_or(true, |budget| {
            budget.day == day() && budget.sent >= self.limit / 2
        })
    }
    pub async fn enqueue(&self, mail: Mail) -> Result<(), String> {
        self.enqueue_inner(mail, None, true, None, false).await
    }
    pub(crate) async fn enqueue_link(&self, mail: Mail, token: String) -> Result<(), String> {
        self.enqueue_inner(mail, None, true, Some(token), false)
            .await
    }

    pub(crate) async fn enqueue_signup(&self, mail: Mail, token: String) -> Result<(), String> {
        self.enqueue_inner(mail, None, true, Some(token), true)
            .await
    }

    /// Do the same bounded queue work for a nonexistent reset address, without mail.
    pub(crate) async fn enqueue_decoy(&self, mail: Mail, token: String) -> Result<(), String> {
        self.enqueue_inner(mail, None, false, Some(token), false)
            .await
    }

    pub async fn enqueue_manual(
        &self,
        to: String,
        kind: ManualKind,
        deliver: bool,
    ) -> Result<(), String> {
        self.enqueue_manual_for(to, kind, deliver, None).await
    }

    /// Queue an invitation for delivery by hand; the request names it and holds no link.
    pub(crate) async fn enqueue_manual_invitation(
        &self,
        to: String,
        invitation: String,
    ) -> Result<(), String> {
        self.enqueue_manual_for(to, ManualKind::Invite, true, Some(invitation))
            .await
    }

    async fn enqueue_manual_for(
        &self,
        to: String,
        kind: ManualKind,
        deliver: bool,
        invitation: Option<String>,
    ) -> Result<(), String> {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|e| e.to_string())?;
        let id = random.iter().map(|b| format!("{b:02x}")).collect();
        let created = seconds();
        let request = ManualRequest {
            id,
            to: to.clone(),
            kind,
            created,
            expires: created + 86400,
            status: if deliver {
                ManualStatus::Pending
            } else {
                ManualStatus::Decoy
            },
            invitation,
        };
        let signup = matches!(kind, ManualKind::Signup | ManualKind::AlreadyRegistered);
        self.enqueue_inner(
            Mail {
                to,
                subject: String::new(),
                body: String::new(),
                html: None,
            },
            Some(request),
            deliver,
            None,
            signup,
        )
        .await
    }
    pub fn pending_manual(&self) -> Result<Vec<ManualRequest>, String> {
        self.transport.pending_manual()
    }
    /// Wait for earlier queue work before an account's delivery records are removed.
    pub(crate) async fn flush(&self) -> Result<(), String> {
        let (done, received) = tokio::sync::oneshot::channel();
        self.tx
            .send(QueueItem::Barrier(done))
            .await
            .map_err(|_| "Mail queue unavailable")?;
        received.await.map_err(|_| "Mail queue unavailable".into())
    }

    pub(crate) fn purge_address(&self, email: &str) -> Result<(), String> {
        if let Some(journal) = &self.journal {
            journal.purge_address(email)?;
        }
        self.transport.purge_address(email)
    }
    pub(crate) fn purge_invitations(
        &self,
        email: &str,
        invitations: &[String],
    ) -> Result<(), String> {
        self.transport.purge_invitations(email, invitations)
    }
    pub fn dispatch_manual(
        &self,
        id: &str,
        issue: &mut dyn FnMut(&ManualRequest) -> Result<Mail, String>,
    ) -> Result<Mail, String> {
        self.transport.dispatch_manual(id, issue)
    }
    async fn enqueue_inner(
        &self,
        mail: Mail,
        manual: Option<ManualRequest>,
        deliver: bool,
        token: Option<String>,
        signup: bool,
    ) -> Result<(), String> {
        let permit = self
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| "Mail queue is busy")?;
        let queue = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            queue.enqueue_blocking(
                QueuedMail {
                    mail,
                    manual,
                    deliver,
                    journal: None,
                },
                token,
                signup,
            )
        })
        .await
        .map_err(|_| "Mail queue worker failed".to_owned())?
    }

    fn enqueue_blocking(
        &self,
        mut mail: QueuedMail,
        token: Option<String>,
        signup: bool,
    ) -> Result<(), String> {
        let permit = self.tx.try_reserve().map_err(|_| "Mail queue is full")?;
        let mut budget = self
            .budget
            .lock()
            .map_err(|_| "Mail budget lock poisoned")?;
        let mut charged = false;
        let mut charge = || {
            if charged {
                return Ok(());
            }
            let mut next = budget.clone();
            if next.day != day() {
                next = Budget {
                    day: day(),
                    sent: 0,
                };
            }
            let limit = if signup { self.limit / 2 } else { self.limit };
            if next.sent >= limit {
                return Err("Mail budget exhausted".to_owned());
            }
            next.sent += 1;
            let parent = self.path.parent().ok_or("Missing mail budget directory")?;
            let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.as_file()
                    .set_permissions(fs::Permissions::from_mode(0o600))
                    .map_err(|e| e.to_string())?;
            }
            serde_json::to_writer(&mut file, &next).map_err(|e| e.to_string())?;
            file.flush()
                .and_then(|_| file.as_file().sync_all())
                .map_err(|e| e.to_string())?;
            file.persist(&self.path).map_err(|e| e.error.to_string())?;
            crate::access::sync_directory(parent)?;
            *budget = next;
            charged = true;
            Ok(())
        };
        // The outbox checks the address cap under its lock before calling charge. Decoys keep
        // their rate history without spending a unit or appearing in the operator's queue.
        if let Some(request) = &mail.manual {
            for attempt in 0..3 {
                match self.transport.write_manual_charged(request, &mut charge) {
                    Ok(()) => return Ok(()),
                    Err(error)
                        if matches!(
                            error.as_str(),
                            "Manual request limit reached"
                                | "Invalid email"
                                | "Manual request already exists"
                        ) =>
                    {
                        return Ok(());
                    }
                    Err(error) if error == "Mail budget exhausted" || attempt == 2 => {
                        return Err(error);
                    }
                    Err(_) => std::thread::sleep(Duration::from_secs(1 << attempt)),
                }
            }
        }
        if mail.deliver {
            charge()?;
        }
        if let Some(journal) = &self.journal
            && mail.manual.is_none()
        {
            if self.link_check.is_some() && token.is_none() {
                return Err("Mail link is required".into());
            }
            let created = seconds();
            let expires = match &token {
                Some(token) => self
                    .link_check
                    .as_ref()
                    .and_then(|check| check(token))
                    .ok_or("Mail link expired")?,
                None => created + 3600,
            };
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).map_err(|e| e.to_string())?;
            let id = random.iter().map(|b| format!("{b:02x}")).collect();
            let entry = JournalEntry {
                request: ManualRequest {
                    id,
                    to: mail.mail.to.clone(),
                    kind: ManualKind::Signup,
                    created,
                    expires,
                    status: if mail.deliver {
                        ManualStatus::Pending
                    } else {
                        ManualStatus::Decoy
                    },
                    invitation: None,
                },
                subject: mail.mail.subject.clone(),
                body: mail.mail.body.clone(),
                html: mail.mail.html.clone(),
                token,
            };
            journal.save(&entry)?;
            mail.journal = Some(entry);
        }
        permit.send(QueueItem::Mail(Box::new(mail)));
        Ok(())
    }
}

#[cfg(test)]
mod smtp_tests {
    use super::*;
    use base64::Engine;
    use lettre::transport::smtp::client::Certificate;
    use tokio::io::{
        AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
    };
    use tokio_rustls::{
        TlsAcceptor,
        rustls::{
            ServerConfig,
            pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
        },
    };

    const CA: &[u8] = include_bytes!("../tests/fixtures/smtp-tls/ca.der");
    const CERT: &[u8] = include_bytes!("../tests/fixtures/smtp-tls/server.der");
    const KEY: &[u8] = include_bytes!("../tests/fixtures/smtp-tls/server-key.der");

    fn acceptor() -> TlsAcceptor {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec()));
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(CERT.to_vec())], key)
            .unwrap();
        TlsAcceptor::from(Arc::new(config))
    }

    fn client(port: u16, mode: SmtpTls) -> Smtp {
        let parameters = TlsParameters::builder("localhost".into())
            .add_root_certificate(Certificate::from_der(CA.to_vec()).unwrap())
            .build()
            .unwrap();
        Smtp::build(
            "localhost",
            port,
            "api_token".into(),
            "fixture-password".into(),
            "sender@example.test".into(),
            Some(mode),
            Some(parameters),
        )
        .unwrap()
    }

    async fn read_line<S: AsyncRead + Unpin>(socket: &mut BufReader<S>) -> String {
        let mut line = String::new();
        assert!(socket.read_line(&mut line).await.unwrap() > 0);
        line
    }

    async fn serve_secure<S: AsyncRead + AsyncWrite + Unpin>(socket: S) -> String {
        let mut socket = BufReader::new(socket);
        socket
            .get_mut()
            .write_all(b"220 localhost ESMTP\r\n")
            .await
            .unwrap();
        conversation(&mut socket).await
    }

    async fn conversation<S: AsyncRead + AsyncWrite + Unpin>(socket: &mut BufReader<S>) -> String {
        assert!(read_line(socket).await.starts_with("EHLO "));
        socket
            .get_mut()
            .write_all(b"250-localhost\r\n250-AUTH PLAIN\r\n250 8BITMIME\r\n")
            .await
            .unwrap();
        let auth = read_line(socket).await;
        let encoded = auth
            .trim_end()
            .strip_prefix("AUTH PLAIN ")
            .expect("AUTH PLAIN with initial response");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded, b"\0api_token\0fixture-password");
        socket
            .get_mut()
            .write_all(b"235 Authentication successful\r\n")
            .await
            .unwrap();
        assert!(
            read_line(socket)
                .await
                .starts_with("MAIL FROM:<sender@example.test>")
        );
        socket.get_mut().write_all(b"250 OK\r\n").await.unwrap();
        assert!(
            read_line(socket)
                .await
                .starts_with("RCPT TO:<person@example.test>")
        );
        socket.get_mut().write_all(b"250 OK\r\n").await.unwrap();
        assert_eq!(read_line(socket).await, "DATA\r\n");
        socket
            .get_mut()
            .write_all(b"354 Continue\r\n")
            .await
            .unwrap();
        let mut message = String::new();
        loop {
            let line = read_line(socket).await;
            if line == ".\r\n" {
                break;
            }
            message.push_str(if line.starts_with("..") {
                &line[1..]
            } else {
                &line
            });
        }
        // The data ends at the line holding only a dot; the CRLF before it ends the message's own
        // last line (RFC 5321 4.1.1.4), so the message is everything read before that line. Until
        // lettre 0.11.24 the client added an empty line after a message ending with CRLF.
        assert!(message.ends_with("\r\n"));
        socket.get_mut().write_all(b"250 Queued\r\n").await.unwrap();
        let _ = socket.get_mut().shutdown().await;
        message
    }

    async fn smtp_stub(
        listener: tokio::net::TcpListener,
        mode: SmtpTls,
        count: usize,
    ) -> Vec<String> {
        let mut messages = Vec::new();
        for _ in 0..count {
            let (socket, _) = listener.accept().await.unwrap();
            let acceptor = acceptor();
            if mode == SmtpTls::StartTls {
                let mut socket = BufReader::new(socket);
                socket
                    .get_mut()
                    .write_all(b"220 localhost ESMTP\r\n")
                    .await
                    .unwrap();
                assert!(read_line(&mut socket).await.starts_with("EHLO "));
                socket
                    .get_mut()
                    .write_all(b"250-localhost\r\n250 STARTTLS\r\n")
                    .await
                    .unwrap();
                assert_eq!(read_line(&mut socket).await, "STARTTLS\r\n");
                socket
                    .get_mut()
                    .write_all(b"220 Begin TLS\r\n")
                    .await
                    .unwrap();
                let tls = acceptor.accept(socket.into_inner()).await.unwrap();
                messages.push(conversation(&mut BufReader::new(tls)).await);
            } else {
                let tls = acceptor.accept(socket).await.unwrap();
                messages.push(serve_secure(tls).await);
            }
        }
        messages
    }

    fn decoded_body(message: &str) -> String {
        let (headers, body) = message.split_once("\r\n\r\n").expect("message headers");
        assert!(
            headers.contains("From: RichView <sender@example.test>"),
            "{headers}"
        );
        decoded(headers, body)
    }

    /// Each part of a `multipart/alternative` message: its content type and decoded text.
    fn decoded_parts(message: &str) -> Vec<(String, String)> {
        let (headers, body) = message.split_once("\r\n\r\n").expect("message headers");
        let headers = headers.to_ascii_lowercase();
        assert!(headers.contains("multipart/alternative"), "{headers}");
        let at = headers.find("boundary=\"").expect("boundary") + "boundary=\"".len();
        let boundary = &message[at..at + headers[at..].find('"').unwrap()];
        body.split(&format!("--{boundary}"))
            .filter(|part| part.starts_with("\r\n"))
            .map(|part| {
                let part = part.trim_start_matches("\r\n");
                let (headers, body) = part.split_once("\r\n\r\n").expect("part headers");
                let kind = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-type: ")
                            .map(str::to_owned)
                    })
                    .unwrap_or_default();
                (
                    kind,
                    decoded(headers, body.strip_suffix("\r\n").unwrap_or(body)),
                )
            })
            .collect()
    }

    fn decoded(headers: &str, body: &str) -> String {
        let bytes = if headers
            .to_ascii_lowercase()
            .contains("content-transfer-encoding: quoted-printable")
        {
            quoted_printable::decode(body.as_bytes(), quoted_printable::ParseMode::Robust).unwrap()
        } else if headers
            .to_ascii_lowercase()
            .contains("content-transfer-encoding: base64")
        {
            base64::engine::general_purpose::STANDARD
                .decode(body.replace(['\r', '\n'], ""))
                .unwrap()
        } else {
            body.as_bytes().to_vec()
        };
        String::from_utf8(bytes).unwrap().replace("\r\n", "\n")
    }

    #[test]
    fn smtp_ports_require_an_explicit_secure_mode_when_nonstandard() {
        use clap::Parser;
        assert_eq!(SmtpTls::for_port(465, None), Ok(SmtpTls::Implicit));
        assert_eq!(SmtpTls::for_port(587, None), Ok(SmtpTls::StartTls));
        assert!(SmtpTls::for_port(25, None).is_err());
        assert!(SmtpTls::for_port(0, Some(SmtpTls::Implicit)).is_err());
        assert!(SmtpTls::for_port(465, Some(SmtpTls::StartTls)).is_err());
        assert_eq!(
            SmtpTls::for_port(2525, Some(SmtpTls::StartTls)),
            Ok(SmtpTls::StartTls)
        );
        assert!(
            crate::cli::ServerArgs::try_parse_from(["server", "--smtp-tls", "plaintext"]).is_err()
        );
    }

    #[test]
    fn smtp_reports_the_configured_sender_without_its_display_name() {
        let smtp = Smtp::new(
            "smtp.example.test",
            465,
            "user".into(),
            "password".into(),
            "A display name <custom@example.test>".into(),
            Some(SmtpTls::Implicit),
        )
        .unwrap();
        assert_eq!(smtp.sender().as_deref(), Some("custom@example.test"));
    }

    #[tokio::test]
    async fn both_tls_modes_deliver_all_three_mail_texts_unchanged() {
        let link = "https://app.example.test/link/fixture";
        let bodies = [
            crate::server::mail_texts::signup(link),
            crate::server::mail_texts::reset(link),
            crate::server::mail_texts::already_registered(link),
        ];
        for mode in [SmtpTls::Implicit, SmtpTls::StartTls] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let smtp = client(listener.local_addr().unwrap().port(), mode);
            let server = tokio::spawn(smtp_stub(listener, mode, bodies.len()));
            for body in &bodies {
                smtp.send(&Mail {
                    to: "person@example.test".into(),
                    subject: "RichView".into(),
                    body: body.clone(),
                    html: None,
                })
                .await
                .unwrap();
            }
            let received = tokio::time::timeout(Duration::from_secs(10), server)
                .await
                .unwrap()
                .unwrap();
            for (message, expected) in received.iter().zip(&bodies) {
                assert_eq!(&decoded_body(message), expected);
            }
        }
    }

    #[tokio::test]
    async fn smtp_sends_the_html_part_beside_the_unchanged_text() {
        use crate::server::mail_texts::{Sender, Shared, share, share_html};
        let link = "https://app.example.test/link/fixture";
        let sender = Sender {
            name: "Jana Novak",
            email: "jana@example.test",
        };
        let report = Shared {
            organization: "Acme",
            title: "Quarterly churn review",
            description: "Where the third quarter lost subscribers.",
            editor: false,
        };
        let (text, html) = (
            share(&sender, &report, link),
            share_html(&sender, &report, link),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let smtp = client(listener.local_addr().unwrap().port(), SmtpTls::Implicit);
        let server = tokio::spawn(smtp_stub(listener, SmtpTls::Implicit, 1));
        smtp.send(&Mail {
            to: "person@example.test".into(),
            subject: "Jana Novak shared “Quarterly churn review” with you".into(),
            body: text.clone(),
            html: Some(html.clone()),
        })
        .await
        .unwrap();
        let received = tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .unwrap()
            .unwrap();
        let parts = decoded_parts(&received[0]);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].0.starts_with("text/plain"), "{}", parts[0].0);
        assert_eq!(parts[0].1, text);
        assert!(parts[1].0.starts_with("text/html"), "{}", parts[1].0);
        assert_eq!(parts[1].1, html);
    }

    #[tokio::test]
    async fn both_tls_modes_reject_an_untrusted_certificate_before_authentication() {
        for mode in [SmtpTls::Implicit, SmtpTls::StartTls] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                let socket = if mode == SmtpTls::StartTls {
                    let mut socket = BufReader::new(socket);
                    socket
                        .get_mut()
                        .write_all(b"220 localhost ESMTP\r\n")
                        .await
                        .unwrap();
                    assert!(read_line(&mut socket).await.starts_with("EHLO "));
                    socket
                        .get_mut()
                        .write_all(b"250-localhost\r\n250 STARTTLS\r\n")
                        .await
                        .unwrap();
                    assert_eq!(read_line(&mut socket).await, "STARTTLS\r\n");
                    socket
                        .get_mut()
                        .write_all(b"220 Begin TLS\r\n")
                        .await
                        .unwrap();
                    socket.into_inner()
                } else {
                    socket
                };
                if let Ok(mut tls) = acceptor().accept(socket).await {
                    let mut byte = [0];
                    let received =
                        tokio::time::timeout(Duration::from_secs(5), tls.read(&mut byte)).await;
                    assert!(
                        !matches!(received, Ok(Ok(n)) if n > 0),
                        "untrusted relay received SMTP data"
                    );
                }
            });
            let smtp = Smtp::new(
                "localhost",
                port,
                "api_token".into(),
                "fixture-password".into(),
                "sender@example.test".into(),
                Some(mode),
            )
            .unwrap();
            let mail = Mail {
                to: "person@example.test".into(),
                subject: "RichView".into(),
                body: "fixture link".into(),
                html: None,
            };
            assert!(smtp.send(&mail).await.is_err());
            tokio::time::timeout(Duration::from_secs(10), server)
                .await
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn smtp_errors_do_not_log_credentials_or_links() {
        const CHILD: &str = "RICHVIEW_TEST_SMTP_LOG_CHILD";
        const PASSWORD: &str = "smtp-fixture-password-unique";
        const LINK: &str = "https://example.test/link/smtp-fixture-unique";
        if std::env::var_os(CHILD).is_some() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let parameters = TlsParameters::builder("localhost".into())
                .add_root_certificate(Certificate::from_der(CA.to_vec()).unwrap())
                .build()
                .unwrap();
            let smtp = Smtp::build(
                "localhost",
                port,
                "api_token".into(),
                PASSWORD.into(),
                "sender@example.test".into(),
                Some(SmtpTls::Implicit),
                Some(parameters),
            )
            .unwrap();
            let server = tokio::spawn(async move {
                for _ in 0..1 {
                    let (socket, _) = listener.accept().await.unwrap();
                    let mut tls = BufReader::new(acceptor().accept(socket).await.unwrap());
                    tls.get_mut()
                        .write_all(b"220 localhost ESMTP\r\n")
                        .await
                        .unwrap();
                    assert!(read_line(&mut tls).await.starts_with("EHLO "));
                    tls.get_mut()
                        .write_all(b"250-localhost\r\n250 AUTH PLAIN\r\n")
                        .await
                        .unwrap();
                    assert!(read_line(&mut tls).await.starts_with("AUTH PLAIN "));
                    tls.get_mut()
                        .write_all(format!("535 {PASSWORD} {LINK}\r\n").as_bytes())
                        .await
                        .unwrap();
                }
            });
            let dir = tempfile::tempdir().unwrap();
            let queue = MailQueue::start(Arc::new(smtp), dir.path(), 10).unwrap();
            queue
                .enqueue(Mail {
                    to: "person@example.test".into(),
                    subject: "RichView".into(),
                    body: LINK.into(),
                    html: None,
                })
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(15), server)
                .await
                .unwrap()
                .unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
            return;
        }
        let output = tokio::task::spawn_blocking(|| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "mail::smtp_tests::smtp_errors_do_not_log_credentials_or_links",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert!(output.status.success(), "child SMTP test failed");
        let logs = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(logs.contains("hosted mail delivery failed; queued for restart"));
        assert!(!logs.contains(PASSWORD));
        assert!(!logs.contains(LINK));
    }
}

#[cfg(test)]
mod durable_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Provider {
        stall: AtomicBool,
        started: tokio::sync::Notify,
        sent: Mutex<Vec<String>>,
    }
    impl Transport for Provider {
        fn durable(&self) -> bool {
            true
        }
        fn send<'a>(
            &'a self,
            mail: &'a Mail,
        ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
            Box::pin(async move {
                self.started.notify_one();
                if self.stall.load(Ordering::SeqCst) {
                    std::future::pending::<()>().await;
                }
                self.sent.lock().unwrap().push(mail.body.clone());
                Ok(())
            })
        }
    }
    fn provider(stall: bool) -> Arc<Provider> {
        Arc::new(Provider {
            stall: AtomicBool::new(stall),
            started: tokio::sync::Notify::new(),
            sent: Mutex::new(Vec::new()),
        })
    }
    fn mail(body: &str) -> Mail {
        Mail {
            to: "person@example.test".into(),
            subject: "Test".into(),
            body: body.into(),
            html: None,
        }
    }

    #[tokio::test]
    async fn stalled_delivery_survives_shutdown_and_replays_once() {
        let dir = tempfile::tempdir().unwrap();
        let stalled = provider(true);
        let queue = MailQueue::start(stalled.clone(), dir.path(), 10).unwrap();
        queue.enqueue(mail("one")).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), stalled.started.notified())
            .await
            .unwrap();
        queue.enqueue(mail("two")).await.unwrap();
        assert_eq!(queue.pending_summary().unwrap().0, 2);
        queue.begin_shutdown();
        assert!(queue.enqueue(mail("late")).await.is_err());
        queue
            .finish_shutdown(tokio::time::Instant::now() + Duration::from_millis(50))
            .await;
        drop(queue);
        let sent = provider(false);
        let queue = MailQueue::start(sent.clone(), dir.path(), 10).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while sent.sent.lock().unwrap().len() != 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            while queue.pending_summary().unwrap().0 != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let mut messages = sent.sent.lock().unwrap().clone();
        messages.sort();
        assert_eq!(messages, ["one", "two"]);
        queue.begin_shutdown();
        queue
            .finish_shutdown(tokio::time::Instant::now() + Duration::from_secs(1))
            .await;
        let again = provider(false);
        let queue = MailQueue::start(again.clone(), dir.path(), 10).unwrap();
        assert_eq!(queue.summary().unwrap().2, 2);
        assert_eq!(queue.sent_since_start(), 0);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(again.sent.lock().unwrap().is_empty());
        queue.begin_shutdown();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn watchdog_bounds_a_stalled_journal_admission() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        const CHILD: &str = "RICHVIEW_TEST_STALLED_ADMISSION";
        if let Some(dir) = std::env::var_os(CHILD) {
            crate::server::start_process_shutdown_watchdog(Duration::from_millis(300)).unwrap();
            let dir = PathBuf::from(dir);
            let queue = MailQueue::start(provider(false), &dir, 10).unwrap();
            let journal = queue.journal.as_ref().unwrap().clone();
            // Hold the journal lock to stall a started admission in its blocking worker.
            let (held, ready) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let _guard = journal.lock.lock().unwrap();
                let _ = held.send(());
                loop {
                    std::thread::sleep(Duration::from_secs(60));
                }
            });
            ready.recv_timeout(Duration::from_secs(2)).unwrap();
            let budget = dir.join("mail-budget.json");
            tokio::spawn(async move {
                let _ = queue.enqueue(mail("stalled")).await;
            });
            tokio::time::timeout(Duration::from_secs(3), async {
                while !budget.exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            println!("ADMISSION_FIXTURE_READY");
            std::future::pending::<()>().await;
            return;
        }
        let root = std::env::current_dir().unwrap().join("tmp");
        fs::create_dir_all(&root).unwrap();
        let dir = tempfile::tempdir_in(root).unwrap();
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mail::durable_tests::watchdog_bounds_a_stalled_journal_admission",
                "--nocapture",
            ])
            .env(CHILD, dir.path())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                if line.contains("ADMISSION_FIXTURE_READY") {
                    return;
                }
            }
            panic!("admission fixture exited before ready");
        })
        .await
        .unwrap();
        let pid = child.id().unwrap() as libc::pid_t;
        // SAFETY: this PID belongs to the child spawned by this test.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
        let status = tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.code(), Some(143));
    }

    #[test]
    fn deleting_a_sender_removes_their_unsent_smtp_journal_mail_only() {
        use crate::server::mail_texts::{Sender, Shared, invite, share};
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(dir.path()).unwrap();
        let alice = Sender {
            name: "Alice",
            email: "alice@example.test",
        };
        let bob = Sender {
            name: "Bob",
            email: "bob@example.test",
        };
        for (id, body) in [
            (
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                invite(
                    &alice,
                    "A",
                    crate::access::OrgRole::Reader,
                    "https://app.test/link/a",
                ),
            ),
            (
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                invite(
                    &bob,
                    "B",
                    crate::access::OrgRole::Reader,
                    "https://app.test/link/b",
                ),
            ),
            (
                "cccccccccccccccccccccccccccccccc",
                share(
                    &alice,
                    &Shared {
                        organization: "A",
                        title: "Report",
                        description: "",
                        editor: false,
                    },
                    "https://app.test/link/c",
                ),
            ),
        ] {
            journal
                .save(&JournalEntry {
                    request: ManualRequest {
                        id: id.into(),
                        to: "friend@example.test".into(),
                        kind: ManualKind::Invite,
                        created: seconds(),
                        expires: seconds() + 3600,
                        status: ManualStatus::Pending,
                        invitation: None,
                    },
                    subject: "Invitation".into(),
                    body,
                    html: None,
                    token: None,
                })
                .unwrap();
        }
        journal.purge_address("ALICE@example.test").unwrap();
        let entries = journal.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].body.contains("bob@example.test"));
    }

    #[tokio::test]
    async fn an_ambiguous_failure_is_not_retried_in_the_same_run() {
        use std::sync::atomic::AtomicUsize;
        struct Failing(AtomicUsize);
        impl Transport for Failing {
            fn durable(&self) -> bool {
                true
            }
            fn send<'a>(
                &'a self,
                _: &'a Mail,
            ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
                Box::pin(async move {
                    self.0.fetch_add(1, Ordering::SeqCst);
                    Err(())
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let sender = Arc::new(Failing(AtomicUsize::new(0)));
        let queue = MailQueue::start(sender.clone(), dir.path(), 10).unwrap();
        queue.enqueue(mail("uncertain")).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while sender.0.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(1250)).await;
        assert_eq!(sender.0.load(Ordering::SeqCst), 1);
        assert_eq!(queue.summary().unwrap().0, 1);
        queue.begin_shutdown();
    }

    #[tokio::test]
    async fn manual_decoys_and_address_caps_do_not_spend_reserved_mail() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = Arc::new(Outbox::open(&dir.path().join("outbox")).unwrap());
        let queue = MailQueue::start(outbox, dir.path(), 10).unwrap();
        for _ in 0..4 {
            queue
                .enqueue_manual("same@example.test".into(), ManualKind::Signup, true)
                .await
                .unwrap();
        }
        queue
            .enqueue_manual("unknown@example.test".into(), ManualKind::Reset, false)
            .await
            .unwrap();
        assert_eq!(queue.budget.lock().unwrap().sent, 3);
        for i in 0..2 {
            queue
                .enqueue_manual(format!("new{i}@example.test"), ManualKind::Signup, true)
                .await
                .unwrap();
        }
        assert!(queue.signup_exhausted());
        assert!(!queue.exhausted());
        queue
            .enqueue_manual("known@example.test".into(), ManualKind::Reset, true)
            .await
            .unwrap();
        assert_eq!(queue.budget.lock().unwrap().sent, 6);
        queue.begin_shutdown();
    }

    #[tokio::test]
    async fn opens_legacy_link_budget_and_manual_request_schemas() {
        let dir = tempfile::tempdir().unwrap();
        let at = seconds();
        let links_path = dir.path().join("links.json");
        fs::write(&links_path, serde_json::to_vec(&serde_json::json!([{ "sha256": "legacy", "purpose": "signup", "email": "person@example.test", "created": at, "expires": at + 3600, "used": false }])).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&links_path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let links = crate::hosted_links::Links::open(dir.path()).unwrap();
        assert_eq!(links.pending().unwrap().len(), 1);
        fs::write(
            dir.path().join("mail-budget.json"),
            serde_json::to_vec(&serde_json::json!({ "day": day(), "sent": 2 })).unwrap(),
        )
        .unwrap();
        let outbox = Arc::new(Outbox::open(&dir.path().join("outbox")).unwrap());
        let id = "00000000000000000000000000000008";
        fs::write(outbox.dir.join(format!("{id}.request.json")), serde_json::to_vec(&serde_json::json!({ "id": id, "to": "person@example.test", "kind": "signup", "created": at, "expires": at + 86400, "status": "pending" })).unwrap()).unwrap();
        let queue = MailQueue::start(outbox, dir.path(), 2).unwrap();
        assert!(queue.exhausted());
        assert_eq!(queue.pending_manual().unwrap().len(), 1);
        queue.begin_shutdown();
    }

    #[tokio::test]
    async fn a_crash_after_counting_does_not_replay_the_settled_mail() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(dir.path()).unwrap();
        let at = seconds();
        let id = "00000000000000000000000000000006";
        journal
            .save(&JournalEntry {
                request: ManualRequest {
                    id: id.into(),
                    to: "person@example.test".into(),
                    kind: ManualKind::Signup,
                    created: at,
                    expires: at + 3600,
                    status: ManualStatus::Pending,
                    invitation: None,
                },
                subject: "Test".into(),
                body: "link".into(),
                html: None,
                token: None,
            })
            .unwrap();
        // This is the durable state after counting and before unlinking the record.
        journal
            .save_ledger(&SentLedger {
                count: 1,
                settling: vec![id.into()],
            })
            .unwrap();
        drop(journal);
        let provider = provider(false);
        let queue = MailQueue::start(provider.clone(), dir.path(), 10).unwrap();
        assert_eq!(queue.summary().unwrap(), (0, None, 1));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(provider.sent.lock().unwrap().is_empty());
        queue.begin_shutdown();
    }

    #[tokio::test]
    async fn expiry_cleanup_continues_after_counter_write_fails_and_storage_recovers() {
        struct Gate {
            started: tokio::sync::Notify,
            release: tokio::sync::Notify,
        }
        impl Transport for Gate {
            fn durable(&self) -> bool {
                true
            }
            fn send<'a>(
                &'a self,
                _: &'a Mail,
            ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
                Box::pin(async move {
                    self.started.notify_one();
                    self.release.notified().await;
                    Ok(())
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let provider = Arc::new(Gate {
            started: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let queue = MailQueue::start(provider.clone(), dir.path(), 10).unwrap();
        queue.enqueue(mail("private link")).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), provider.started.notified())
            .await
            .unwrap();
        let journal = queue.journal.as_ref().unwrap().clone();
        let counter = journal.outbox.dir.join("sent.json");
        fs::create_dir(&counter).unwrap();
        provider.release.notify_one();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !journal.uncertain.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        fs::remove_dir(&counter).unwrap();
        let path = fs::read_dir(&journal.outbox.dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".request.json")
            })
            .unwrap();
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["expires"] = serde_json::json!(seconds() - 1);
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        Journal::start_cleanup(&journal, None, Duration::from_millis(10));
        tokio::time::timeout(Duration::from_secs(2), async {
            while path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(queue.summary().is_err());
    }

    #[tokio::test]
    async fn timer_removes_expired_journal_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Arc::new(Journal::open(dir.path()).unwrap());
        let at = seconds();
        journal
            .save(&JournalEntry {
                request: ManualRequest {
                    id: "00000000000000000000000000000003".into(),
                    to: "person@example.test".into(),
                    kind: ManualKind::Signup,
                    created: at - 100,
                    expires: at - 1,
                    status: ManualStatus::Pending,
                    invitation: None,
                },
                subject: "Test".into(),
                body: "expired".into(),
                html: None,
                token: None,
            })
            .unwrap();
        Journal::start_cleanup(&journal, None, Duration::from_millis(10));
        tokio::time::timeout(Duration::from_secs(2), async {
            while !journal.entries().unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn timer_removes_old_manual_records_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = Arc::new(Outbox::open(&dir.path().join("manual")).unwrap());
        let transport: Arc<dyn Transport> = outbox.clone();
        let at = seconds();
        outbox
            .save_manual(&ManualRequest {
                id: "00000000000000000000000000000007".into(),
                to: "person@example.test".into(),
                kind: ManualKind::Signup,
                created: at - 7 * 86400,
                expires: at - 6 * 86400,
                status: ManualStatus::Dispatched,
                invitation: None,
            })
            .unwrap();
        start_transport_cleanup(&transport, Duration::from_millis(10), None);
        tokio::time::timeout(Duration::from_secs(2), async {
            while fs::read_dir(&outbox.dir).unwrap().next().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn startup_cleans_old_manual_records_and_keeps_recent_pending_requests() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = Arc::new(Outbox::open(&dir.path().join("manual")).unwrap());
        let at = seconds();
        for (id, created, expires, status) in [
            (
                "00000000000000000000000000000004",
                at - 7 * 86400,
                at - 6 * 86400,
                ManualStatus::Dispatched,
            ),
            (
                "00000000000000000000000000000005",
                at,
                at + 86400,
                ManualStatus::Pending,
            ),
        ] {
            outbox
                .save_manual(&ManualRequest {
                    id: id.into(),
                    to: "person@example.test".into(),
                    kind: ManualKind::Signup,
                    created,
                    expires,
                    status,
                    invitation: None,
                })
                .unwrap();
        }
        let queue = MailQueue::start(outbox.clone(), dir.path(), 10).unwrap();
        assert_eq!(queue.pending_summary().unwrap().0, 1);
        assert_eq!(outbox.records().unwrap().len(), 1);
        assert_eq!(fs::read_dir(&outbox.dir).unwrap().count(), 1);
        queue.begin_shutdown();
    }

    #[tokio::test]
    async fn startup_drops_expired_or_spent_link_without_sending() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(dir.path()).unwrap();
        let at = seconds();
        for (id, expires, token) in [
            ("00000000000000000000000000000001", at - 1, None),
            (
                "00000000000000000000000000000002",
                at + 100,
                Some("spent".to_owned()),
            ),
        ] {
            journal
                .save(&JournalEntry {
                    request: ManualRequest {
                        id: id.into(),
                        to: "person@example.test".into(),
                        kind: ManualKind::Signup,
                        created: at - 100,
                        expires,
                        status: ManualStatus::Pending,
                        invitation: None,
                    },
                    subject: "Test".into(),
                    body: "link".into(),
                    html: None,
                    token,
                })
                .unwrap();
        }
        let sent = provider(false);
        let checker: LinkCheck = Arc::new(|_| None);
        let queue =
            MailQueue::start_with_links(sent.clone(), dir.path(), 10, Some(checker), None).unwrap();
        assert_eq!(queue.pending_summary().unwrap(), (0, None));
        assert!(sent.sent.lock().unwrap().is_empty());
        queue.begin_shutdown();
    }
}
