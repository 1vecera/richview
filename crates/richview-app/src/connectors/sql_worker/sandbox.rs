//! Restrict the worker before DuckDB opens or prepares any author-supplied input: the declared
//! source too is opened only inside the restriction, on every platform.
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

/// Windows has no call that confines a running process, so the host starts the worker already
/// confined and the worker gives up its one remaining right before it opens the declared source:
/// - **Token.** The process token is restricted: every group is deny-only, every privilege is
///   gone, and its one restricting SID is new for each launch, so no file, registry key, device,
///   pipe or process on the machine grants it anything. Only the worker's own objects grant it,
///   through the token's default DACL. Opening the network needs the AFD device, a file like
///   any other.
/// - **Integrity.** Low at start and Untrusted once the worker reverts, so it cannot write to
///   anything labelled above it, which is everything the user owns.
/// - **Source.** Once per render, and again whenever the source changes, the host copies the
///   declared source's bytes, unparsed, into a new folder that grants the user alone (up to
///   4 GiB, leaving 1 GiB free; the next render sweeps the folders a killed one left); while a
///   worker runs, its launch's restricting SID may pass through that folder and read the copy, and
///   nothing more. The restricted token can open nothing by path, because it may not pass through
///   the folders above, so before it restricts itself the worker takes a handle to the copy's
///   folder and points its own imports of `CreateFileW`, `GetFileAttributesW` and
///   `GetFileAttributesExW` at a redirect: a path whose folder is exactly the copy's and whose last
///   part is one plain name (no `..`, no stream) opens relative to that handle, under the
///   restricted token and without following a reparse point; every other path reaches Windows
///   unchanged and is refused. DuckDB opens the copy only after the restriction.
/// - **First thread.** The first thread starts under a second token that has the user's access
///   and keeps only SeChangeNotify of the user's privileges, also at Low, so the loader can map
///   system DLLs, DuckDB can start once on an empty in-memory database (loading what it needs on
///   its first start, its random source among them) and the worker can take the copy's folder.
///   The worker reverts to the restricted process token before DuckDB opens anything of the
///   author's; nothing can take that token back afterwards.
/// - **Job.** One process and no children (also refused by the child process policy), killed
///   when the host's handle closes, 1 GiB of commit and 61 s of CPU time, no access to other
///   processes' windows, clipboard or desktop. The host keeps its 30 s deadline and 256 MiB watch.
/// - **Handles.** Only the three standard handles are inherited, and the environment holds only
///   DuckDB's memory hint and the Windows folder.
#[cfg(windows)]
mod platform {
    use super::*;
    use crate::win_process::{
        self, Grant, Job, Launch, PROCESS_TOKEN, Process, Sid, Tokens, token_information,
    };
    use std::{
        ffi::OsStr,
        fs::File,
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
        },
        path::PathBuf,
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, ERROR_NO_TOKEN, FILETIME, GENERIC_ALL,
            GetLastError, HANDLE, LUID, WAIT_TIMEOUT,
        },
        Security::{
            CONTAINER_INHERIT_ACE, CreateRestrictedToken, DuplicateTokenEx,
            InitializeSecurityDescriptor, IsTokenRestricted, LUID_AND_ATTRIBUTES,
            LookupPrivilegeValueW, OBJECT_INHERIT_ACE, RevertToSelf, SECURITY_ATTRIBUTES,
            SECURITY_DESCRIPTOR, SID_AND_ATTRIBUTES, SecurityImpersonation,
            SetSecurityDescriptorDacl, SetTokenInformation, TOKEN_ADJUST_DEFAULT,
            TOKEN_ASSIGN_PRIMARY, TOKEN_DEFAULT_DACL, TOKEN_DUPLICATE, TOKEN_GROUPS,
            TOKEN_IMPERSONATE, TOKEN_MANDATORY_LABEL, TOKEN_PRIVILEGES, TOKEN_QUERY, TOKEN_USER,
            TokenDefaultDacl, TokenGroups, TokenImpersonation, TokenIntegrityLevel,
            TokenPrivileges, TokenUser,
        },
        Storage::FileSystem::{
            FILE_ALL_ACCESS, FILE_GENERIC_READ, FILE_TRAVERSE, GetDiskFreeSpaceExW, SYNCHRONIZE,
        },
        System::{
            JobObjects::{
                JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
                JOB_OBJECT_LIMIT_PROCESS_TIME, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JobObjectExtendedLimitInformation, QueryInformationJobObject,
            },
            SystemServices::{
                JOB_OBJECT_UILIMIT_ALL, SE_GROUP_ENABLED, SE_GROUP_INTEGRITY,
                SE_GROUP_USE_FOR_DENY_ONLY, SECURITY_DESCRIPTOR_REVISION,
            },
            Threading::{
                GetCurrentProcess, GetCurrentThread, GetProcessTimes, OpenProcess,
                OpenProcessToken, OpenThreadToken, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, WaitForSingleObject,
            },
        },
    };

    const LOW: u32 = 0x1000;
    const UNTRUSTED: u32 = 0;
    /// `PROCESS_ALL_ACCESS`, which equals `THREAD_ALL_ACCESS`: the worker's rights to itself.
    const ALL_ACCESS: u32 = 0x001f_ffff;
    const MEMORY_BYTES: usize = 1 << 30;
    /// The copy's name in its folder; the worker opens it by this name alone.
    pub(in super::super) const STAGED_NAME: &str = "source.duckdb";

    fn check(ok: i32) -> io::Result<()> {
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn integrity_of(token: HANDLE) -> io::Result<u32> {
        let buffer = token_information(token, TokenIntegrityLevel)?;
        // SAFETY: the buffer holds a TOKEN_MANDATORY_LABEL whose SID it also holds.
        let sid = unsafe {
            (*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL))
                .Label
                .Sid
        };
        Sid::copy(sid)
            .last()
            .ok_or_else(|| io::Error::other("token has no integrity level"))
    }

    fn set_integrity(token: HANDLE, rid: u32) -> io::Result<()> {
        let sid = Sid::new(16, &[rid]);
        let label = TOKEN_MANDATORY_LABEL {
            Label: SID_AND_ATTRIBUTES {
                Sid: sid.ptr(),
                Attributes: SE_GROUP_INTEGRITY as u32,
            },
        };
        // SAFETY: the label and the SID it points to live through the call.
        check(unsafe {
            SetTokenInformation(
                token,
                TokenIntegrityLevel,
                (&label as *const TOKEN_MANDATORY_LABEL).cast(),
                (size_of::<TOKEN_MANDATORY_LABEL>() + sid.bytes()) as u32,
            )
        })
    }

    /// An ACL granting `mask` to each SID.
    fn acl(sids: &[&Sid], mask: u32) -> io::Result<Vec<u32>> {
        let grants: Vec<Grant> = sids
            .iter()
            .map(|sid| Grant {
                sid,
                mask,
                inherit: 0,
            })
            .collect();
        win_process::acl(&grants)
    }

    fn set_default_dacl(token: HANDLE, acl: &mut [u32]) -> io::Result<()> {
        let dacl = TOKEN_DEFAULT_DACL {
            DefaultDacl: acl.as_mut_ptr().cast(),
        };
        // SAFETY: the record and the ACL it points to live through the call.
        check(unsafe {
            SetTokenInformation(
                token,
                TokenDefaultDacl,
                (&dacl as *const TOKEN_DEFAULT_DACL).cast(),
                size_of::<TOKEN_DEFAULT_DACL>() as u32,
            )
        })
    }

    /// `SeChangeNotifyPrivilege`, the one privilege the startup token keeps: without it every
    /// folder above the worker's executable and DLLs must grant traverse to the token.
    pub(super) fn change_notify() -> io::Result<LUID> {
        let name: Vec<u16> = "SeChangeNotifyPrivilege"
            .encode_utf16()
            .chain([0])
            .collect();
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        // SAFETY: the name is NUL-terminated and `luid` is a valid out-pointer.
        check(unsafe { LookupPrivilegeValueW(null(), name.as_ptr(), &mut luid) })?;
        Ok(luid)
    }

    /// `UNICODE_STRING`, `OBJECT_ATTRIBUTES` and `IO_STATUS_BLOCK` for `NtCreateFile`.
    #[repr(C)]
    pub(super) struct UnicodeString {
        pub(super) length: u16,
        pub(super) maximum_length: u16,
        pub(super) buffer: *const u16,
    }

    #[repr(C)]
    pub(super) struct ObjectAttributes {
        pub(super) length: u32,
        pub(super) root: HANDLE,
        pub(super) name: *const UnicodeString,
        pub(super) attributes: u32,
        pub(super) descriptor: *const core::ffi::c_void,
        pub(super) quality: *const core::ffi::c_void,
    }

    #[repr(C)]
    pub(super) struct IoStatusBlock {
        pub(super) status: usize,
        pub(super) information: usize,
    }

    #[link(name = "ntdll", kind = "raw-dylib")]
    unsafe extern "system" {
        pub(super) fn NtCreateFile(
            handle: *mut HANDLE,
            access: u32,
            attributes: *const ObjectAttributes,
            status: *mut IoStatusBlock,
            allocation: *const i64,
            file_attributes: u32,
            share: u32,
            disposition: u32,
            options: u32,
            ea: *const core::ffi::c_void,
            ea_length: u32,
        ) -> i32;
        fn RtlNtStatusToDosError(status: i32) -> u32;
    }

    /// DuckDB opens the run's copy by its path, but the restricted token can open nothing by
    /// path: it lacks the privilege to pass through the folders above the copy. Before it
    /// restricts itself, the worker takes a handle to the copy's folder and points its own
    /// imports of `CreateFileW`, `GetFileAttributesW` and `GetFileAttributesExW` at the
    /// functions here. A path whose folder is exactly the copy's and whose last part is one
    /// plain name opens relative to that handle, under the restricted token, never following a
    /// reparse point; every other path goes to Windows unchanged and meets the token's refusal.
    /// Nothing here opens anything with more than the token's own rights.
    mod redirect {
        use super::*;
        use std::sync::{
            OnceLock,
            atomic::{AtomicUsize, Ordering},
        };
        use windows_sys::Win32::{
            Foundation::{
                ERROR_ACCESS_DENIED, ERROR_FILENAME_EXCED_RANGE, ERROR_INVALID_PARAMETER,
                INVALID_HANDLE_VALUE, SetLastError,
            },
            Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
                FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_FLAG_DELETE_ON_CLOSE, FILE_FLAG_NO_BUFFERING, FILE_FLAG_OVERLAPPED,
                FILE_FLAG_RANDOM_ACCESS, FILE_FLAG_SEQUENTIAL_SCAN, FILE_FLAG_WRITE_THROUGH,
                FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
                GetFileInformationByHandle, INVALID_FILE_ATTRIBUTES, OPEN_EXISTING,
                WIN32_FILE_ATTRIBUTE_DATA,
            },
            System::{
                LibraryLoader::GetModuleHandleW,
                Memory::{PAGE_READWRITE, VirtualProtect},
            },
        };

        // NtCreateFile's disposition and options.
        const FILE_OPEN: u32 = 1;
        const FILE_WRITE_THROUGH: u32 = 0x2;
        const FILE_SEQUENTIAL_ONLY: u32 = 0x4;
        const FILE_NO_INTERMEDIATE_BUFFERING: u32 = 0x8;
        const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
        const FILE_NON_DIRECTORY_FILE: u32 = 0x40;
        const FILE_RANDOM_ACCESS: u32 = 0x800;
        const FILE_OPEN_FOR_BACKUP_INTENT: u32 = 0x4000;
        const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const OBJ_CASE_INSENSITIVE: u32 = 0x40;

        /// The copy's folder: the handle taken before the restriction, and its path in capitals.
        struct Folder {
            handle: usize,
            path: String,
        }

        static FOLDER: OnceLock<Folder> = OnceLock::new();
        static CREATE_FILE: AtomicUsize = AtomicUsize::new(0);
        static ATTRIBUTES: AtomicUsize = AtomicUsize::new(0);
        static ATTRIBUTES_EX: AtomicUsize = AtomicUsize::new(0);

        type CreateFile = unsafe extern "system" fn(
            *const u16,
            u32,
            u32,
            *const SECURITY_ATTRIBUTES,
            u32,
            u32,
            HANDLE,
        ) -> HANDLE;
        type Attributes = unsafe extern "system" fn(*const u16) -> u32;
        type AttributesEx =
            unsafe extern "system" fn(*const u16, i32, *mut core::ffi::c_void) -> i32;

        /// Take a handle to `folder` and route this process's opens of files in it through the
        /// handle. Called once, before the restriction.
        pub(in super::super::super) fn open_through(folder: &Path) -> io::Result<()> {
            let text = folder
                .to_str()
                .ok_or_else(|| io::Error::other("SQL copy folder is not Unicode"))?;
            let name: Vec<u16> = text.encode_utf16().chain([0]).collect();
            // SAFETY: the name is NUL-terminated; FILE_TRAVERSE on a folder needs backup semantics.
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    FILE_TRAVERSE | SYNCHRONIZE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    null(),
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS,
                    null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let folder = Folder {
                handle: handle as usize,
                path: text
                    .replace('/', "\\")
                    .trim_end_matches('\\')
                    .to_uppercase(),
            };
            if FOLDER.set(folder).is_err() {
                // SAFETY: CreateFileW returned this handle, which nothing else holds.
                unsafe { CloseHandle(handle) };
                return Err(io::Error::other("SQL copy folder is already set"));
            }
            // Every redirected import must be patched: one left to the loader would refuse its
            // opens of the copy later, as a missing database rather than as this failure.
            let hooks: [(&str, usize, &AtomicUsize); 3] = [
                (
                    "CreateFileW",
                    create_file as CreateFile as usize,
                    &CREATE_FILE,
                ),
                (
                    "GetFileAttributesW",
                    attributes as Attributes as usize,
                    &ATTRIBUTES,
                ),
                (
                    "GetFileAttributesExW",
                    attributes_ex as AttributesEx as usize,
                    &ATTRIBUTES_EX,
                ),
            ];
            for (function, hook, original) in hooks {
                // SAFETY: each hook has the signature of the function whose import it replaces.
                if unsafe { patch(function.as_bytes(), hook, original)? } == 0 {
                    return Err(io::Error::other(format!(
                        "SQL worker cannot redirect {function}: the executable does not import it"
                    )));
                }
            }
            Ok(())
        }

        /// Point every import of `function` in this executable at `hook`, keeping the target
        /// the loader resolved in `original`. Returns how many import slots changed.
        ///
        /// # Safety
        /// `hook` must have the signature of `function`.
        unsafe fn patch(function: &[u8], hook: usize, original: &AtomicUsize) -> io::Result<usize> {
            let at = |base: *const u8, offset: usize| -> u32 {
                // SAFETY: callers pass offsets inside the mapped image.
                unsafe { base.add(offset).cast::<u32>().read_unaligned() }
            };
            // SAFETY: a null name means this executable, mapped for the life of the process.
            let base = unsafe { GetModuleHandleW(null()) } as *const u8;
            if base.is_null() {
                return Err(io::Error::last_os_error());
            }
            let headers = at(base, 0x3c) as usize;
            // "PE\0\0", then a PE32+ optional header whose second data directory is the imports.
            // SAFETY: the offsets are inside the image's headers.
            let magic = unsafe { base.add(headers + 24).cast::<u16>().read_unaligned() };
            if at(base, headers) != 0x4550 || magic != 0x20b {
                return Err(io::Error::other("SQL worker image is not PE32+"));
            }
            let imports = at(base, headers + 24 + 112 + 8) as usize;
            let mut patched = 0;
            if imports == 0 {
                return Ok(0);
            }
            let mut descriptor = imports;
            loop {
                let (names, dll, slots) = (
                    at(base, descriptor) as usize,
                    at(base, descriptor + 12),
                    at(base, descriptor + 16) as usize,
                );
                if dll == 0 && slots == 0 {
                    break;
                }
                for index in 0.. {
                    if names == 0 {
                        break;
                    }
                    // SAFETY: the name table is a zero-terminated array of 64-bit entries.
                    let entry =
                        unsafe { base.add(names + index * 8).cast::<u64>().read_unaligned() };
                    if entry == 0 {
                        break;
                    }
                    if entry >> 63 == 1 {
                        continue; // imported by ordinal
                    }
                    // SAFETY: a name entry points at a hint and a NUL-terminated name.
                    let name = unsafe {
                        std::ffi::CStr::from_ptr(
                            base.add((entry & 0x7fff_ffff) as usize + 2).cast(),
                        )
                    };
                    if name.to_bytes() != function {
                        continue;
                    }
                    // SAFETY: the slot is this entry's place in the import address table.
                    let slot = unsafe { base.add(slots + index * 8) } as *mut usize;
                    // SAFETY: as above; the loader filled the slot.
                    let resolved = unsafe { slot.read_volatile() };
                    if let Err(kept) =
                        original.compare_exchange(0, resolved, Ordering::AcqRel, Ordering::Acquire)
                        && kept != resolved
                    {
                        return Err(io::Error::other("SQL worker imports differ"));
                    }
                    let mut protection = 0u32;
                    // SAFETY: the slot is one pointer of this image, made writable and then
                    // restored around the write.
                    check(unsafe {
                        VirtualProtect(
                            slot.cast(),
                            size_of::<usize>(),
                            PAGE_READWRITE,
                            &mut protection,
                        )
                    })?;
                    // SAFETY: as above.
                    unsafe { slot.write_volatile(hook) };
                    check(unsafe {
                        VirtualProtect(slot.cast(), size_of::<usize>(), protection, &mut protection)
                    })?;
                    patched += 1;
                }
                descriptor += 20;
            }
            Ok(patched)
        }

        /// The name of `path` when `path` is exactly the copy's folder and one plain name.
        fn name_in_folder(path: *const u16) -> Option<Vec<u16>> {
            let folder = FOLDER.get()?;
            if path.is_null() {
                return None;
            }
            let mut length = 0;
            // SAFETY: Windows callers pass a NUL-terminated string.
            while unsafe { *path.add(length) } != 0 {
                length += 1;
            }
            // SAFETY: `length` units precede the terminator.
            let text =
                String::from_utf16(unsafe { std::slice::from_raw_parts(path, length) }).ok()?;
            let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
            let (parent, name) = text.rsplit_once(['\\', '/'])?;
            let plain = !name.is_empty()
                && name != "."
                && name != ".."
                && !name.contains([':', '*', '?', '"', '<', '>', '|']);
            (plain && parent.replace('/', "\\").to_uppercase() == folder.path)
                .then(|| name.encode_utf16().collect())
        }

        /// Open `name` in the copy's folder through its handle; the restricted token decides.
        /// A reparse point is opened as itself and refused, as is a folder.
        fn open(
            name: &[u16],
            access: u32,
            share: u32,
            options: u32,
        ) -> Result<(HANDLE, BY_HANDLE_FILE_INFORMATION), u32> {
            let folder = FOLDER.get().ok_or(ERROR_ACCESS_DENIED)?;
            let bytes = u16::try_from(name.len() * 2).map_err(|_| ERROR_FILENAME_EXCED_RANGE)?;
            let object = UnicodeString {
                length: bytes,
                maximum_length: bytes,
                buffer: name.as_ptr(),
            };
            let attributes = ObjectAttributes {
                length: size_of::<ObjectAttributes>() as u32,
                root: folder.handle as HANDLE,
                name: &object,
                attributes: OBJ_CASE_INSENSITIVE,
                descriptor: null(),
                quality: null(),
            };
            let mut status_block = IoStatusBlock {
                status: 0,
                information: 0,
            };
            let mut handle: HANDLE = null_mut();
            // SAFETY: every pointer refers to a live record above.
            let status = unsafe {
                NtCreateFile(
                    &mut handle,
                    access | SYNCHRONIZE | FILE_READ_ATTRIBUTES,
                    &attributes,
                    &mut status_block,
                    null(),
                    0,
                    share,
                    FILE_OPEN,
                    options | FILE_OPEN_REPARSE_POINT,
                    null(),
                    0,
                )
            };
            if status < 0 {
                // SAFETY: no preconditions.
                return Err(unsafe { RtlNtStatusToDosError(status) });
            }
            // SAFETY: zero is valid for this plain record, which the call fills.
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            // SAFETY: the handle was just opened with FILE_READ_ATTRIBUTES.
            let read = unsafe { GetFileInformationByHandle(handle, &mut info) };
            if read == 0
                || info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY)
                    != 0
            {
                // SAFETY: the handle is ours and closed once.
                unsafe { CloseHandle(handle) };
                return Err(ERROR_ACCESS_DENIED);
            }
            Ok((handle, info))
        }

        /// `CreateFileW` for a file in the copy's folder: only an existing file is opened.
        unsafe extern "system" fn create_file(
            path: *const u16,
            access: u32,
            share: u32,
            security: *const SECURITY_ATTRIBUTES,
            disposition: u32,
            flags: u32,
            template: HANDLE,
        ) -> HANDLE {
            let Some(name) = name_in_folder(path) else {
                // SAFETY: the slot held this function before the hook replaced it.
                let original: CreateFile = unsafe {
                    std::mem::transmute::<usize, CreateFile>(CREATE_FILE.load(Ordering::Acquire))
                };
                // SAFETY: the caller's arguments, unchanged.
                return unsafe {
                    original(path, access, share, security, disposition, flags, template)
                };
            };
            if disposition != OPEN_EXISTING || flags & FILE_FLAG_DELETE_ON_CLOSE != 0 {
                // SAFETY: no preconditions.
                unsafe { SetLastError(ERROR_ACCESS_DENIED) };
                return INVALID_HANDLE_VALUE;
            }
            let mut options = if flags & FILE_FLAG_BACKUP_SEMANTICS != 0 {
                FILE_OPEN_FOR_BACKUP_INTENT
            } else {
                FILE_NON_DIRECTORY_FILE
            };
            for (flag, option) in [
                (FILE_FLAG_WRITE_THROUGH, FILE_WRITE_THROUGH),
                (FILE_FLAG_NO_BUFFERING, FILE_NO_INTERMEDIATE_BUFFERING),
                (FILE_FLAG_RANDOM_ACCESS, FILE_RANDOM_ACCESS),
                (FILE_FLAG_SEQUENTIAL_SCAN, FILE_SEQUENTIAL_ONLY),
            ] {
                if flags & flag != 0 {
                    options |= option;
                }
            }
            if flags & FILE_FLAG_OVERLAPPED == 0 {
                options |= FILE_SYNCHRONOUS_IO_NONALERT;
            }
            match open(&name, access, share, options) {
                Ok((handle, _)) => {
                    // SAFETY: no preconditions.
                    unsafe { SetLastError(0) };
                    handle
                }
                Err(error) => {
                    // SAFETY: no preconditions.
                    unsafe { SetLastError(error) };
                    INVALID_HANDLE_VALUE
                }
            }
        }

        /// The attributes of a file in the copy's folder, read through its handle.
        fn read_attributes(name: &[u16]) -> Result<BY_HANDLE_FILE_INFORMATION, u32> {
            let (handle, info) = open(
                name,
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_OPEN_FOR_BACKUP_INTENT | FILE_SYNCHRONOUS_IO_NONALERT,
            )?;
            // SAFETY: the handle is ours and closed once.
            unsafe { CloseHandle(handle) };
            Ok(info)
        }

        /// `GetFileAttributesW` for a file in the copy's folder.
        unsafe extern "system" fn attributes(path: *const u16) -> u32 {
            let Some(name) = name_in_folder(path) else {
                // SAFETY: the slot held this function before the hook replaced it.
                let original: Attributes = unsafe {
                    std::mem::transmute::<usize, Attributes>(ATTRIBUTES.load(Ordering::Acquire))
                };
                // SAFETY: the caller's argument, unchanged.
                return unsafe { original(path) };
            };
            match read_attributes(&name) {
                Ok(info) => info.dwFileAttributes,
                Err(error) => {
                    // SAFETY: no preconditions.
                    unsafe { SetLastError(error) };
                    INVALID_FILE_ATTRIBUTES
                }
            }
        }

        /// `GetFileAttributesExW` for a file in the copy's folder; only the standard level.
        unsafe extern "system" fn attributes_ex(
            path: *const u16,
            level: i32,
            data: *mut core::ffi::c_void,
        ) -> i32 {
            let Some(name) = name_in_folder(path) else {
                // SAFETY: the slot held this function before the hook replaced it.
                let original: AttributesEx = unsafe {
                    std::mem::transmute::<usize, AttributesEx>(
                        ATTRIBUTES_EX.load(Ordering::Acquire),
                    )
                };
                // SAFETY: the caller's arguments, unchanged.
                return unsafe { original(path, level, data) };
            };
            // GetFileExInfoStandard is the only level; anything else is refused, never widened.
            if level != 0 || data.is_null() {
                // SAFETY: no preconditions.
                unsafe { SetLastError(ERROR_INVALID_PARAMETER) };
                return 0;
            }
            match read_attributes(&name) {
                Ok(info) => {
                    let out = WIN32_FILE_ATTRIBUTE_DATA {
                        dwFileAttributes: info.dwFileAttributes,
                        ftCreationTime: info.ftCreationTime,
                        ftLastAccessTime: info.ftLastAccessTime,
                        ftLastWriteTime: info.ftLastWriteTime,
                        nFileSizeHigh: info.nFileSizeHigh,
                        nFileSizeLow: info.nFileSizeLow,
                    };
                    // SAFETY: the caller passes room for this record at the standard level.
                    unsafe {
                        data.cast::<WIN32_FILE_ATTRIBUTE_DATA>()
                            .write_unaligned(out)
                    };
                    1
                }
                Err(error) => {
                    // SAFETY: no preconditions.
                    unsafe { SetLastError(error) };
                    0
                }
            }
        }
    }

    pub(in super::super) use redirect::open_through;

    /// The largest database the host copies for the worker.
    pub(in super::super) const COPY_LIMIT: u64 = 4 << 30;
    /// The free space a copy must leave in the temporary folder.
    const COPY_RESERVE: u64 = 1 << 30;
    /// The start of every copy folder's name; the rest is `<pid>-<creation time>-<random>` of the
    /// process that made it, in hex after the pid, so a later run can tell when it has ended.
    const COPY_PREFIX: &str = "richview-sql-";

    /// Refuse a copy of `len` bytes above [`COPY_LIMIT`], or one that would leave less than
    /// [`COPY_RESERVE`] of the `free` bytes in the temporary folder.
    pub(super) fn copy_fits(len: u64, free: u64) -> io::Result<()> {
        if len > COPY_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "SQL source exceeds the copy limit",
            ));
        }
        if free < len.saturating_add(COPY_RESERVE) {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "too little free space to copy the SQL source",
            ));
        }
        Ok(())
    }

    /// The bytes free to this user on the volume of `folder`.
    fn free_space(folder: &Path) -> io::Result<u64> {
        let path: Vec<u16> = folder.as_os_str().encode_wide().chain([0]).collect();
        let mut free = 0u64;
        // SAFETY: `path` is NUL-terminated; the other outputs are optional.
        check(unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut free, null_mut(), null_mut()) })?;
        Ok(free)
    }

    /// When `process` started, in 100 ns units since 1601.
    pub(super) fn started(process: HANDLE) -> io::Result<u64> {
        let mut times = [FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        }; 4];
        let [created, exited, kernel, user] = &mut times;
        // SAFETY: four FILETIME outputs for a handle with query rights.
        check(unsafe { GetProcessTimes(process, created, exited, kernel, user) })?;
        Ok(u64::from(times[0].dwHighDateTime) << 32 | u64::from(times[0].dwLowDateTime))
    }

    /// Whether the process `pid` that started at `created` still runs. A process that exists but
    /// cannot be opened, or whose start cannot be read, counts as running.
    pub(super) fn running(pid: u32, created: u64) -> bool {
        // SAFETY: no preconditions; a null handle reports why.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if handle.is_null() {
            // SAFETY: no preconditions.
            return unsafe { GetLastError() } != ERROR_INVALID_PARAMETER;
        }
        // SAFETY: OpenProcess returned this handle, which nothing else holds.
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        match started(handle.as_raw_handle()) {
            // Another process has the id now.
            Ok(start) if start != created => false,
            // SAFETY: a process handle with SYNCHRONIZE; zero waits for nothing.
            Ok(_) => (unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) }) == WAIT_TIMEOUT,
            Err(_) => true,
        }
    }

    /// Remove the copy folders in `temp` whose process has ended. A run that ended without
    /// dropping its copies (Ctrl+C, a kill, a logoff) leaves them; folders that are links, or
    /// whose names do not parse, stay.
    pub(super) fn sweep(temp: &Path) {
        let Ok(entries) = std::fs::read_dir(temp) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(owner) = name
                .to_str()
                .and_then(|name| name.strip_prefix(COPY_PREFIX))
            else {
                continue;
            };
            let mut parts = owner.split('-');
            let (Some(pid), Some(created), Some(_), None) = (
                parts.next().and_then(|pid| pid.parse().ok()),
                parts
                    .next()
                    .and_then(|created| u64::from_str_radix(created, 16).ok()),
                parts.next(),
                parts.next(),
            ) else {
                continue;
            };
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) && !running(pid, created) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    /// A render's copy of one declared source, in a folder of its own. The user keeps full access;
    /// a launch's restricting SID may pass through the folder and read the copy only while that
    /// launch holds an [`Admitted`]. Dropping it, when the render ends, removes the folder; a
    /// later render sweeps the folder of one that ended without dropping it.
    pub(in super::super) struct SourceCopy {
        folder: PathBuf,
        pub(in super::super) source: PathBuf,
        user: Sid,
        admitted: std::sync::Mutex<Vec<Sid>>,
    }

    impl SourceCopy {
        /// Copy `source`'s bytes, without parsing them, into a new folder that grants the user
        /// alone until a launch is admitted. A source over [`COPY_LIMIT`], or one the temporary
        /// folder lacks the space for, is refused before anything is copied.
        pub(in super::super) fn new(source: &Path) -> io::Result<std::sync::Arc<SourceCopy>> {
            static SWEPT: std::sync::Once = std::sync::Once::new();
            let temp = std::env::temp_dir();
            SWEPT.call_once(|| sweep(&temp));
            copy_fits(std::fs::metadata(source)?.len(), free_space(&temp)?)?;
            let user = win_process::current_user()?;
            // SAFETY: the pseudo-handle of this process needs no closing.
            let created = started(unsafe { GetCurrentProcess() })?;
            let folder = temp.join(format!(
                "{COPY_PREFIX}{}-{created:x}-{:08x}",
                std::process::id(),
                fastrand::u32(..)
            ));
            win_process::create_private_dir(
                &folder,
                &[Grant {
                    sid: &user,
                    mask: FILE_ALL_ACCESS,
                    inherit: OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                }],
            )?;
            let copy = SourceCopy {
                source: folder.join(STAGED_NAME),
                folder,
                user,
                admitted: std::sync::Mutex::new(Vec::new()),
            };
            // The folder is new and the user's alone, so nothing else can have made this file.
            std::fs::copy(source, &copy.source)?;
            copy.apply(&[])?;
            Ok(std::sync::Arc::new(copy))
        }

        /// Set the folder's and the copy's DACLs to the user and the `admitted` SIDs alone.
        fn apply(&self, admitted: &[Sid]) -> io::Result<()> {
            let user = Grant {
                sid: &self.user,
                mask: FILE_ALL_ACCESS,
                inherit: 0,
            };
            let mut folder = vec![Grant {
                inherit: OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                ..user
            }];
            let mut file = vec![user];
            for sid in admitted {
                folder.push(Grant {
                    sid,
                    mask: FILE_TRAVERSE | SYNCHRONIZE,
                    inherit: 0,
                });
                file.push(Grant {
                    sid,
                    mask: FILE_GENERIC_READ,
                    inherit: 0,
                });
            }
            win_process::set_dacl(&self.folder, &folder)?;
            win_process::set_dacl(&self.source, &file)
        }
    }

    impl Drop for SourceCopy {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    /// One launch's admission to a run's copy; dropping it, once the worker has ended, takes the
    /// launch's SID off the folder and the copy again.
    pub(in super::super) struct Admitted {
        copy: std::sync::Arc<SourceCopy>,
        sid: Sid,
    }

    impl Admitted {
        pub(in super::super) fn source(&self) -> &Path {
            &self.copy.source
        }

        /// The launch's restricting SID.
        #[cfg(test)]
        pub(super) fn sid(&self) -> &Sid {
            &self.sid
        }
    }

    impl Drop for Admitted {
        fn drop(&mut self) {
            let mut admitted = self
                .copy
                .admitted
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            admitted.retain(|sid| sid != &self.sid);
            let _ = self.copy.apply(&admitted);
        }
    }

    /// The two tokens and the object security of one worker launch.
    pub(in super::super) struct Lockdown {
        primary: OwnedHandle,
        pub(in super::super) first_thread: OwnedHandle,
        /// The launch's restricting SID, which the run's copy grants while this launch runs.
        unique: Sid,
        _acl: Vec<u32>,
        descriptor: Box<SECURITY_DESCRIPTOR>,
    }

    impl Lockdown {
        pub(in super::super) fn new() -> io::Result<Lockdown> {
            let mut host: HANDLE = null_mut();
            // SAFETY: the pseudo handle is always valid; `host` is a valid out-pointer.
            check(unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY
                        | TOKEN_DUPLICATE
                        | TOKEN_ASSIGN_PRIMARY
                        | TOKEN_ADJUST_DEFAULT
                        | TOKEN_IMPERSONATE,
                    &mut host,
                )
            })?;
            // SAFETY: OpenProcessToken returned a new handle.
            let host = unsafe { OwnedHandle::from_raw_handle(host) };
            let user_buffer = token_information(host.as_raw_handle(), TokenUser)?;
            // SAFETY: the buffer holds a TOKEN_USER and the SID it points to.
            let user =
                Sid::copy(unsafe { (*(user_buffer.as_ptr() as *const TOKEN_USER)).User.Sid });
            let group_buffer = token_information(host.as_raw_handle(), TokenGroups)?;
            // SAFETY: the buffer holds a TOKEN_GROUPS with GroupCount entries.
            let groups = unsafe {
                let header = &*(group_buffer.as_ptr() as *const TOKEN_GROUPS);
                std::slice::from_raw_parts(header.Groups.as_ptr(), header.GroupCount as usize)
            };
            let privilege_buffer = token_information(host.as_raw_handle(), TokenPrivileges)?;
            // SAFETY: the buffer holds a TOKEN_PRIVILEGES with PrivilegeCount entries.
            let privileges: Vec<LUID_AND_ATTRIBUTES> = unsafe {
                let header = &*(privilege_buffer.as_ptr() as *const TOKEN_PRIVILEGES);
                std::slice::from_raw_parts(
                    header.Privileges.as_ptr(),
                    header.PrivilegeCount as usize,
                )
            }
            .iter()
            .map(|p| LUID_AND_ATTRIBUTES {
                Luid: p.Luid,
                Attributes: 0,
            })
            .collect();
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).map_err(|e| io::Error::other(e.to_string()))?;
            let random: Vec<u32> = random
                .chunks(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            // S-1-5-21 and four random numbers: an account no system or domain has.
            let unique = Sid::new(5, &[21, random[0], random[1], random[2], random[3]]);
            let system = Sid::new(5, &[18]);
            let not_label = |g: &&SID_AND_ATTRIBUTES| g.Attributes & SE_GROUP_INTEGRITY as u32 == 0;

            let deny: Vec<SID_AND_ATTRIBUTES> = groups
                .iter()
                .filter(not_label)
                .map(|g| SID_AND_ATTRIBUTES {
                    Sid: g.Sid,
                    Attributes: 0,
                })
                .collect();
            let restricting = [SID_AND_ATTRIBUTES {
                Sid: unique.ptr(),
                Attributes: 0,
            }];
            let mut primary: HANDLE = null_mut();
            // SAFETY: every array lives through the call and has the count passed with it.
            check(unsafe {
                CreateRestrictedToken(
                    host.as_raw_handle(),
                    0,
                    deny.len() as u32,
                    deny.as_ptr(),
                    privileges.len() as u32,
                    privileges.as_ptr(),
                    restricting.len() as u32,
                    restricting.as_ptr(),
                    &mut primary,
                )
            })?;
            // SAFETY: CreateRestrictedToken returned a new handle.
            let primary = unsafe { OwnedHandle::from_raw_handle(primary) };

            // The first thread's token: the user's own access, as a restricted token whose
            // restricting SIDs are all its enabled SIDs, because a restricted process may only
            // impersonate a restricted token.
            let mut same: Vec<SID_AND_ATTRIBUTES> = vec![SID_AND_ATTRIBUTES {
                Sid: user.ptr(),
                Attributes: 0,
            }];
            same.extend(
                groups
                    .iter()
                    .filter(not_label)
                    .filter(|g| {
                        g.Attributes & SE_GROUP_ENABLED as u32 != 0
                            && g.Attributes & SE_GROUP_USE_FOR_DENY_ONLY as u32 == 0
                    })
                    .map(|g| SID_AND_ATTRIBUTES {
                        Sid: g.Sid,
                        Attributes: 0,
                    }),
            );
            // It keeps only SeChangeNotify: an elevated host's SeBackup, SeDebug and the rest are
            // deleted, not just disabled.
            let traverse = change_notify()?;
            let startup_privileges: Vec<LUID_AND_ATTRIBUTES> = privileges
                .iter()
                .filter(|p| {
                    (p.Luid.LowPart, p.Luid.HighPart) != (traverse.LowPart, traverse.HighPart)
                })
                .map(|p| LUID_AND_ATTRIBUTES {
                    Luid: p.Luid,
                    Attributes: 0,
                })
                .collect();
            let mut restricted_same: HANDLE = null_mut();
            // SAFETY: as above.
            check(unsafe {
                CreateRestrictedToken(
                    host.as_raw_handle(),
                    0,
                    0,
                    null(),
                    startup_privileges.len() as u32,
                    startup_privileges.as_ptr(),
                    same.len() as u32,
                    same.as_ptr(),
                    &mut restricted_same,
                )
            })?;
            // SAFETY: CreateRestrictedToken returned a new handle.
            let restricted_same = unsafe { OwnedHandle::from_raw_handle(restricted_same) };
            let mut first_thread: HANDLE = null_mut();
            // SAFETY: the source handle has TOKEN_DUPLICATE; the out-pointer is valid.
            check(unsafe {
                DuplicateTokenEx(
                    restricted_same.as_raw_handle(),
                    TOKEN_IMPERSONATE | TOKEN_QUERY | TOKEN_ADJUST_DEFAULT | TOKEN_DUPLICATE,
                    null(),
                    SecurityImpersonation,
                    TokenImpersonation,
                    &mut first_thread,
                )
            })?;
            // SAFETY: DuplicateTokenEx returned a new handle.
            let first_thread = unsafe { OwnedHandle::from_raw_handle(first_thread) };

            // Objects either token creates belong to the worker alone: the user (for the
            // ordinary check), the launch's SID (for the restricted check) and the system.
            let mut defaults = acl(&[&user, &unique, &system], GENERIC_ALL)?;
            for token in [&primary, &first_thread] {
                set_integrity(token.as_raw_handle(), LOW)?;
                set_default_dacl(token.as_raw_handle(), &mut defaults)?;
            }
            let mut objects = acl(&[&user, &unique, &system], ALL_ACCESS)?;
            // SAFETY: zero is valid for this plain record before it is initialized.
            let mut descriptor: Box<SECURITY_DESCRIPTOR> = Box::new(unsafe { std::mem::zeroed() });
            let pointer: *mut SECURITY_DESCRIPTOR = &mut *descriptor;
            // SAFETY: the descriptor is boxed and the ACL buffer is kept beside it in `self`.
            check(unsafe {
                InitializeSecurityDescriptor(pointer.cast(), SECURITY_DESCRIPTOR_REVISION)
            })?;
            check(unsafe {
                SetSecurityDescriptorDacl(pointer.cast(), 1, objects.as_mut_ptr().cast(), 0)
            })?;
            Ok(Lockdown {
                primary,
                first_thread,
                unique,
                _acl: objects,
                descriptor,
            })
        }

        /// Let this launch's restricting SID pass through `copy`'s folder and read the copy, until
        /// the returned admission drops.
        pub(in super::super) fn admit(
            &self,
            copy: &std::sync::Arc<SourceCopy>,
        ) -> io::Result<Admitted> {
            let mut admitted = copy
                .admitted
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            admitted.push(self.unique.clone());
            if let Err(error) = copy.apply(&admitted) {
                admitted.pop();
                return Err(error);
            }
            Ok(Admitted {
                copy: copy.clone(),
                sid: self.unique.clone(),
            })
        }

        pub(in super::super) fn launch(
            &self,
            program: &Path,
            args: &[&OsStr],
            env: &[(&str, &OsStr)],
            descriptors: &[RawHandle],
            job: &Job,
        ) -> io::Result<Process> {
            let objects = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: (&*self.descriptor as *const SECURITY_DESCRIPTOR) as *mut _,
                bInheritHandle: 0,
            };
            let root = std::env::var_os("SystemDrive")
                .map(|drive| {
                    let mut root = drive;
                    root.push("\\");
                    std::path::PathBuf::from(root)
                })
                .unwrap_or_else(|| "C:\\".into());
            win_process::launch(&Launch {
                program,
                args,
                env: Some(env),
                cwd: Some(&root),
                descriptors,
                job,
                tokens: Some(Tokens {
                    primary: &self.primary,
                    first_thread: &self.first_thread,
                    objects: &objects,
                }),
                no_child_processes: true,
            })
        }
    }

    /// The worker's job: one process, no children, killed with the host's handle.
    pub(in super::super) fn job() -> io::Result<Job> {
        // SAFETY: zero is valid for every field of this plain record.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_PROCESS_TIME;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        // In 100 ns units: two engine threads for the whole deadline, as RLIMIT_CPU on Unix.
        limits.BasicLimitInformation.PerProcessUserTimeLimit =
            ((super::super::TIMEOUT.as_secs() * 2 + 1) * 10_000_000) as i64;
        limits.ProcessMemoryLimit = MEMORY_BYTES;
        Job::new(&limits, Some(JOB_OBJECT_UILIMIT_ALL))
    }

    /// The worker, confined, with the host's ends of its standard input and output and its
    /// admission to the run's copy of the source.
    pub(in super::super) struct Sandboxed {
        pub process: Process,
        pub job: Job,
        pub stdin: File,
        pub stdout: File,
        pub admitted: Admitted,
    }

    /// Start the worker confined, admitted to `copy`, with its environment only `env`.
    pub(in super::super) fn spawn(
        worker: &Path,
        copy: &std::sync::Arc<SourceCopy>,
        env: &[(&str, &OsStr)],
    ) -> io::Result<Sandboxed> {
        let lockdown = Lockdown::new()?;
        let admitted = lockdown.admit(copy)?;
        let job = job()?;
        let (child_in, stdin) = win_process::pipe()?;
        let (stdout, child_out) = win_process::pipe()?;
        let null = win_process::null_device()?;
        let process = lockdown.launch(
            worker,
            &[],
            env,
            &[
                child_in.as_raw_handle(),
                child_out.as_raw_handle(),
                null.as_raw_handle(),
            ],
            &job,
        )?;
        Ok(Sandboxed {
            process,
            job,
            stdin: File::from(stdin),
            stdout: File::from(stdout),
            admitted,
        })
    }

    /// The worker refuses to run outside the job the host gives it.
    pub(in super::super) fn require_job() -> io::Result<()> {
        // SAFETY: zero is valid for this plain record; a null job means this process's job.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: the record is the class's documented type and size.
        check(unsafe {
            QueryInformationJobObject(
                null_mut(),
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                null_mut(),
            )
        })?;
        let flags = limits.BasicLimitInformation.LimitFlags;
        let required = JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        if flags & required != required
            || limits.BasicLimitInformation.ActiveProcessLimit != 1
            || limits.ProcessMemoryLimit > MEMORY_BYTES
        {
            return Err(io::Error::other("SQL worker is not in its job"));
        }
        Ok(())
    }

    pub(super) fn restrict(_source: &Path) -> io::Result<()> {
        // Lower the process token while the first thread's token may still open it, then drop
        // that token for good.
        let mut token: HANDLE = null_mut();
        // SAFETY: the pseudo handle is always valid; `token` is a valid out-pointer.
        check(unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_DEFAULT | TOKEN_QUERY,
                &mut token,
            )
        })?;
        // SAFETY: OpenProcessToken returned a new handle.
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        set_integrity(token.as_raw_handle(), UNTRUSTED)?;
        drop(token);
        // SAFETY: no preconditions; the thread keeps running under the process token.
        check(unsafe { RevertToSelf() })?;
        let mut thread_token: HANDLE = null_mut();
        // SAFETY: valid pseudo handle and out-pointer; a token here means the revert failed.
        if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut thread_token) } != 0 {
            // SAFETY: OpenThreadToken returned a new handle.
            unsafe { CloseHandle(thread_token) };
            return Err(io::Error::other("SQL worker still impersonates"));
        }
        // SAFETY: GetLastError has no preconditions.
        if unsafe { GetLastError() } != ERROR_NO_TOKEN {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the pseudo handle has query access to this process's token.
        if unsafe { IsTokenRestricted(PROCESS_TOKEN) } == 0 {
            return Err(io::Error::other("SQL worker token is not restricted"));
        }
        if integrity_of(PROCESS_TOKEN)? != UNTRUSTED {
            return Err(io::Error::other("SQL worker integrity is not Untrusted"));
        }
        let privileges = token_information(PROCESS_TOKEN, TokenPrivileges)?;
        // SAFETY: the buffer holds a TOKEN_PRIVILEGES.
        if unsafe { (*(privileges.as_ptr() as *const TOKEN_PRIVILEGES)).PrivilegeCount } != 0 {
            return Err(io::Error::other("SQL worker token keeps a privilege"));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub(super) use platform::{
    Admitted, COPY_LIMIT, STAGED_NAME, Sandboxed, SourceCopy, open_through, require_job, spawn,
};
#[cfg(all(windows, test))]
pub(super) use platform::{Lockdown, job};

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
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

// The Windows counterpart of the probe above. Each route is first shown open outside the
// sandbox, then denied inside it; the denials print as the evidence of the run.
#[cfg(all(test, windows))]
mod windows_tests {
    use super::platform::{IoStatusBlock, NtCreateFile, ObjectAttributes, UnicodeString};
    use super::*;
    use crate::win_process::{self, Grant};
    use duckdb::{Config, Connection};
    use serde_json::{Value, json};
    use std::{
        ffi::OsStr,
        fs,
        io::Read,
        net::{TcpListener, TcpStream, UdpSocket},
        os::windows::{
            ffi::OsStrExt,
            fs::OpenOptionsExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        path::PathBuf,
        ptr::{null, null_mut},
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, GENERIC_READ, GENERIC_WRITE,
            HANDLE, INVALID_HANDLE_VALUE,
        },
        Networking::WinSock::{
            AF_INET, INVALID_SOCKET, IPPROTO_TCP, SOCK_STREAM, WSA_FLAG_NO_HANDLE_INHERIT,
            WSA_FLAG_OVERLAPPED, WSADATA, WSAGetLastError, WSASocketW, WSAStartup, closesocket,
        },
        Security::{TOKEN_PRIVILEGES, TokenPrivileges},
        Storage::FileSystem::{
            CreateFileW, FILE_ALL_ACCESS, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE, GetShortPathNameW, OPEN_EXISTING,
            PIPE_ACCESS_DUPLEX, READ_CONTROL, SYNCHRONIZE, WRITE_DAC,
        },
        System::{
            Pipes::{CreateNamedPipeW, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES},
            Registry::{
                HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RegCloseKey, RegOpenKeyExW,
            },
            Threading::{
                GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
                SetThreadToken,
            },
        },
    };

    const PROBE: &str = "RICHVIEW_SQL_SANDBOX_PROBE";

    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain([0]).collect()
    }

    fn open_path(path: &str) -> io::Result<()> {
        let name = wide(OsStr::new(path));
        // SAFETY: the name is NUL-terminated; a handle that opens is closed at once.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateFileW returned this handle.
        unsafe { CloseHandle(handle) };
        Ok(())
    }

    fn raw_socket() -> io::Result<()> {
        // SAFETY: zero is valid for this plain record, which WSAStartup fills.
        let mut data: WSADATA = unsafe { std::mem::zeroed() };
        // SAFETY: valid out-pointer.
        let started = unsafe { WSAStartup(0x0202, &mut data) };
        if started != 0 {
            return Err(io::Error::from_raw_os_error(started));
        }
        // SAFETY: plain socket creation; a socket that opens is closed at once.
        let socket = unsafe {
            WSASocketW(
                AF_INET as i32,
                SOCK_STREAM,
                IPPROTO_TCP,
                null(),
                0,
                WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT,
            )
        };
        if socket == INVALID_SOCKET {
            // SAFETY: no preconditions.
            return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
        }
        // SAFETY: the socket was just created.
        unsafe { closesocket(socket) };
        Ok(())
    }

    /// Open a TCP endpoint on the network driver as Winsock's provider does: `\Device\Afd\Endpoint`
    /// with the open packet (flags, group, family, type, protocol) in an extended attribute
    /// named `AfdOpenPacketXX`. Every socket on Windows starts as this open.
    fn afd_endpoint() -> io::Result<()> {
        const NAME: &[u8] = b"AfdOpenPacketXX";
        const STATUS_ACCESS_DENIED: u32 = 0xC000_0022;
        let mut packet = Vec::new();
        for word in [
            0,
            0,
            u32::from(AF_INET),
            SOCK_STREAM as u32,
            IPPROTO_TCP as u32,
        ] {
            packet.extend(word.to_le_bytes());
        }
        packet.resize(30, 0);
        // FILE_FULL_EA_INFORMATION: next entry, flags, name length, value length, name, NUL, value.
        let mut ea = 0u32.to_le_bytes().to_vec();
        ea.extend([0, NAME.len() as u8]);
        ea.extend((packet.len() as u16).to_le_bytes());
        ea.extend(NAME);
        ea.push(0);
        ea.extend(&packet);
        let path: Vec<u16> = r"\Device\Afd\Endpoint".encode_utf16().collect();
        let name = UnicodeString {
            length: (path.len() * 2) as u16,
            maximum_length: (path.len() * 2) as u16,
            buffer: path.as_ptr(),
        };
        let attributes = ObjectAttributes {
            length: size_of::<ObjectAttributes>() as u32,
            root: null_mut(),
            name: &name,
            attributes: 0x40, // OBJ_CASE_INSENSITIVE
            descriptor: null(),
            quality: null(),
        };
        let mut status_block = IoStatusBlock {
            status: 0,
            information: 0,
        };
        let mut handle: HANDLE = null_mut();
        // SAFETY: every pointer refers to a live record above; the EA buffer has the length
        // passed. A handle that opens is closed at once.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                GENERIC_READ | GENERIC_WRITE | SYNCHRONIZE,
                &attributes,
                &mut status_block,
                null(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                3, // FILE_OPEN_IF
                0,
                ea.as_ptr().cast(),
                ea.len() as u32,
            )
        };
        if status < 0 {
            let kind = if status as u32 == STATUS_ACCESS_DENIED {
                io::ErrorKind::PermissionDenied
            } else {
                io::ErrorKind::Other
            };
            return Err(io::Error::new(
                kind,
                format!("NtCreateFile status {:#010x}", status as u32),
            ));
        }
        // SAFETY: NtCreateFile returned this handle.
        unsafe { CloseHandle(handle) };
        Ok(())
    }

    fn open_process(pid: u32, access: u32) -> io::Result<()> {
        // SAFETY: a handle that opens is closed at once.
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: OpenProcess returned this handle.
        unsafe { CloseHandle(handle) };
        Ok(())
    }

    fn open_key(root: HKEY, path: &str) -> io::Result<()> {
        let name = wide(OsStr::new(path));
        let mut key: HKEY = null_mut();
        // SAFETY: the name is NUL-terminated; a key that opens is closed at once.
        let status = unsafe { RegOpenKeyExW(root, name.as_ptr(), 0, KEY_READ, &mut key) };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        // SAFETY: RegOpenKeyExW returned this key.
        unsafe { RegCloseKey(key) };
        Ok(())
    }

    fn start_process(windows: &Path) -> io::Result<()> {
        let mut child = std::process::Command::new(windows.join("System32").join("cmd.exe"))
            .args(["/c", "exit", "0"])
            .spawn()?;
        child.wait().map(drop)
    }

    /// Every route the sandboxed worker must not have, as (what, attempt).
    type Route = (String, Box<dyn Fn() -> io::Result<()>>);

    fn routes(probe: &Value) -> Vec<Route> {
        let root = PathBuf::from(probe["root"].as_str().unwrap());
        let windows = PathBuf::from(probe["windows"].as_str().unwrap());
        let address = probe["address"].as_str().unwrap().to_owned();
        let pipe = probe["pipe"].as_str().unwrap().to_owned();
        let host = probe["host"].as_u64().unwrap() as u32;
        let secret = root.join("secret.csv");
        let mut routes: Vec<Route> = vec![
            (
                "read an unrelated file beside the source".into(),
                Box::new(move || fs::read(&secret).map(drop)),
            ),
            ("read a system file (win.ini)".into(), {
                let w = windows.clone();
                Box::new(move || fs::read(w.join("win.ini")).map(drop))
            }),
            (
                "connect to a loopback TCP listener".into(),
                Box::new(move || TcpStream::connect(&address).map(drop)),
            ),
            (
                "bind a UDP socket".into(),
                Box::new(|| UdpSocket::bind("127.0.0.1:0").map(drop)),
            ),
            ("create a TCP socket (Winsock)".into(), Box::new(raw_socket)),
            (
                "open the network driver (\\Device\\Afd)".into(),
                Box::new(|| open_path(r"\\.\GLOBALROOT\Device\Afd")),
            ),
            (
                "open a TCP endpoint on the network driver as Winsock does (\\Device\\Afd\\Endpoint)"
                    .into(),
                Box::new(afd_endpoint),
            ),
            (
                "connect to a named pipe".into(),
                Box::new(move || open_path(&pipe)),
            ),
            ("start a process (cmd.exe)".into(), {
                let w = windows.clone();
                Box::new(move || start_process(&w))
            }),
            (
                "open the host process for query".into(),
                Box::new(move || open_process(host, PROCESS_QUERY_LIMITED_INFORMATION)),
            ),
            (
                "open the host process for memory reads".into(),
                Box::new(move || open_process(host, PROCESS_VM_READ)),
            ),
            (
                "read the user's registry (HKCU\\Software)".into(),
                Box::new(|| open_key(HKEY_CURRENT_USER, "Software")),
            ),
            (
                "read the machine's registry (HKLM\\...\\CurrentVersion)".into(),
                Box::new(|| {
                    open_key(
                        HKEY_LOCAL_MACHINE,
                        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
                    )
                }),
            ),
        ];
        if let Some(extra) = probe["extra"].as_str() {
            let extra = PathBuf::from(extra);
            routes.push((
                format!("read {}", extra.display()),
                Box::new(move || fs::read(&extra).map(drop)),
            ));
        }
        routes
    }

    /// Report SQL with DuckDB's own guards turned off: only the operating system stops it.
    fn statements(root: &Path, address: &str) -> Vec<String> {
        let at = |name: &str| root.join(name).display().to_string();
        vec![
            format!("SELECT * FROM read_text('{}')", at("secret.csv")),
            format!("SELECT * FROM read_csv('{}')", at("secret.csv")),
            format!("COPY (SELECT 1 AS n) TO '{}'", at("written.csv")),
            format!("ATTACH '{}' AS other", at("other.duckdb")),
            format!("INSTALL '{}'", at("forbidden.duckdb_extension")),
            format!("LOAD '{}'", at("forbidden.duckdb_extension")),
            // Autoloading reads json from the extension folder, where a stand-in waits.
            format!("SELECT * FROM read_json('{}')", at("data.json")),
            format!("SELECT * FROM read_csv('http://{address}/secret.csv')"),
        ]
    }

    /// DuckDB with its own guards off, extension loading and installing on, and a home and an
    /// extension folder of its own, so only the operating system can stop a statement.
    fn unguarded(root: &Path) -> Connection {
        let config = Config::default()
            .with("threads", "2")
            .unwrap()
            .enable_external_access(true)
            .unwrap()
            .enable_autoload_extension(true)
            .unwrap()
            .with("autoinstall_known_extensions", "true")
            .unwrap()
            .with("allow_unsigned_extensions", "true")
            .unwrap()
            .with("home_directory", root.join("home").display().to_string())
            .unwrap()
            .with(
                "extension_directory",
                root.join("extensions").display().to_string(),
            )
            .unwrap();
        Connection::open_in_memory_with_flags(config).unwrap()
    }

    fn child(input: &str) {
        let probe: Value = serde_json::from_str(input).unwrap();
        let root = PathBuf::from(probe["root"].as_str().unwrap());
        let source = root.join("source.duckdb");
        let staged = PathBuf::from(probe["staged"].as_str().unwrap());
        let token = probe["token"].as_u64().unwrap() as usize as HANDLE;
        // libtest runs this on its own thread, so it takes the first thread's token the worker's
        // main thread starts with, and closes the only handle to it.
        // SAFETY: a null thread means this thread; the handle was inherited for this purpose.
        assert_ne!(
            unsafe { SetThreadToken(null(), token) },
            0,
            "{}",
            io::Error::last_os_error()
        );
        // SAFETY: the inherited handle is closed once.
        unsafe { CloseHandle(token) };
        assert!(
            fs::read_to_string(root.join("secret.csv"))
                .unwrap()
                .contains("host-only-sentinel")
        );
        // The worker's own form of the path: canonical, without the `\\?\` prefix.
        let staged = super::super::plain_path(&staged.canonicalize().unwrap());
        let folder = staged.parent().unwrap().to_path_buf();
        super::super::close_inherited_descriptors().expect("inherited handles");
        super::super::resource_limits().expect("the worker's job");
        // As the worker does: take the copy's folder, restrict, and only then open the copy.
        open_through(&folder).expect("the startup token takes the copy's folder");
        super::super::start_duckdb().expect("DuckDB starts before the restriction");
        restrict(&staged).expect("OS sandbox installs");
        println!(
            "sandbox entered: token restricted, no privileges, integrity Untrusted, no impersonation, in the worker's job"
        );
        let declared =
            crate::connectors::duckdb_adapter::DuckDbConnector::open_read_only(&staged, "probe")
                .expect("DuckDB opens the copy inside the sandbox");
        assert_eq!(
            declared
                .query_row("SELECT n FROM allowed", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            7
        );
        println!(
            "allowed  DuckDB opened the run's copy inside the sandbox, through its folder: SELECT n FROM allowed = 7"
        );
        let mut opened = Vec::new();
        for (what, attempt) in routes(&probe) {
            match attempt() {
                Ok(()) => opened.push(what),
                Err(error) => println!("denied   {what}: {error}"),
            }
        }
        // The network driver itself refuses the endpoint, as access denied, not some later step.
        let endpoint = afd_endpoint().expect_err("the endpoint opens inside the sandbox");
        assert_eq!(
            endpoint.kind(),
            io::ErrorKind::PermissionDenied,
            "the endpoint is refused as STATUS_ACCESS_DENIED: {endpoint}"
        );
        let in_folder = |name: &str| PathBuf::from(format!("{}\\{name}", folder.display()));
        let outside = in_folder(&format!(
            "..\\{}\\secret.csv",
            root.file_name().unwrap().to_string_lossy()
        ));
        for (what, attempt) in [
            (
                "read the declared source itself",
                fs::read(&source).map(drop),
            ),
            (
                "write the declared source",
                fs::OpenOptions::new().write(true).open(&source).map(drop),
            ),
            (
                "truncate the declared source",
                fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&source)
                    .map(drop),
            ),
            (
                "create a file beside the source",
                fs::File::create(root.join("written.txt")).map(drop),
            ),
            ("list the source's folder", fs::read_dir(&root).map(drop)),
            (
                "write the copy (through its folder)",
                fs::OpenOptions::new().write(true).open(&staged).map(drop),
            ),
            (
                "delete the copy (through its folder)",
                fs::remove_file(&staged),
            ),
            (
                "create a file beside the copy (through its folder)",
                fs::File::create(in_folder("written.txt")).map(drop),
            ),
            ("list the copy's folder", fs::read_dir(&folder).map(drop)),
            (
                "read the copy's parent through its folder (folder\\..)",
                fs::read_dir(in_folder("..")).map(drop),
            ),
            (
                "read a file outside the copy's folder by a path through it (folder\\..\\…)",
                fs::read(&outside).map(drop),
            ),
            (
                "read an alternate stream of the copy (source.duckdb:x)",
                fs::read(in_folder(&format!("{STAGED_NAME}:x"))).map(drop),
            ),
            (
                "read through a symbolic link planted in the copy's folder, the link and its target readable by this launch (folder\\linked.duckdb)",
                fs::read(in_folder("linked.duckdb")).map(drop),
            ),
            (
                "open a junction planted in the copy's folder, readable by this launch (folder\\junction)",
                fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                    .open(in_folder("junction"))
                    .map(drop),
            ),
            (
                "read a file through that junction (folder\\junction\\linked.txt)",
                fs::read(in_folder("junction\\linked.txt")).map(drop),
            ),
            (
                "read outside the copy's folder by a \\\\?\\ path through it (\\\\?\\folder\\..\\…)",
                fs::read(format!(r"\\?\{}", outside.display())).map(drop),
            ),
            (
                "read the copy by a device path (\\\\.\\folder\\source.duckdb), which is not redirected",
                fs::read(format!(r"\\.\{}", staged.display())).map(drop),
            ),
            (
                "read the copy by a name with a trailing dot (folder\\source.duckdb.)",
                fs::read(in_folder(&format!("{STAGED_NAME}."))).map(drop),
            ),
            (
                "read the copy through its folder with a trailing dot (folder.\\source.duckdb), which is not redirected",
                fs::read(format!("{}.\\{STAGED_NAME}", folder.display())).map(drop),
            ),
            (
                "read the copy through the short form of its folder, which is not redirected",
                fs::read(format!(
                    "{}\\{STAGED_NAME}",
                    probe["short_folder"].as_str().unwrap()
                ))
                .map(drop),
            ),
            (
                "read a file outside by its short form",
                fs::read(probe["short_secret"].as_str().unwrap()).map(drop),
            ),
        ] {
            match attempt {
                Ok(()) => opened.push(what.to_owned()),
                Err(error) => println!("denied   {what}: {error}"),
            }
        }
        let connection = unguarded(&root);
        assert!(
            connection
                .query_row(
                    "SELECT current_setting('enable_external_access')",
                    [],
                    |row| row.get::<_, bool>(0)
                )
                .unwrap()
        );
        for sql in statements(&root, probe["address"].as_str().unwrap()) {
            let result = if sql.starts_with("SELECT") {
                connection.query_row(&sql, [], |_| Ok(()))
            } else {
                connection.execute_batch(&sql)
            };
            match result {
                Ok(()) => opened.push(sql),
                Err(error) => println!("denied   {sql}: {}", error.to_string().replace('\n', " ")),
            }
        }
        assert!(opened.is_empty(), "the sandbox allowed: {opened:#?}");
    }

    #[test]
    fn a_copy_over_the_limit_or_the_free_space_is_refused_before_anything_is_copied() {
        use super::platform::copy_fits;
        assert!(copy_fits(COPY_LIMIT, u64::MAX).is_ok());
        assert_eq!(
            copy_fits(COPY_LIMIT + 1, u64::MAX).unwrap_err().kind(),
            io::ErrorKind::FileTooLarge
        );
        assert_eq!(
            copy_fits(10, 10 + (1 << 30) - 1).unwrap_err().kind(),
            io::ErrorKind::StorageFull,
            "a copy leaves 1 GiB free"
        );
        assert!(copy_fits(10, 10 + (1 << 30)).is_ok());
    }

    #[test]
    fn copy_folders_of_ended_processes_are_swept_and_the_rest_kept() {
        use super::platform::{running, started, sweep};
        let temp = tempfile::tempdir().unwrap();
        let windows = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let mut child = std::process::Command::new(windows.join(r"System32\cmd.exe"))
            .args(["/c", "exit"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let created = started(child.as_raw_handle()).unwrap();
        child.wait().unwrap();
        assert!(
            !running(pid, created),
            "a process that ended has ended, though a handle to it is still open"
        );
        // SAFETY: the pseudo-handle of this process needs no closing.
        let me = started(unsafe { GetCurrentProcess() }).unwrap();
        assert!(running(std::process::id(), me));
        assert!(
            !running(std::process::id(), me + 1),
            "an id with another start time belongs to another process"
        );
        let ended = temp
            .path()
            .join(format!("richview-sql-{pid}-{created:x}-00000001"));
        let live = temp.path().join(format!(
            "richview-sql-{}-{me:x}-00000002",
            std::process::id()
        ));
        let other = temp.path().join("richview-sql-notes");
        for folder in [&ended, &live, &other] {
            fs::create_dir(folder).unwrap();
            fs::write(folder.join(STAGED_NAME), "copy").unwrap();
        }
        sweep(temp.path());
        assert!(!ended.exists(), "the copy of a run that ended is swept");
        assert!(live.exists(), "a running process keeps its copy");
        assert!(other.exists(), "a folder whose name does not parse stays");
        drop(child);
    }

    #[test]
    fn os_policy_blocks_sql_io_even_when_duckdb_external_access_is_enabled() {
        if let Ok(input) = std::env::var(PROBE) {
            child(&input);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = super::super::plain_path(&dir.path().canonicalize().unwrap());
        let windows = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let seed = Connection::open(root.join("source.duckdb")).unwrap();
        seed.execute_batch("CREATE TABLE allowed AS SELECT 7 AS n")
            .unwrap();
        let (version, platform): (String, String) = seed
            .query_row("SELECT library_version, (SELECT platform FROM pragma_platform()) FROM pragma_version()", [], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        drop(seed);
        Connection::open(root.join("other.duckdb"))
            .unwrap()
            .execute_batch("CREATE TABLE secret AS SELECT 9 AS n")
            .unwrap();
        fs::write(root.join("secret.csv"), "value\nhost-only-sentinel\n").unwrap();
        fs::write(root.join("data.json"), "[{\"value\": 1}]").unwrap();
        fs::write(
            root.join("forbidden.duckdb_extension"),
            "forbidden-extension-sentinel",
        )
        .unwrap();
        let planted = root.join("extensions").join(&version).join(&platform);
        fs::create_dir_all(&planted).unwrap();
        fs::create_dir_all(root.join("home")).unwrap();
        fs::write(
            planted.join("json.duckdb_extension"),
            "forbidden-extension-sentinel",
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let pipe = format!(
            r"\\.\pipe\richview-probe-{}-{:08x}",
            std::process::id(),
            fastrand::u32(..)
        );
        let pipe_name = wide(OsStr::new(&pipe));
        // SAFETY: the name is NUL-terminated; the default security is what any program gets.
        let server = unsafe {
            CreateNamedPipeW(
                pipe_name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                4096,
                0,
                null(),
            )
        };
        assert_ne!(
            server,
            INVALID_HANDLE_VALUE,
            "{}",
            io::Error::last_os_error()
        );
        // SAFETY: CreateNamedPipeW returned this handle.
        let _server = unsafe { OwnedHandle::from_raw_handle(server) };
        let extra = std::env::var("RICHVIEW_SQL_PROBE_EXTRA").ok();
        let probe = json!({
            "root": root, "windows": windows, "address": listener.local_addr().unwrap().to_string(),
            "pipe": pipe, "host": std::process::id(), "extra": extra, "token": 0,
        });
        // Control: outside the sandbox each route is open, so its denial inside is the sandbox's.
        for (what, attempt) in routes(&probe) {
            if what.contains("listener") {
                continue; // The listener never accepts, so a control connection would linger.
            }
            attempt().unwrap_or_else(|error| {
                panic!("control: '{what}' fails outside the sandbox too: {error}")
            });
            println!("control  {what}: open outside the sandbox");
        }
        // Control: outside the sandbox DuckDB reaches the files and extensions it is asked for
        // (writes and downloads are left out); inside, the same statements meet the OS's denial.
        let control = unguarded(&root);
        for sql in statements(&root, "")
            .into_iter()
            .filter(|sql| !sql.starts_with("COPY") && !sql.contains("http"))
        {
            let result = if sql.starts_with("SELECT") {
                control.query_row(&sql, [], |_| Ok(()))
            } else {
                control.execute_batch(&sql)
            };
            let outcome = result.map_or_else(
                |error| error.to_string().replace('\n', " "),
                |()| "ran".into(),
            );
            println!("control  {sql}: {outcome}");
        }
        drop(control);

        let lockdown = Lockdown::new().unwrap();
        let privileges = crate::win_process::token_information(
            lockdown.first_thread.as_raw_handle(),
            TokenPrivileges,
        )
        .unwrap();
        // SAFETY: the buffer holds a TOKEN_PRIVILEGES with PrivilegeCount entries.
        let kept: Vec<(u32, i32)> = unsafe {
            let header = &*(privileges.as_ptr() as *const TOKEN_PRIVILEGES);
            std::slice::from_raw_parts(header.Privileges.as_ptr(), header.PrivilegeCount as usize)
        }
        .iter()
        .map(|p| (p.Luid.LowPart, p.Luid.HighPart))
        .collect();
        let traverse = super::platform::change_notify().unwrap();
        assert_eq!(kept, [(traverse.LowPart, traverse.HighPart)]);
        println!(
            "startup token keeps one privilege, SeChangeNotifyPrivilege; the others are deleted"
        );
        // As one run does: one copy of the source, and this launch admitted to it.
        let copy = SourceCopy::new(&root.join("source.duckdb")).unwrap();
        assert_eq!(
            fs::read(&copy.source).unwrap(),
            fs::read(root.join("source.duckdb")).unwrap()
        );
        let entries = || {
            let folder = win_process::dacl_of(copy.source.parent().unwrap());
            let file = win_process::dacl_of(&copy.source);
            (folder, file)
        };
        assert_eq!(entries(), ((true, 1), (true, 1)), "the user alone");
        let earlier = Lockdown::new().unwrap().admit(&copy).unwrap();
        assert_eq!(entries(), ((true, 2), (true, 2)));
        drop(earlier); // An earlier launch of the run, whose SID comes off when it ends.
        assert_eq!(
            entries(),
            ((true, 1), (true, 1)),
            "the earlier launch's SID is off"
        );
        let admitted = lockdown.admit(&copy).unwrap();
        println!(
            "the run's copy grants the user and the running launch's SID alone; an ended launch's SID is taken off"
        );
        // Links planted in the copy's folder, the links themselves and their target readable by
        // this launch as the copy is: the redirect must still refuse them, since it opens a
        // reparse point as itself and never follows one.
        let folder = copy.source.parent().unwrap().to_path_buf();
        let linked = root.join("linked.txt");
        fs::write(&linked, "linked-sentinel").unwrap();
        let user = win_process::current_user().unwrap();
        let readable = [
            Grant {
                sid: &user,
                mask: FILE_ALL_ACCESS,
                inherit: 0,
            },
            Grant {
                sid: admitted.sid(),
                mask: FILE_GENERIC_READ,
                inherit: 0,
            },
        ];
        win_process::set_dacl(&linked, &readable).unwrap();
        std::os::windows::fs::symlink_file(&linked, folder.join("linked.duckdb"))
            .expect("the test runs with the symbolic link privilege, as Administrator");
        let junction = std::process::Command::new(windows.join(r"System32\cmd.exe"))
            .args(["/c", "mklink", "/J"])
            .arg(folder.join("junction"))
            .arg(&root)
            .output()
            .unwrap();
        assert!(junction.status.success(), "{junction:?}");
        for link in ["linked.duckdb", "junction"] {
            let handle = fs::OpenOptions::new()
                .access_mode(WRITE_DAC | READ_CONTROL)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(folder.join(link))
                .unwrap();
            win_process::set_handle_dacl(handle.as_raw_handle(), &readable).unwrap();
        }
        let short = |path: &Path| {
            let long: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            let mut buffer = vec![0u16; 1024];
            // SAFETY: `long` is NUL-terminated and the buffer holds the length passed.
            let length =
                unsafe { GetShortPathNameW(long.as_ptr(), buffer.as_mut_ptr(), 1024) } as usize;
            assert!(
                length > 0 && length < 1024,
                "{}",
                io::Error::last_os_error()
            );
            String::from_utf16(&buffer[..length]).unwrap()
        };
        let short_secret = short(&root.join("secret.csv"));
        let short_folder = short(&folder);
        println!(
            "planted in the copy's folder: a symbolic link to {} and a junction to its folder, both readable by this launch; short forms {short_secret} and {short_folder}",
            linked.display()
        );
        let job = job().unwrap();
        let mut token: HANDLE = null_mut();
        // SAFETY: duplicate the first thread's token into this process, for the child only.
        assert_ne!(
            unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    lockdown.first_thread.as_raw_handle(),
                    GetCurrentProcess(),
                    &mut token,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            0
        );
        // SAFETY: DuplicateHandle returned this handle.
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let mut probe = probe;
        probe["token"] = json!(token.as_raw_handle() as usize);
        probe["staged"] = json!(admitted.source());
        probe["short_secret"] = json!(short_secret);
        probe["short_folder"] = json!(short_folder);
        let (child_in, stdin) = win_process::pipe().unwrap();
        let (stdout, child_out) = win_process::pipe().unwrap();
        let (stderr, child_err) = win_process::pipe().unwrap();
        let name = concat!(
            module_path!(),
            "::os_policy_blocks_sql_io_even_when_duckdb_external_access_is_enabled"
        )
        .strip_prefix("richview_app::")
        .unwrap();
        let input = probe.to_string();
        let process = lockdown
            .launch(
                &std::env::current_exe().unwrap(),
                &[
                    OsStr::new("--exact"),
                    OsStr::new(name),
                    OsStr::new("--nocapture"),
                ],
                &[
                    ("SLURM_MEM_PER_NODE", OsStr::new("128M")),
                    ("SystemRoot", windows.as_os_str()),
                    (PROBE, OsStr::new(&input)),
                ],
                &[
                    child_in.as_raw_handle(),
                    child_out.as_raw_handle(),
                    child_err.as_raw_handle(),
                    token.as_raw_handle(),
                ],
                &job,
            )
            .unwrap();
        drop((child_in, child_out, child_err, token, stdin));
        let errors = std::thread::spawn(move || {
            let mut text = String::new();
            let mut file = fs::File::from(stderr);
            let _ = file.read_to_string(&mut text);
            text
        });
        let mut output = String::new();
        let _ = fs::File::from(stdout).read_to_string(&mut output);
        let errors = errors.join().unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let code = loop {
            if let Some(code) = process.try_wait().unwrap() {
                break code;
            }
            assert!(
                Instant::now() < deadline,
                "probe did not exit\n{output}\n{errors}"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        println!("{output}");
        assert_eq!(code, 0, "{output}\n{errors}");
        assert!(
            output.contains("allowed  DuckDB opened the run's copy inside the sandbox"),
            "{output}\n{errors}"
        );
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        );
        assert!(!root.join("written.csv").exists());
        assert!(!root.join("written.txt").exists());
        assert!(copy.source.exists());
        drop(job);
        let folder = copy.source.parent().unwrap().to_path_buf();
        drop(admitted);
        assert!(folder.exists(), "the run's copy stays for its next query");
        drop(copy);
        assert!(!folder.exists(), "the run's copy outlived the run");
    }
}
