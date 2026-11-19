//! Execute report SQL in an OS-restricted process, supervised by the trusted native host.
mod protocol;
mod sandbox;

use super::{
    duckdb_adapter::DuckDbConnector,
    spec::{
        ConnectorError, DatasetSnapshot, Provenance, ProvenanceSource, Query, QueryBody, SqlError,
    },
};
use protocol::{Failure, Reply, Request};
#[cfg(unix)]
use std::process::{Child, Command, Stdio};
use std::{
    io::{self, Write},
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(30);
const MEMORY_BYTES: u64 = 256 * 1024 * 1024;

/// The copies of SQL sources one render has made. On Windows the confined worker reads only a
/// copy of its source (see sandbox.rs); a render copies each source once, again only when the
/// source's size or write time changes, and removes the copies when it drops this, at its end.
/// Elsewhere it holds nothing.
#[derive(Default)]
pub(super) struct SourceCopies {
    #[cfg(windows)]
    copies: std::sync::Mutex<
        Vec<(
            std::path::PathBuf,
            Stamp,
            std::sync::Arc<sandbox::SourceCopy>,
        )>,
    >,
}

/// A source's size and last write time when it was copied.
#[cfg(windows)]
type Stamp = (u64, Option<std::time::SystemTime>);

#[cfg(windows)]
impl SourceCopies {
    /// The run's copy of `source` as it is now.
    fn of(&self, source: &Path) -> io::Result<std::sync::Arc<sandbox::SourceCopy>> {
        let metadata = std::fs::metadata(source)?;
        let stamp = (metadata.len(), metadata.modified().ok());
        let mut copies = self
            .copies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((_, _, copy)) = copies
            .iter()
            .find(|(path, kept, _)| path == source && *kept == stamp)
        {
            return Ok(copy.clone());
        }
        // An older copy goes once the workers still reading it have ended.
        copies.retain(|(path, _, _)| path != source);
        let copy = sandbox::SourceCopy::new(source)?;
        copies.push((source.to_path_buf(), stamp, copy.clone()));
        Ok(copy)
    }
}

/// Run `query` against `source` in the restricted worker and return its rows.
///
/// The connector calls this after its own SQL preguard; called directly, the worker's own
/// policy is the only one that applies.
pub fn execute(
    source: &Path,
    worker: Option<&Path>,
    query: &Query<'_>,
) -> Result<DatasetSnapshot, ConnectorError> {
    execute_in(&SourceCopies::default(), source, worker, query)
}

/// [`execute`] within a run that keeps its source copies in `copies`.
pub(super) fn execute_in(
    #[cfg_attr(not(windows), allow(unused_variables))] copies: &SourceCopies,
    source: &Path,
    worker: Option<&Path>,
    query: &Query<'_>,
) -> Result<DatasetSnapshot, ConnectorError> {
    let run = || -> Result<DatasetSnapshot, Failure> {
        let worker = worker
            .map(Path::to_path_buf)
            .or_else(|| {
                std::env::current_exe().ok().map(|path| {
                    path.with_file_name(format!(
                        "richview-sql-worker{}",
                        std::env::consts::EXE_SUFFIX
                    ))
                })
            })
            .ok_or(Failure::Sandbox)?;
        if !worker.is_absolute() {
            return Err(Failure::Sandbox);
        }
        let QueryBody::Sql(sql) = query.body else {
            return Err(Failure::Input);
        };
        let source = source
            .canonicalize()
            .map_err(|_| Failure::Query(SqlError::Database))?;
        // DuckDB keeps changes it has not yet written into a database in `<database>.wal`. The
        // worker may read the database file alone, on every platform, so it would miss them.
        match std::fs::symlink_metadata(wal_of(&source)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            _ => return Err(Failure::PendingWal),
        }
        let request = |source: std::path::PathBuf| {
            protocol::encode(&Request {
                version: protocol::VERSION,
                source,
                sql: sql.to_owned(),
                connector: query.connector.to_owned(),
                fingerprint: query.fingerprint.to_owned(),
                params: query.params.clone(),
            })
            .map_err(|_| Failure::Input)
        };
        #[cfg(unix)]
        let output = {
            let input = request(source)?;
            let mut child = Command::new(worker)
                .env_clear()
                // DuckDB consults cgroup files even with memory_limit set. Its fixed memory
                // discovery hint avoids those forbidden reads; no host environment is inherited.
                .env("SLURM_MEM_PER_NODE", "128M")
                .current_dir("/")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| Failure::Sandbox)?;
            let stdin = child.stdin.take().ok_or(Failure::Sandbox)?;
            let stdout = child.stdout.take().ok_or(Failure::Sandbox)?;
            supervise(child, stdin, stdout, input, TIMEOUT, MEMORY_BYTES)?
        };
        // The host starts the worker already confined and admits it to the run's copy of the
        // source, which it may read only while it runs (see sandbox.rs); the Windows folder is
        // the one host value the worker keeps, beside DuckDB's memory hint.
        #[cfg(windows)]
        let output = {
            let copy = copies.of(&source).map_err(|error| match error.kind() {
                io::ErrorKind::FileTooLarge => Failure::CopyTooLarge,
                io::ErrorKind::StorageFull => Failure::CopyNoSpace,
                _ => Failure::Query(SqlError::Database),
            })?;
            let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
            let sandboxed = sandbox::spawn(
                &worker,
                &copy,
                &[
                    ("SLURM_MEM_PER_NODE", std::ffi::OsStr::new("128M")),
                    ("SystemRoot", &windows),
                ],
            )
            .map_err(|_| Failure::Sandbox)?;
            let sandbox::Sandboxed {
                process,
                job,
                stdin,
                stdout,
                admitted,
            } = sandboxed;
            let input = request(admitted.source().to_path_buf())?;
            supervise(
                Confined {
                    process,
                    job,
                    _admitted: admitted,
                },
                stdin,
                stdout,
                input,
                TIMEOUT,
                MEMORY_BYTES,
            )?
        };
        match serde_json::from_slice::<Reply>(&output).map_err(|_| Failure::Output)? {
            Reply::Failed { error } => Err(error),
            Reply::Complete { rows, schema } => {
                if rows.len() > super::duckdb_adapter::MAX_ROWS {
                    return Err(Failure::Rows);
                }
                let row_count = rows.len();
                Ok(DatasetSnapshot {
                    rows,
                    schema,
                    provenance: Provenance {
                        connector: query.connector.to_owned(),
                        source: ProvenanceSource::Sql,
                        fetched_at: crate::time::Instant::now().seconds(),
                        fingerprint: query.fingerprint.to_owned(),
                        row_count,
                    },
                    cache: None,
                })
            }
        }
    };
    run().map_err(|failure| connector_error(failure, query.connector))
}

fn connector_error(failure: Failure, connector: &str) -> ConnectorError {
    let reason = match failure {
        Failure::Query(error) => {
            return ConnectorError::Sql {
                connector: connector.to_owned(),
                error,
            };
        }
        Failure::Metadata => super::duckdb_adapter::METADATA_REFUSAL,
        Failure::Parameters => {
            "SQL parameters must match every named placeholder exactly; positional placeholders are not supported"
        }
        Failure::Values => super::duckdb_adapter::UNSUPPORTED_SCALAR,
        Failure::Rows => {
            return ConnectorError::QueryRejected {
                reason: format!(
                    "result exceeds {} rows; narrow the query",
                    super::duckdb_adapter::MAX_ROWS
                ),
            };
        }
        Failure::Timeout => "SQL worker exceeded the 30000 ms timeout",
        Failure::Memory => "SQL worker exceeded its memory limit",
        Failure::Sandbox => {
            "SQL worker isolation is unavailable; check the trusted helper and OS sandbox support"
        }
        Failure::KernelTooOld => return ConnectorError::SqlIsolationUnavailable,
        Failure::Terminated => "SQL worker terminated before completing the query",
        Failure::Input => "SQL worker request exceeds its limit or is invalid",
        Failure::Output => "SQL worker result exceeds its limit or is invalid",
        Failure::PendingWal => {
            "the SQL database has changes not yet written into it, in the .wal file beside it; close the program writing it, or run CHECKPOINT in it, then render again"
        }
        #[cfg(windows)]
        Failure::CopyTooLarge => {
            "the SQL database is larger than 4 GiB, the most RichView copies into its Windows sandbox"
        }
        #[cfg(windows)]
        Failure::CopyNoSpace => {
            "the temporary folder lacks the free space to copy the SQL database into the Windows sandbox"
        }
    };
    ConnectorError::QueryRejected {
        reason: reason.into(),
    }
}

/// What the host needs of a running worker, on every platform.
trait Supervised {
    /// `Some(succeeded)` once the worker has exited.
    fn exited(&mut self) -> io::Result<Option<bool>>;
    fn kill(&mut self);
    fn reap(&mut self);
    fn resident_bytes(&self) -> io::Result<u64>;
}

#[cfg(unix)]
impl Supervised for Child {
    fn exited(&mut self) -> io::Result<Option<bool>> {
        Ok(self.try_wait()?.map(|status| status.success()))
    }
    fn kill(&mut self) {
        let _ = Child::kill(self);
    }
    fn reap(&mut self) {
        let _ = self.wait();
    }
    fn resident_bytes(&self) -> io::Result<u64> {
        resident_bytes(self.id())
    }
}

/// The confined worker, its job and its admission to the run's copy of the source; ending the
/// job ends the worker, and the admission ends once the worker has.
#[cfg(windows)]
struct Confined {
    process: crate::win_process::Process,
    job: crate::win_process::Job,
    _admitted: sandbox::Admitted,
}

#[cfg(windows)]
impl Supervised for Confined {
    fn exited(&mut self) -> io::Result<Option<bool>> {
        Ok(self.process.try_wait()?.map(|code| code == 0))
    }
    fn kill(&mut self) {
        let _ = self.job.terminate();
    }
    fn reap(&mut self) {
        let _ = self.process.wait_for(Some(Duration::from_secs(5)));
    }
    fn resident_bytes(&self) -> io::Result<u64> {
        self.process.resident_bytes()
    }
}

struct Worker<C: Supervised>(C);
impl<C: Supervised> Drop for Worker<C> {
    fn drop(&mut self) {
        self.0.kill();
        self.0.reap();
    }
}

fn supervise(
    child: impl Supervised,
    mut stdin: impl Write + Send,
    stdout: impl io::Read + Send,
    input: Vec<u8>,
    timeout: Duration,
    memory: u64,
) -> Result<Vec<u8>, Failure> {
    let started = Instant::now();
    std::thread::scope(|scope| {
        // The guard drops inside the scope, before scope joins can wait for child pipe EOF.
        let mut child = Worker(child);
        let writer = scope.spawn(move || stdin.write_all(&input));
        let (send, receive) = mpsc::channel();
        let reader = scope.spawn(move || {
            let _ = send.send(protocol::read(stdout));
        });
        let mut bytes = None;
        let result = loop {
            if started.elapsed() >= timeout {
                break Err(Failure::Timeout);
            }
            match receive.try_recv() {
                Ok(Ok(output)) => bytes = Some(output),
                Ok(Err(_)) => break Err(Failure::Output),
                Err(mpsc::TryRecvError::Disconnected) if bytes.is_none() => {
                    break Err(Failure::Output);
                }
                _ => {}
            }
            match child.0.exited() {
                Ok(Some(succeeded)) => {
                    if !succeeded {
                        break Err(Failure::Terminated);
                    }
                    break bytes.take().map(Ok).unwrap_or_else(|| {
                        receive
                            .recv()
                            .ok()
                            .and_then(Result::ok)
                            .ok_or(Failure::Output)
                    });
                }
                Err(_) => break Err(Failure::Sandbox),
                _ => {}
            }
            match child.0.resident_bytes() {
                Ok(bytes) if bytes > memory => break Err(Failure::Memory),
                Ok(_) => {}
                Err(_) => {
                    // A worker can exit between try_wait and the kernel accounting call.
                    if !matches!(child.0.exited(), Ok(Some(_))) {
                        break Err(Failure::Sandbox);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        if result.is_err() {
            child.0.kill();
        }
        child.0.reap();
        let written = writer
            .join()
            .map_err(|_| Failure::Input)?
            .map_err(|_| Failure::Input);
        reader.join().map_err(|_| Failure::Output)?;
        result.and_then(|output| written.map(|_| output))
    })
}

#[cfg(target_os = "macos")]
fn resident_bytes(pid: u32) -> io::Result<u64> {
    let mut usage: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    if unsafe {
        libc::proc_pid_rusage(
            pid as i32,
            libc::RUSAGE_INFO_V2,
            &mut usage as *mut _ as *mut libc::rusage_info_t,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(usage.ri_resident_size.max(usage.ri_phys_footprint))
}

#[cfg(target_os = "linux")]
fn resident_bytes(pid: u32) -> io::Result<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/statm"))?;
    let pages: u64 = status
        .split_whitespace()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| io::Error::other("SQL worker memory accounting unavailable"))?;
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return Err(io::Error::last_os_error());
    }
    pages
        .checked_mul(page_size as u64)
        .ok_or_else(|| io::Error::other("SQL worker memory accounting overflow"))
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn resident_bytes(_pid: u32) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Unsupported SQL worker platform",
    ))
}

/// Read one bounded request, restrict this process, and write a bounded result to stdout.
///
/// The dedicated executable calls this before creating threads or opening DuckDB. It never
/// accepts executable paths, engine configuration or filesystem grants from report SQL.
pub fn run_stdio() -> i32 {
    let reply = match worker_request() {
        Ok(snapshot) => Reply::Complete {
            rows: snapshot.rows,
            schema: snapshot.schema,
        },
        Err(error) => Reply::Failed { error },
    };
    let output = protocol::encode(&reply).or_else(|_| {
        protocol::encode(&Reply::Failed {
            error: Failure::Output,
        })
    });
    match output.and_then(|bytes| std::io::stdout().lock().write_all(&bytes)) {
        Ok(()) => 0,
        Err(_) => 2,
    }
}

fn sandbox_failure(error: io::Error) -> Failure {
    if error
        .get_ref()
        .is_some_and(|cause| cause.is::<sandbox::KernelTooOld>())
    {
        Failure::KernelTooOld
    } else {
        Failure::Sandbox
    }
}

fn worker_request() -> Result<DatasetSnapshot, Failure> {
    close_inherited_descriptors().map_err(|_| Failure::Sandbox)?;
    resource_limits().map_err(|_| Failure::Sandbox)?;
    let input = protocol::read(std::io::stdin().lock()).map_err(|_| Failure::Input)?;
    let request: Request = serde_json::from_slice(&input).map_err(|_| Failure::Input)?;
    if request.version != protocol::VERSION || !request.source.is_absolute() {
        return Err(Failure::Input);
    }
    let source = request
        .source
        .canonicalize()
        .map_err(|_| Failure::Query(SqlError::Database))?;
    // On Windows the source is the run's copy, alone in a folder that the restricted token may
    // pass through but no folder above it. The worker takes a handle to that folder while its
    // startup token may still open it, and DuckDB's opens of files in that folder go through
    // the handle (see sandbox.rs). On every platform DuckDB opens the source only once the
    // restriction holds.
    #[cfg(windows)]
    let database = {
        let folder = source.parent().ok_or(Failure::Input)?;
        if source.file_name() != Some(std::ffi::OsStr::new(sandbox::STAGED_NAME)) {
            return Err(Failure::Input);
        }
        sandbox::open_through(&plain_path(folder)).map_err(|_| Failure::Sandbox)?;
        start_duckdb().map_err(|_| Failure::Sandbox)?;
        plain_path(&source)
    };
    #[cfg(not(windows))]
    let database = source.clone();
    sandbox::restrict(&source).map_err(sandbox_failure)?;
    let open = || DuckDbConnector::open_read_only(&database, &request.connector);
    DuckDbConnector::execute_read_only_with(
        open,
        &Query {
            connector: &request.connector,
            body: QueryBody::Sql(&request.sql),
            params: &request.params,
            cache: None,
            fingerprint: &request.fingerprint,
        },
        TIMEOUT,
    )
    .map_err(|error| match error {
        ConnectorError::Sql { error, .. } => Failure::Query(error),
        ConnectorError::QueryRejected { reason }
            if reason == super::duckdb_adapter::METADATA_REFUSAL =>
        {
            Failure::Metadata
        }
        ConnectorError::QueryRejected { reason }
            if reason == super::duckdb_adapter::UNSUPPORTED_SCALAR =>
        {
            Failure::Values
        }
        ConnectorError::QueryRejected { reason } if reason.contains("rows") => Failure::Rows,
        ConnectorError::QueryRejected { reason } if reason.contains("timeout") => Failure::Timeout,
        ConnectorError::QueryRejected { reason } if reason.contains("memory") => Failure::Memory,
        ConnectorError::QueryRejected { reason } if reason.contains("parameter") => {
            Failure::Parameters
        }
        _ => Failure::Query(SqlError::Query),
    })
}

#[cfg(unix)]
fn resource_limits() -> io::Result<()> {
    fn set(resource: usize, value: u64) -> io::Result<()> {
        let limit = libc::rlimit {
            rlim_cur: value as libc::rlim_t,
            rlim_max: value as libc::rlim_t,
        };
        if unsafe { libc::setrlimit(resource as _, &limit) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    set(libc::RLIMIT_CORE as usize, 0)?;
    set(libc::RLIMIT_FSIZE as usize, 0)?;
    // Two engine threads can use two CPU-seconds per wall-second. The host's wall deadline
    // remains authoritative; this independent CPU cap must not terminate it prematurely.
    set(libc::RLIMIT_CPU as usize, TIMEOUT.as_secs() * 2 + 1)?;
    set(libc::RLIMIT_NOFILE as usize, 64)?;
    #[cfg(target_os = "linux")]
    set(libc::RLIMIT_AS as usize, 1024 * 1024 * 1024)?;
    Ok(())
}

/// On Windows the host's job sets the limits before the worker runs; the worker only checks
/// that it is inside that job.
#[cfg(windows)]
fn resource_limits() -> io::Result<()> {
    sandbox::require_job()
}

#[cfg(not(any(unix, windows)))]
fn resource_limits() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Unsupported SQL worker platform",
    ))
}

/// DuckDB loads some system libraries the first time it starts, its random source among them,
/// and the restricted token can load nothing by path. So the worker starts DuckDB once with an
/// empty in-memory database, which holds no author input, while its startup token still can.
#[cfg(windows)]
fn start_duckdb() -> io::Result<()> {
    duckdb::Connection::open_in_memory()
        .map(drop)
        .map_err(|error| io::Error::other(error.to_string()))
}

/// Where DuckDB keeps the changes it has not yet written into `database`.
fn wal_of(database: &Path) -> std::path::PathBuf {
    let mut name = database.as_os_str().to_owned();
    name.push(".wal");
    name.into()
}

#[cfg(windows)]
const _: () = assert!(sandbox::COPY_LIMIT == 4 << 30, "the refusal names 4 GiB");

/// A drive path without its `\\?\` prefix, which `canonicalize` adds and DuckDB need not parse.
#[cfg(windows)]
fn plain_path(path: &Path) -> std::path::PathBuf {
    let text = path.as_os_str().to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') && !text.contains('\u{fffd}') => {
            rest.into()
        }
        _ => path.to_path_buf(),
    }
}

#[cfg(target_os = "linux")]
fn close_inherited_descriptors() -> io::Result<()> {
    if unsafe { libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn close_inherited_descriptors() -> io::Result<()> {
    let pid = unsafe { libc::getpid() };
    let size =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if size < 0 {
        return Err(io::Error::last_os_error());
    }
    let count = size as usize / std::mem::size_of::<libc::proc_fdinfo>() + 32;
    let mut fds: Vec<libc::proc_fdinfo> =
        (0..count).map(|_| unsafe { std::mem::zeroed() }).collect();
    let bytes = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            (fds.len() * std::mem::size_of::<libc::proc_fdinfo>()) as i32,
        )
    };
    if bytes <= 0 || bytes as usize >= fds.len() * std::mem::size_of::<libc::proc_fdinfo>() {
        return Err(io::Error::other(
            "Cannot close inherited SQL worker descriptors",
        ));
    }
    for fd in fds
        .iter()
        .take(bytes as usize / std::mem::size_of::<libc::proc_fdinfo>())
    {
        if fd.proc_fd > 2 {
            unsafe { libc::close(fd.proc_fd) };
        }
    }
    Ok(())
}

/// The host's launch names the three standard handles as the only ones the worker inherits.
#[cfg(windows)]
fn close_inherited_descriptors() -> io::Result<()> {
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn close_inherited_descriptors() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Unsupported SQL worker platform",
    ))
}
