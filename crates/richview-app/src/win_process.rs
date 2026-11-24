//! Child processes on Windows for the PDF printer and the SQL worker (ARC-67).
//!
//! `std::process::Command` cannot do four things these children need, so a launch here does them
//! with `CreateProcessW` itself:
//! - only the handles a launch names are inherited (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`);
//! - C runtime descriptors from 3 up reach the child through `lpReserved2`, the table the C
//!   runtime reads at startup, as Node and libuv pass them;
//! - the child starts suspended, joins its job object and gets its first thread's token before
//!   any of its code runs;
//! - it may start under a restricted token, with child process creation turned off.
//!
//! It also makes the private folders those children use, with a DACL the caller names.

use std::{
    ffi::OsStr,
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
    },
    path::Path,
    ptr::{null, null_mut},
    sync::Mutex,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{
        HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    Security::{
        ACE_FLAGS, ACL, ACL_REVISION, AddAccessAllowedAceEx, DACL_SECURITY_INFORMATION,
        GetLengthSid, GetTokenInformation, InitializeAcl, InitializeSecurityDescriptor,
        PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
        SECURITY_DESCRIPTOR, SetFileSecurityW, SetSecurityDescriptorControl,
        SetSecurityDescriptorDacl, TOKEN_INFORMATION_CLASS, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::CreateDirectoryW,
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_BASIC_UI_RESTRICTIONS,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
            JobObjectBasicUIRestrictions, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Pipes::CreatePipe,
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        SystemServices::SECURITY_DESCRIPTOR_REVISION,
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW,
            CreateProcessW, DETACHED_PROCESS, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE,
            InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW,
            SetThreadToken, TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

/// `PROCESS_CREATION_CHILD_PROCESS_RESTRICTED`: the process may not create child processes.
const CHILD_PROCESS_RESTRICTED: u32 = 1;
/// `GetCurrentProcessToken()`, a pseudo handle with query access to this process's token.
pub const PROCESS_TOKEN: HANDLE = -4isize as HANDLE;
/// Descriptor flags of the C runtime's inheritance table: open, and a pipe or a device.
const FOPEN: u8 = 0x01;
const FPIPE: u8 = 0x08;

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// A SID kept in DWORD-aligned memory: revision, count, authority, sub-authorities.
#[derive(Clone, PartialEq, Eq)]
pub struct Sid(Vec<u32>);

impl Sid {
    pub fn new(authority: u8, sub: &[u32]) -> Sid {
        let mut words = vec![u32::from_le_bytes([1, sub.len() as u8, 0, 0])];
        words.push(u32::from_le_bytes([0, 0, 0, authority]));
        words.extend(sub);
        Sid(words)
    }

    /// A copy of the SID at `sid`, whose length Windows reports.
    pub fn copy(sid: *mut core::ffi::c_void) -> Sid {
        // SAFETY: `sid` points to a valid SID inside a token buffer the caller holds.
        let len = unsafe { GetLengthSid(sid) } as usize;
        let mut words = vec![0u32; len.div_ceil(4)];
        // SAFETY: both ranges have `len` bytes and do not overlap.
        unsafe { std::ptr::copy_nonoverlapping(sid as *const u8, words.as_mut_ptr().cast(), len) };
        Sid(words)
    }

    pub fn ptr(&self) -> *mut core::ffi::c_void {
        self.0.as_ptr() as *mut _
    }

    /// The SID's size in bytes.
    pub fn bytes(&self) -> usize {
        8 + 4 * (self.0[0].to_le_bytes()[1] as usize)
    }

    /// The last sub-authority, which for an integrity label is its level.
    pub fn last(&self) -> Option<u32> {
        self.0.get(2..).and_then(<[u32]>::last).copied()
    }
}

/// One record of a token, in a buffer aligned for every token record.
pub fn token_information(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> io::Result<Vec<u64>> {
    let mut size = 0u32;
    // SAFETY: a null buffer asks only for the size.
    unsafe { GetTokenInformation(token, class, null_mut(), 0, &mut size) };
    let mut buffer = vec![0u64; (size as usize).div_ceil(8).max(1)];
    // SAFETY: the buffer holds `size` bytes, aligned for every token record.
    check(unsafe {
        GetTokenInformation(token, class, buffer.as_mut_ptr().cast(), size, &mut size)
    })?;
    Ok(buffer)
}

/// The user this process runs as.
pub fn current_user() -> io::Result<Sid> {
    let buffer = token_information(PROCESS_TOKEN, TokenUser)?;
    // SAFETY: the buffer holds a TOKEN_USER and the SID it points to.
    Ok(Sid::copy(unsafe {
        (*(buffer.as_ptr() as *const TOKEN_USER)).User.Sid
    }))
}

/// One entry of a DACL: `sid` may use `mask`, with these inheritance flags.
#[derive(Clone, Copy)]
pub struct Grant<'a> {
    pub sid: &'a Sid,
    pub mask: u32,
    pub inherit: ACE_FLAGS,
}

/// An ACL with one allowing entry per grant, in order.
pub fn acl(grants: &[Grant]) -> io::Result<Vec<u32>> {
    let size = size_of::<ACL>() + grants.iter().map(|g| 8 + g.sid.bytes()).sum::<usize>();
    let mut acl = vec![0u32; size.div_ceil(4)];
    // SAFETY: the buffer is DWORD-aligned and as large as the size passed.
    check(unsafe {
        InitializeAcl(
            acl.as_mut_ptr().cast(),
            (acl.len() * 4) as u32,
            ACL_REVISION,
        )
    })?;
    for grant in grants {
        // SAFETY: the ACL has room for this entry; the SID is valid.
        check(unsafe {
            AddAccessAllowedAceEx(
                acl.as_mut_ptr().cast(),
                ACL_REVISION,
                grant.inherit,
                grant.mask,
                grant.sid.ptr(),
            )
        })?;
    }
    Ok(acl)
}

/// Run `use_it` with a security descriptor whose DACL is exactly `grants`, protected so that it
/// inherits nothing from a parent folder.
fn with_descriptor<T>(
    grants: &[Grant],
    use_it: impl FnOnce(*mut core::ffi::c_void) -> io::Result<T>,
) -> io::Result<T> {
    let mut dacl = acl(grants)?;
    // SAFETY: zero is valid for this plain record before it is initialized.
    let mut descriptor: SECURITY_DESCRIPTOR = unsafe { std::mem::zeroed() };
    let pointer: *mut SECURITY_DESCRIPTOR = &mut descriptor;
    // SAFETY: the descriptor and the ACL it points to live through `use_it`.
    check(unsafe { InitializeSecurityDescriptor(pointer.cast(), SECURITY_DESCRIPTOR_REVISION) })?;
    check(unsafe { SetSecurityDescriptorDacl(pointer.cast(), 1, dacl.as_mut_ptr().cast(), 0) })?;
    check(unsafe {
        SetSecurityDescriptorControl(pointer.cast(), SE_DACL_PROTECTED, SE_DACL_PROTECTED)
    })?;
    use_it(pointer.cast())
}

/// Create `path` as a new folder whose DACL is exactly `grants`: protected, so it inherits
/// nothing from its parent. It fails if anything already has that name.
pub fn create_private_dir(path: &Path, grants: &[Grant]) -> io::Result<()> {
    with_descriptor(grants, |descriptor| {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let name = wide(path.as_os_str());
        // SAFETY: the name is NUL-terminated and the attributes point to the descriptor.
        check(unsafe { CreateDirectoryW(name.as_ptr(), &attributes) })
    })
}

/// Replace the DACL of the file or folder at `path` with exactly `grants`, protected so that it
/// inherits nothing. Nothing inside a folder changes with it.
pub fn set_dacl(path: &Path, grants: &[Grant]) -> io::Result<()> {
    with_descriptor(grants, |descriptor| {
        let name = wide(path.as_os_str());
        // SAFETY: the name is NUL-terminated and the descriptor is valid for the call.
        check(unsafe {
            SetFileSecurityW(
                name.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            )
        })
    })
}

/// An anonymous pipe, `(read end, write end)`; neither end is inheritable until a launch names it.
pub fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let (mut read, mut write): (HANDLE, HANDLE) = (null_mut(), null_mut());
    // SAFETY: both out-pointers are valid; a null attribute pointer makes both ends private.
    check(unsafe { CreatePipe(&mut read, &mut write, null(), 0) })?;
    // SAFETY: CreatePipe succeeded, so both handles are open and owned by nobody else.
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}

/// The `NUL` device, for a standard handle the child must not use.
pub fn null_device() -> io::Result<OwnedHandle> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("NUL")
        .map(OwnedHandle::from)
}

/// A job object: closing it ends every process still in it.
pub struct Job(OwnedHandle);

impl Job {
    /// A job whose processes all end when the last handle to it closes.
    pub fn kill_on_close() -> io::Result<Job> {
        // SAFETY: zero is a valid value for every field of this plain record.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        Job::new(&limits, None)
    }

    /// A job with these limits and, when given, these `JOB_OBJECT_UILIMIT_*` restrictions.
    pub fn new(limits: &JOBOBJECT_EXTENDED_LIMIT_INFORMATION, ui: Option<u32>) -> io::Result<Job> {
        // SAFETY: no security attributes and no name: an unnamed job only this process holds.
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateJobObjectW returned a new handle that nobody else owns.
        let job = Job(unsafe { OwnedHandle::from_raw_handle(handle) });
        // SAFETY: the record is the class's documented type and size.
        check(unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        })?;
        if let Some(restrictions) = ui {
            let ui = JOBOBJECT_BASIC_UI_RESTRICTIONS {
                UIRestrictionsClass: restrictions,
            };
            // SAFETY: as above, for the UI restriction class.
            check(unsafe {
                SetInformationJobObject(
                    job.raw(),
                    JobObjectBasicUIRestrictions,
                    (&ui as *const JOBOBJECT_BASIC_UI_RESTRICTIONS).cast(),
                    size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
                )
            })?;
        }
        Ok(job)
    }

    fn raw(&self) -> HANDLE {
        self.0.as_raw_handle()
    }

    /// End every process in the job.
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: the handle is this job's, open for the life of `self`.
        check(unsafe { TerminateJobObject(self.raw(), 1) })
    }

    /// How many processes of the job still run.
    pub fn active_processes(&self) -> io::Result<u32> {
        // SAFETY: zero is valid for this plain record; the call fills it.
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: the record is the class's documented type and size.
        check(unsafe {
            QueryInformationJobObject(
                self.raw(),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        })?;
        Ok(info.ActiveProcesses)
    }
}

/// A process this module started.
pub struct Process {
    handle: OwnedHandle,
}

impl Process {
    /// The exit code once the process has ended, or `None` while it runs.
    pub fn try_wait(&self) -> io::Result<Option<u32>> {
        self.wait_for(Some(Duration::ZERO))
    }

    /// Wait at most `timeout` (for ever when `None`) and return the exit code if it ended.
    pub fn wait_for(&self, timeout: Option<Duration>) -> io::Result<Option<u32>> {
        let millis = timeout.map_or(INFINITE, |t| t.as_millis().min(u32::MAX as u128 - 1) as u32);
        // SAFETY: the handle is this process's, open for the life of `self`.
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), millis) } {
            WAIT_OBJECT_0 => {
                let mut code = 0u32;
                // SAFETY: as above; `code` is a valid out-pointer.
                check(unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut code) })?;
                Ok(Some(code))
            }
            WAIT_TIMEOUT => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }

    /// End the process if it still runs.
    pub fn kill(&self) -> io::Result<()> {
        // SAFETY: the handle is this process's; ending an ended process is harmless.
        if unsafe { TerminateProcess(self.handle.as_raw_handle(), 1) } == 0
            && self.try_wait()?.is_none()
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Bytes the process holds: the larger of its working set and its private commit.
    pub fn resident_bytes(&self) -> io::Result<u64> {
        // SAFETY: zero is valid for this plain record; the call fills it.
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: the record has the size its `cb` names.
        check(unsafe {
            K32GetProcessMemoryInfo(self.handle.as_raw_handle(), &mut counters, counters.cb)
        })?;
        Ok(counters.WorkingSetSize.max(counters.PagefileUsage) as u64)
    }
}

/// A restricted token for the process, with the token its first thread runs under until the
/// child reverts to the process token itself.
pub struct Tokens<'a> {
    pub primary: &'a OwnedHandle,
    pub first_thread: &'a OwnedHandle,
    /// The security of the new process and thread objects.
    pub objects: &'a SECURITY_ATTRIBUTES,
}

pub struct Launch<'a> {
    pub program: &'a Path,
    pub args: &'a [&'a OsStr],
    /// The whole environment, or `None` to pass this process's on.
    pub env: Option<&'a [(&'a OsStr, &'a OsStr)]>,
    pub cwd: Option<&'a Path>,
    /// Descriptor 0, 1, 2 and then 3 and up, in order. The first three are also the standard
    /// handles. They are the only handles the child inherits.
    pub descriptors: &'a [RawHandle],
    pub job: &'a Job,
    pub tokens: Option<Tokens<'a>>,
    pub no_child_processes: bool,
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain([0]).collect()
}

/// Quote one argument the way the C runtime splits a command line.
fn push_argument(line: &mut Vec<u16>, argument: &OsStr) {
    let (quote, backslash) = (b'"' as u16, b'\\' as u16);
    let units: Vec<u16> = argument.encode_wide().collect();
    if !line.is_empty() {
        line.push(b' ' as u16);
    }
    if !units.is_empty()
        && !units
            .iter()
            .any(|&u| u == b' ' as u16 || u == b'\t' as u16 || u == quote)
    {
        line.extend(units);
        return;
    }
    line.push(quote);
    let mut backslashes = 0;
    for unit in units {
        if unit == backslash {
            backslashes += 1;
        } else {
            if unit == quote {
                line.extend(std::iter::repeat_n(backslash, backslashes + 1));
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    line.extend(std::iter::repeat_n(backslash, backslashes));
    line.push(quote);
}

fn command_line(program: &Path, args: &[&OsStr]) -> Vec<u16> {
    let mut line = Vec::new();
    push_argument(&mut line, program.as_os_str());
    for argument in args {
        push_argument(&mut line, argument);
    }
    line.push(0);
    line
}

fn environment_block(env: &[(&OsStr, &OsStr)]) -> Vec<u16> {
    let mut block = Vec::new();
    for (name, value) in env {
        block.extend(name.encode_wide());
        block.push(b'=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

/// The C runtime's inheritance table: a count, one flag byte per descriptor, then the handles.
fn descriptor_table(handles: &[RawHandle]) -> Vec<u8> {
    let mut table = (handles.len() as i32).to_le_bytes().to_vec();
    table.extend(handles.iter().map(|_| FOPEN | FPIPE));
    for handle in handles {
        table.extend((*handle as usize).to_le_bytes());
    }
    table
}

/// Held from marking a launch's handles inheritable until they are private again, so that no
/// other child of this process, from here or from [`spawn`], inherits a launch's pipe ends.
static LAUNCHING: Mutex<()> = Mutex::new(());

fn launching() -> std::sync::MutexGuard<'static, ()> {
    LAUNCHING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Spawn `command` while no launch here has its handles inheritable. `std::process::Command`
/// lets a child inherit every inheritable handle of this process, so every Windows child this
/// crate starts with it starts through here.
pub fn spawn(command: &mut std::process::Command) -> io::Result<std::process::Child> {
    let _launching = launching();
    command.spawn()
}

/// The launch's handles, inheritable until this is dropped.
struct Inheritable<'a>(&'a [RawHandle]);

impl Inheritable<'_> {
    fn mark(handles: &[RawHandle]) -> io::Result<Inheritable<'_>> {
        for (marked, &handle) in handles.iter().enumerate() {
            // SAFETY: each handle is open in this process for the duration of the launch.
            if let Err(error) = check(unsafe {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT)
            }) {
                drop(Inheritable(&handles[..marked]));
                return Err(error);
            }
        }
        Ok(Inheritable(handles))
    }
}

impl Drop for Inheritable<'_> {
    fn drop(&mut self) {
        for &handle in self.0 {
            // SAFETY: as in `mark`; the child has its own copy, so this changes only ours.
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
        }
    }
}

/// Start `launch` suspended, put it in its job, give its first thread its token, then let it run.
/// A child that fails any step before it runs is ended.
///
/// Launches through this module take turns, and each launch's handles are inheritable only
/// while it creates its child. The crate's other Windows children start through [`spawn`], which
/// waits its turn too.
pub fn launch(launch: &Launch) -> io::Result<Process> {
    let handles: Vec<RawHandle> = {
        let mut unique = Vec::new();
        for &handle in launch.descriptors {
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::other("a child descriptor has no handle"));
            }
            if !unique.contains(&handle) {
                unique.push(handle);
            }
        }
        unique
    };
    if launch.descriptors.len() < 3 {
        return Err(io::Error::other("a child needs its three standard handles"));
    }
    let attributes = 1 + u32::from(launch.no_child_processes);
    let mut size = 0usize;
    // SAFETY: a null list asks only for the size; the call fails by design with that size set.
    unsafe { InitializeProcThreadAttributeList(null_mut(), attributes, 0, &mut size) };
    let mut storage = vec![0u64; size.div_ceil(8)];
    let list: LPPROC_THREAD_ATTRIBUTE_LIST = storage.as_mut_ptr().cast();
    // SAFETY: `storage` has the size the first call asked for and outlives the list.
    check(unsafe { InitializeProcThreadAttributeList(list, attributes, 0, &mut size) })?;
    struct List(LPPROC_THREAD_ATTRIBUTE_LIST);
    impl Drop for List {
        fn drop(&mut self) {
            // SAFETY: the list was initialized and is deleted once.
            unsafe { DeleteProcThreadAttributeList(self.0) };
        }
    }
    let _list = List(list);
    // SAFETY: the handle array and the policy word outlive CreateProcess below.
    check(unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            handles.as_ptr().cast(),
            handles.len() * size_of::<HANDLE>(),
            null_mut(),
            null(),
        )
    })?;
    let policy = CHILD_PROCESS_RESTRICTED;
    if launch.no_child_processes {
        // SAFETY: as above.
        check(unsafe {
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY as usize,
                (&policy as *const u32).cast(),
                size_of::<u32>(),
                null_mut(),
                null(),
            )
        })?;
    }
    let mut table = descriptor_table(launch.descriptors);
    // SAFETY: zero is valid for this plain record; the fields that matter are set below.
    let mut info: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    info.StartupInfo.hStdInput = launch.descriptors[0];
    info.StartupInfo.hStdOutput = launch.descriptors[1];
    info.StartupInfo.hStdError = launch.descriptors[2];
    if launch.descriptors.len() > 3 {
        info.StartupInfo.cbReserved2 = u16::try_from(table.len())
            .map_err(|_| io::Error::other("too many child descriptors"))?;
        info.StartupInfo.lpReserved2 = table.as_mut_ptr();
    }
    info.lpAttributeList = list;
    let program = wide(launch.program.as_os_str());
    let mut line = command_line(launch.program, launch.args);
    let environment = launch.env.map(environment_block);
    let cwd = launch.cwd.map(|cwd| wide(cwd.as_os_str()));
    // A confined process gets no console: Windows would start a console host as its child, under
    // its own restricted token, and that start fails (STATUS_DLL_INIT_FAILED). Its standard
    // handles are the pipes above either way.
    let console = if launch.tokens.is_some() {
        DETACHED_PROCESS
    } else {
        CREATE_NO_WINDOW
    };
    let flags =
        CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | console;
    // SAFETY: zero is valid for this plain record; CreateProcess fills it.
    let mut created: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let objects = launch.tokens.as_ref().map_or(null(), |tokens| {
        tokens.objects as *const SECURITY_ATTRIBUTES
    });
    let environment_ptr = environment
        .as_ref()
        .map_or(null(), |block| block.as_ptr().cast());
    let cwd_ptr = cwd.as_ref().map_or(null(), |cwd| cwd.as_ptr());
    let launching = launching();
    let inheritable = Inheritable::mark(&handles)?;
    // SAFETY: every pointer refers to a live, NUL-terminated or correctly sized buffer above;
    // the command line is mutable as CreateProcess requires.
    let ok = unsafe {
        match &launch.tokens {
            Some(tokens) => CreateProcessAsUserW(
                tokens.primary.as_raw_handle(),
                program.as_ptr(),
                line.as_mut_ptr(),
                objects,
                objects,
                1,
                flags,
                environment_ptr,
                cwd_ptr,
                &info.StartupInfo,
                &mut created,
            ),
            None => CreateProcessW(
                program.as_ptr(),
                line.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                environment_ptr,
                cwd_ptr,
                &info.StartupInfo,
                &mut created,
            ),
        }
    };
    // Read the error before the handles are made private again, which may overwrite it.
    let outcome = check(ok);
    drop(inheritable);
    drop(launching);
    outcome?;
    // SAFETY: CreateProcess succeeded, so both handles are new and ours alone.
    let (process, thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(created.hProcess),
            OwnedHandle::from_raw_handle(created.hThread),
        )
    };
    let child = Process { handle: process };
    let started = (|| {
        // SAFETY: both handles are open; the child has run no code yet.
        check(unsafe { AssignProcessToJobObject(launch.job.raw(), child.handle.as_raw_handle()) })?;
        if let Some(tokens) = &launch.tokens {
            let thread_handle = thread.as_raw_handle();
            // SAFETY: the suspended first thread takes the token before it runs.
            check(unsafe { SetThreadToken(&thread_handle, tokens.first_thread.as_raw_handle()) })?;
        }
        // SAFETY: the thread handle is open; resuming runs the child.
        if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    })();
    if let Err(error) = started {
        let _ = child.kill();
        return Err(error);
    }
    Ok(child)
}

#[cfg(test)]
/// Set the DACL of the object `handle` opened, a link itself for example, to `grants` alone.
pub fn set_handle_dacl(handle: RawHandle, grants: &[Grant]) -> io::Result<()> {
    use windows_sys::Win32::Security::SetKernelObjectSecurity;
    with_descriptor(grants, |descriptor| {
        // SAFETY: the handle is open with WRITE_DAC and the descriptor is valid for the call.
        check(unsafe {
            SetKernelObjectSecurity(
                handle,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            )
        })
    })
}

#[cfg(test)]
/// The DACL of `path`: whether it is protected, and how many entries it has.
pub fn dacl_of(path: &Path) -> (bool, u16) {
    use windows_sys::Win32::Security::{
        GetFileSecurityW, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    };
    let name = wide(path.as_os_str());
    let mut size = 0u32;
    // SAFETY: a null buffer asks only for the size.
    unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION,
            null_mut(),
            0,
            &mut size,
        )
    };
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: the buffer holds `size` bytes.
    check(unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    })
    .unwrap();
    let (mut control, mut revision) = (0u16, 0u32);
    // SAFETY: the buffer holds a self-relative security descriptor.
    check(unsafe {
        GetSecurityDescriptorControl(buffer.as_mut_ptr().cast(), &mut control, &mut revision)
    })
    .unwrap();
    let (mut present, mut dacl, mut defaulted) = (0, null_mut::<ACL>(), 0);
    // SAFETY: as above; the out-pointers are valid.
    check(unsafe {
        GetSecurityDescriptorDacl(
            buffer.as_mut_ptr().cast(),
            &mut present,
            &mut dacl,
            &mut defaulted,
        )
    })
    .unwrap();
    assert!(present != 0 && !dacl.is_null());
    // SAFETY: the DACL lies inside `buffer`, which is still alive.
    (control & SE_DACL_PROTECTED != 0, unsafe {
        (*dacl).AceCount
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quoted(args: &[&str]) -> String {
        let mut line = Vec::new();
        for arg in args {
            push_argument(&mut line, OsStr::new(arg));
        }
        String::from_utf16(&line).unwrap()
    }

    #[test]
    fn arguments_are_quoted_as_the_c_runtime_splits_them() {
        assert_eq!(quoted(&["a", "b c", ""]), r#"a "b c" """#);
        assert_eq!(quoted(&[r#"say "hi""#]), r#""say \"hi\"""#);
        assert_eq!(quoted(&[r"C:\a b\"]), r#""C:\a b\\""#);
        assert_eq!(quoted(&[r"C:\plain\path"]), r"C:\plain\path");
    }

    #[test]
    fn the_descriptor_table_lists_every_descriptor_as_an_open_pipe() {
        let handles = [1usize as RawHandle, 2usize as RawHandle];
        let table = descriptor_table(&handles);
        assert_eq!(&table[..4], &2i32.to_le_bytes());
        assert_eq!(&table[4..6], &[FOPEN | FPIPE, FOPEN | FPIPE]);
        assert_eq!(&table[6..14], &1usize.to_le_bytes());
        assert_eq!(table.len(), 4 + 2 + 2 * size_of::<usize>());
    }

    #[test]
    fn an_empty_environment_is_two_terminators() {
        assert_eq!(environment_block(&[]), [0, 0]);
        let block = environment_block(&[(OsStr::new("A"), OsStr::new("1"))]);
        assert_eq!(String::from_utf16(&block).unwrap(), "A=1\0\0");
    }

    #[test]
    fn a_private_folder_grants_only_its_owner_and_inherits_nothing() {
        use windows_sys::Win32::{
            Security::{CONTAINER_INHERIT_ACE, OBJECT_INHERIT_ACE},
            Storage::FileSystem::FILE_ALL_ACCESS,
        };
        let user = current_user().unwrap();
        let dir = std::env::temp_dir().join(format!("richview-private-{:08x}", fastrand::u32(..)));
        let owner = [Grant {
            sid: &user,
            mask: FILE_ALL_ACCESS,
            inherit: OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
        }];
        create_private_dir(&dir, &owner).unwrap();
        assert_eq!(dacl_of(&dir), (true, 1));
        // A file made inside carries the owner's entry alone, not the temporary folder's.
        std::fs::write(dir.join("profile.txt"), "x").unwrap();
        assert_eq!(dacl_of(&dir.join("profile.txt")).1, 1);
        assert!(
            create_private_dir(&dir, &owner).is_err(),
            "an existing name is refused"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_job_ends_the_tree_it_holds() {
        let job = Job::kill_on_close().unwrap();
        let (stdin, _keep) = pipe().unwrap();
        let null = null_device().unwrap();
        let cmd = std::env::var_os("ComSpec").unwrap();
        // The shell starts a second process that would outlive it without the job.
        let child = launch(&Launch {
            program: Path::new(&cmd),
            args: &[
                OsStr::new("/c"),
                OsStr::new("start /b ping -n 60 127.0.0.1 >NUL & ping -n 60 127.0.0.1 >NUL"),
            ],
            env: None,
            cwd: None,
            descriptors: &[
                stdin.as_raw_handle(),
                null.as_raw_handle(),
                null.as_raw_handle(),
            ],
            job: &job,
            tokens: None,
            no_child_processes: false,
        })
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while job.active_processes().unwrap() < 3 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            job.active_processes().unwrap() >= 3,
            "the shell's children did not start"
        );
        job.terminate().unwrap();
        assert!(
            child
                .wait_for(Some(Duration::from_secs(5)))
                .unwrap()
                .is_some()
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while job.active_processes().unwrap() > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(job.active_processes().unwrap(), 0);
    }

    /// `cmd /c` that keeps running for about three seconds, its standard handles on NUL.
    fn lingering(command: &mut std::process::Command) -> &mut std::process::Command {
        use std::process::Stdio;
        command
            .args(["/c", "ping -n 4 127.0.0.1 >nul"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
    }

    /// A pipe's reader sees its end only once no process holds the write end: how long it waits
    /// shows whether a child inherited that end.
    fn wait_for_end(mut read: std::io::PipeReader) -> Duration {
        use std::io::Read;
        let start = std::time::Instant::now();
        read.read_to_end(&mut Vec::new()).unwrap();
        start.elapsed()
    }

    #[test]
    fn a_command_child_never_inherits_the_pipe_end_of_a_launch_in_progress() {
        use std::{process::Command, sync::mpsc, thread};
        let cmd = Path::new(&std::env::var_os("SystemRoot").unwrap()).join(r"System32\cmd.exe");

        // Control: a plain spawn while a launch has marked its pipe end inheritable gets a copy.
        let (read, write) = std::io::pipe().unwrap();
        let handles = [write.as_raw_handle()];
        let mut control = {
            let _marked = Inheritable::mark(&handles).unwrap();
            lingering(&mut Command::new(&cmd)).spawn().unwrap()
        };
        drop(write);
        assert!(
            wait_for_end(read) >= Duration::from_secs(2),
            "the control child held the launch's pipe end"
        );
        control.wait().unwrap();

        // Through `spawn`, the child starts only once the launch's handles are private again.
        let (read, write) = std::io::pipe().unwrap();
        let handles = [write.as_raw_handle()];
        let launch = launching();
        let marked = Inheritable::mark(&handles).unwrap();
        let (started, spawned) = mpsc::channel();
        let spawner = thread::spawn({
            let cmd = cmd.clone();
            move || {
                let child = spawn(lingering(&mut Command::new(&cmd)));
                started.send(()).unwrap();
                child
            }
        });
        assert!(
            spawned.recv_timeout(Duration::from_millis(500)).is_err(),
            "spawn waits for the launch in progress"
        );
        drop(marked);
        drop(launch);
        let mut child = spawner.join().unwrap().unwrap();
        drop(write);
        assert!(
            wait_for_end(read) < Duration::from_secs(1),
            "the child holds no copy of the launch's pipe end"
        );
        child.wait().unwrap();
    }
}
