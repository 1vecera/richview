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
use std::{
    io::{self, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(30);
const MEMORY_BYTES: u64 = 256 * 1024 * 1024;

pub(super) fn execute(
    source: &Path,
    worker: Option<&Path>,
    query: &Query<'_>,
) -> Result<DatasetSnapshot, ConnectorError> {
    let run = || -> Result<DatasetSnapshot, Failure> {
        let worker = worker
            .map(Path::to_path_buf)
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .map(|path| path.with_file_name("richview-sql-worker"))
            })
            .ok_or(Failure::Sandbox)?;
        if !worker.is_absolute() {
            return Err(Failure::Sandbox);
        }
        let QueryBody::Sql(sql) = query.body else {
            return Err(Failure::Input);
        };
        let request = Request {
            version: protocol::VERSION,
            source: source
                .canonicalize()
                .map_err(|_| Failure::Query(SqlError::Database))?,
            sql: sql.to_owned(),
            connector: query.connector.to_owned(),
            fingerprint: query.fingerprint.to_owned(),
            params: query.params.clone(),
        };
        let input = protocol::encode(&request).map_err(|_| Failure::Input)?;
        let child = Command::new(worker)
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
        let output = supervise(child, input, TIMEOUT, MEMORY_BYTES)?;
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
        // The Windows preview has no sandbox for the worker, so it runs no author SQL (ARC-67).
        Failure::Sandbox if cfg!(windows) => {
            "SQL data sources are not available on Windows yet: the SQL worker has no sandbox there"
        }
        Failure::Sandbox => {
            "SQL worker isolation is unavailable; check the trusted helper and OS sandbox support"
        }
        Failure::KernelTooOld => return ConnectorError::SqlIsolationUnavailable,
        Failure::Terminated => "SQL worker terminated before completing the query",
        Failure::Input => "SQL worker request exceeds its limit or is invalid",
        Failure::Output => "SQL worker result exceeds its limit or is invalid",
    };
    ConnectorError::QueryRejected {
        reason: reason.into(),
    }
}

struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn supervise(
    child: Child,
    input: Vec<u8>,
    timeout: Duration,
    memory: u64,
) -> Result<Vec<u8>, Failure> {
    let started = Instant::now();
    std::thread::scope(|scope| {
        // The guard drops inside the scope, before scope joins can wait for child pipe EOF.
        let mut child = Worker(child);
        let mut stdin = child.0.stdin.take().ok_or(Failure::Sandbox)?;
        let stdout = child.0.stdout.take().ok_or(Failure::Sandbox)?;
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
            match child.0.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
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
            match resident_bytes(child.0.id()) {
                Ok(bytes) if bytes > memory => break Err(Failure::Memory),
                Ok(_) => {}
                Err(_) => {
                    // A worker can exit between try_wait and the kernel accounting call.
                    if !matches!(child.0.try_wait(), Ok(Some(_))) {
                        break Err(Failure::Sandbox);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        if result.is_err() {
            let _ = child.0.kill();
        }
        let _ = child.0.wait();
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
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
    sandbox::restrict(&source).map_err(sandbox_failure)?;
    DuckDbConnector::execute_read_only(
        &source,
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

#[cfg(not(unix))]
fn resource_limits() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Unsupported SQL worker platform",
    ))
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn close_inherited_descriptors() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Unsupported SQL worker platform",
    ))
}
