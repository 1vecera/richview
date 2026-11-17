//! Restrict the worker before DuckDB opens or prepares any author-supplied input.
use std::{io, path::Path};

#[derive(Debug)]
pub(super) struct KernelTooOld;

impl std::fmt::Display for KernelTooOld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Landlock ABI 3 or newer is required")
    }
}

impl std::error::Error for KernelTooOld {}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn abi_error(error: io::Error) -> io::Error {
    if matches!(error.raw_os_error(), Some(libc::ENOSYS | libc::EOPNOTSUPP)) {
        io::Error::new(io::ErrorKind::Unsupported, KernelTooOld)
    } else {
        error
    }
}

pub(super) fn restrict(source: &Path) -> io::Result<()> {
    let source = source.canonicalize()?;
    if !source.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SQL source must be a regular file",
        ));
    }
    platform::restrict(&source)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::ffi::CString;

    #[link(name = "sandbox")]
    unsafe extern "C" {
        fn sandbox_init(
            profile: *const libc::c_char,
            flags: u64,
            error: *mut *mut libc::c_char,
        ) -> libc::c_int;
        fn sandbox_free_error(error: *mut libc::c_char);
    }

    pub(super) fn restrict(source: &Path) -> io::Result<()> {
        let path = source
            .to_str()
            .filter(|path| !path.chars().any(char::is_control))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Unsupported SQL source path")
            })?
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let profile = CString::new(format!(
            r#"(version 1)
(deny default)
(allow file-read* (literal "{path}"))
(allow sysctl-read)
(allow process-info* (target self))
(allow signal (target self))
"#
        ))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Unsupported SQL source path"))?;
        let mut error = std::ptr::null_mut();
        // The helper calls this before opening DuckDB or creating engine threads. Seatbelt's
        // process policy survives all later threads; neither SQL nor engine settings can relax it.
        let status = unsafe { sandbox_init(profile.as_ptr(), 0, &mut error) };
        if !error.is_null() {
            unsafe { sandbox_free_error(error) };
        }
        if status != 0 {
            return Err(io::Error::other("SQL process sandbox is unavailable"));
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::{
        fs::OpenOptions,
        os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    };

    #[repr(C)]
    struct Ruleset {
        access_fs: u64,
    }
    #[repr(C, packed)]
    struct PathRule {
        allowed: u64,
        fd: i32,
    }

    pub(super) fn require_abi(abi: libc::c_long) -> io::Result<()> {
        if abi < 0 {
            return Err(abi_error(io::Error::last_os_error()));
        }
        if abi < 3 {
            return Err(io::Error::new(io::ErrorKind::Unsupported, KernelTooOld));
        }
        Ok(())
    }

    pub(super) fn restrict(source: &Path) -> io::Result<()> {
        // ABI 3 is the first that can prohibit O_RDONLY|O_TRUNC. No best-effort fallback.
        let abi = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<u8>(),
                0,
                1,
            )
        };
        require_abi(abi)?;
        let source = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
            .open(source)?;
        let policy = Ruleset {
            access_fs: (1 << 15) - 1,
        };
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &policy,
                std::mem::size_of::<Ruleset>(),
                0,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let rule = PathRule {
            allowed: 1 << 2,
            fd: source.as_raw_fd(),
        }; // READ_FILE only.
        let installed = unsafe {
            libc::syscall(libc::SYS_landlock_add_rule, fd, 1, &rule, 0) == 0
                && libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == 0
                && libc::syscall(libc::SYS_landlock_restrict_self, fd, 0) == 0
        };
        unsafe {
            libc::close(fd as i32);
        }
        if !installed {
            return Err(io::Error::other("SQL filesystem sandbox is unavailable"));
        }
        network_and_process_filter()
    }

    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    fn network_and_process_filter() -> io::Result<()> {
        const ALLOW: u32 = 0x7fff0000;
        const KILL: u32 = 0x80000000;
        const DENY: u32 = 0x00050000 | libc::EPERM as u32;
        let ins = |code, jt, jf, k| libc::sock_filter { code, jt, jf, k };
        #[cfg(target_arch = "x86_64")]
        const ARCH: u32 = 0xc000003e;
        #[cfg(target_arch = "aarch64")]
        const ARCH: u32 = 0xc00000b7;
        let mut filter = vec![
            ins(0x20, 0, 0, 4),
            ins(0x15, 1, 0, ARCH),
            ins(0x06, 0, 0, KILL),
            ins(0x20, 0, 0, 0),
        ];
        #[cfg(target_arch = "x86_64")]
        filter.extend([ins(0x45, 0, 1, 0x40000000), ins(0x06, 0, 0, KILL)]); // No x32 ABI bypass.
        // pthreads use clone; returning ENOSYS for clone3 preserves libc's safe fallback.
        filter.extend([
            ins(0x15, 0, 1, libc::SYS_clone3 as u32),
            ins(0x06, 0, 0, 0x00050000 | libc::ENOSYS as u32),
        ]);
        let denied = vec![
            libc::SYS_socket,
            libc::SYS_socketpair,
            libc::SYS_connect,
            libc::SYS_bind,
            libc::SYS_accept,
            libc::SYS_accept4,
            libc::SYS_listen,
            libc::SYS_sendto,
            libc::SYS_sendmsg,
            libc::SYS_sendmmsg,
            libc::SYS_recvfrom,
            libc::SYS_recvmsg,
            libc::SYS_recvmmsg,
            libc::SYS_execve,
            libc::SYS_execveat,
            libc::SYS_ptrace,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
            libc::SYS_pidfd_open,
            libc::SYS_pidfd_getfd,
            libc::SYS_pidfd_send_signal,
            libc::SYS_kill,
            libc::SYS_tkill,
            libc::SYS_bpf,
            libc::SYS_perf_event_open,
            libc::SYS_unshare,
            libc::SYS_setns,
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_pivot_root,
            libc::SYS_chroot,
            libc::SYS_open_by_handle_at,
            libc::SYS_name_to_handle_at,
            libc::SYS_memfd_create,
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
            libc::SYS_ioctl,
            libc::SYS_fchmod,
            libc::SYS_fchmodat,
            452, // fchmodat2 on both supported Linux syscall ABIs.
            libc::SYS_fchown,
            libc::SYS_fchownat,
            libc::SYS_utimensat,
            libc::SYS_setxattr,
            libc::SYS_lsetxattr,
            libc::SYS_fsetxattr,
            libc::SYS_removexattr,
            libc::SYS_lremovexattr,
            libc::SYS_fremovexattr,
        ];
        #[cfg(target_arch = "x86_64")]
        let denied = {
            let mut denied = denied;
            denied.extend([
                libc::SYS_fork,
                libc::SYS_vfork,
                libc::SYS_chmod,
                libc::SYS_chown,
                libc::SYS_lchown,
                libc::SYS_utime,
                libc::SYS_utimes,
                libc::SYS_futimesat,
            ]);
            denied
        };
        for syscall in denied {
            filter.extend([ins(0x15, 0, 1, syscall as u32), ins(0x06, 0, 0, DENY)]);
        }
        // Clone may create threads in this process, never another process holding its pipes.
        filter.extend([
            ins(0x15, 0, 4, libc::SYS_clone as u32),
            ins(0x20, 0, 0, 16),
            ins(0x45, 1, 0, libc::CLONE_THREAD as u32),
            ins(0x06, 0, 0, DENY),
            ins(0x06, 0, 0, ALLOW),
        ]);
        // Signals needed by libc may target this process's threads only.
        filter.extend([
            ins(0x15, 0, 4, libc::SYS_tgkill as u32),
            ins(0x20, 0, 0, 16),
            ins(0x15, 1, 0, unsafe { libc::getpid() } as u32),
            ins(0x06, 0, 0, DENY),
            ins(0x06, 0, 0, ALLOW),
            ins(0x06, 0, 0, ALLOW),
        ]);
        let program = libc::sock_fprog {
            len: filter
                .len()
                .try_into()
                .map_err(|_| io::Error::other("SQL syscall policy is too large"))?,
            filter: filter.as_mut_ptr(),
        };
        // TSYNC also confines any runtime-created sibling threads; positive return means failure.
        if unsafe {
            libc::syscall(
                libc::SYS_seccomp,
                1,
                libc::SECCOMP_FILTER_FLAG_TSYNC,
                &program,
            )
        } != 0
        {
            return Err(io::Error::other(
                "SQL network/process sandbox is unavailable",
            ));
        }
        Ok(())
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    fn network_and_process_filter() -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Unsupported SQL worker architecture",
        ))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    use super::*;
    pub(super) fn restrict(_source: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "SQL process sandbox is unavailable",
        ))
    }
}

// These exercise the macOS and Linux sandboxes; other platforms have none to test.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use duckdb::{Config, Connection};
    use std::{fs, net::TcpListener, os::fd::AsRawFd, process::Command};

    #[cfg(target_os = "linux")]
    #[test]
    fn old_landlock_abi_reports_the_kernel_requirement() {
        let error = platform::require_abi(2).unwrap_err();
        assert!(matches!(
            super::super::sandbox_failure(error),
            super::super::protocol::Failure::KernelTooOld
        ));
        assert!(platform::require_abi(3).is_ok());
    }

    #[test]
    fn absent_landlock_is_distinct_from_other_sandbox_failures() {
        for errno in [libc::ENOSYS, libc::EOPNOTSUPP] {
            let error = abi_error(io::Error::from_raw_os_error(errno));
            assert!(matches!(
                super::super::sandbox_failure(error),
                super::super::protocol::Failure::KernelTooOld
            ));
        }
        let denied = abi_error(io::Error::from_raw_os_error(libc::EPERM));
        assert!(matches!(
            super::super::sandbox_failure(denied),
            super::super::protocol::Failure::Sandbox
        ));
    }

    #[test]
    fn os_policy_blocks_sql_io_even_when_duckdb_external_access_is_enabled() {
        const PROBE: &str = "RICHVIEW_SQL_SANDBOX_PROBE";
        if let Ok(input) = std::env::var(PROBE) {
            let (root, address, inherited): (std::path::PathBuf, String, i32) =
                serde_json::from_str(&input).unwrap();
            let source = root.join("source.duckdb");
            let secret = root.join("secret.csv");
            assert!(
                fs::read_to_string(&secret)
                    .unwrap()
                    .contains("host-only-sentinel")
            );
            super::super::close_inherited_descriptors().expect("inherited files close");
            assert_eq!(
                unsafe { libc::fcntl(inherited, libc::F_GETFD) },
                -1,
                "inherited file must not survive worker entry"
            );
            restrict(&source).expect("OS sandbox installs");
            let config = Config::default()
                .access_mode(duckdb::AccessMode::ReadOnly)
                .unwrap()
                .with("threads", "2")
                .unwrap();
            let declared = Connection::open_with_flags(&source, config)
                .expect("declared source remains readable");
            assert_eq!(
                declared
                    .query_row("SELECT n FROM allowed", [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                7
            );
            assert!(
                fs::read_to_string(&secret).is_err(),
                "OS must deny unrelated host files"
            );
            assert!(
                fs::OpenOptions::new().write(true).open(&source).is_err(),
                "declared source must remain read-only"
            );
            let source_name = std::ffi::CString::new(source.to_str().unwrap()).unwrap();
            let truncated =
                unsafe { libc::open(source_name.as_ptr(), libc::O_RDONLY | libc::O_TRUNC) };
            if truncated >= 0 {
                unsafe { libc::close(truncated) };
            }
            assert!(truncated < 0, "read-only open must not permit truncation");
            assert!(
                std::net::TcpStream::connect(&address).is_err(),
                "OS must deny sockets independently of DuckDB"
            );
            let config = Config::default()
                .with("threads", "2")
                .unwrap()
                .enable_external_access(true)
                .unwrap()
                .enable_autoload_extension(true)
                .unwrap()
                .with("allow_unsigned_extensions", "true")
                .unwrap();
            let connection = Connection::open_in_memory_with_flags(config).unwrap();
            assert!(
                connection
                    .query_row(
                        "SELECT current_setting('enable_external_access')",
                        [],
                        |row| row.get::<_, bool>(0)
                    )
                    .unwrap()
            );
            for sql in [
                format!("SELECT * FROM read_text('{}')", secret.display()),
                format!("SELECT * FROM read_csv('{}')", secret.display()),
                format!(
                    "COPY (SELECT 1 AS n) TO '{}'",
                    root.join("written.csv").display()
                ),
                format!("ATTACH '{}' AS other", root.join("other.duckdb").display()),
                format!(
                    "INSTALL '{}'",
                    root.join("forbidden.duckdb_extension").display()
                ),
                format!(
                    "LOAD '{}'",
                    root.join("forbidden.duckdb_extension").display()
                ),
                format!("SELECT * FROM read_csv('http://{address}/secret.csv')"),
            ] {
                let result = if sql.starts_with("SELECT") {
                    connection.query_row(&sql, [], |_| Ok(()))
                } else {
                    connection.execute_batch(&sql)
                };
                assert!(result.is_err(), "sandbox allowed {sql}");
            }
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        Connection::open(root.join("source.duckdb"))
            .unwrap()
            .execute_batch("CREATE TABLE allowed AS SELECT 7 AS n")
            .unwrap();
        Connection::open(root.join("other.duckdb"))
            .unwrap()
            .execute_batch("CREATE TABLE secret AS SELECT 9 AS n")
            .unwrap();
        fs::write(root.join("secret.csv"), "value\nhost-only-sentinel\n").unwrap();
        fs::write(
            root.join("forbidden.duckdb_extension"),
            "forbidden-extension-sentinel",
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let inherited = fs::File::open(root.join("secret.csv")).unwrap();
        assert_eq!(
            unsafe { libc::fcntl(inherited.as_raw_fd(), libc::F_SETFD, 0) },
            0
        );
        let test_name = concat!(
            module_path!(),
            "::os_policy_blocks_sql_io_even_when_duckdb_external_access_is_enabled"
        )
        .strip_prefix("richview_app::")
        .unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_name, "--nocapture"])
            .env_clear()
            .env("SLURM_MEM_PER_NODE", "128M")
            .env(
                PROBE,
                serde_json::to_string(&(
                    root.clone(),
                    listener.local_addr().unwrap().to_string(),
                    inherited.as_raw_fd(),
                ))
                .unwrap(),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        );
        assert!(!root.join("written.csv").exists());
    }
}
