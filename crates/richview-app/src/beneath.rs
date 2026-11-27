//! Files opened, created and replaced beneath a folder that was opened once.
//!
//! A path that was checked and is then opened again can name another file by the time it is
//! opened: a folder swapped for a link in between. A name looked up relative to a folder that is
//! already open, never through a link, cannot. Every call here takes one name, never a path, and
//! looks it up in the folder it is called on. Only where the system has such handles; the callers
//! keep their pathnames elsewhere.

use std::ffi::{CString, OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

/// An open folder.
pub(crate) struct Dir(OwnedFd);

/// What a name in a folder is, looked at without following it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Folder,
    /// A regular file, with its permission bits (no set-ID or sticky bit) and its group.
    File(crate::input_limits::Perm),
    Link,
    Other,
}

fn c_name(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a file name cannot hold a NUL"))
}

fn owned(fd: libc::c_int) -> io::Result<OwnedFd> {
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` is a descriptor just opened, and nothing else owns it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn done(status: libc::c_int) -> io::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

impl Dir {
    /// The folder at `path`. The path itself is looked up the ordinary way, so the folder a person
    /// named may sit behind links; with `follow_last` false, the last part of it may not be one.
    pub(crate) fn open_path(path: &Path, follow_last: bool) -> io::Result<Self> {
        let name = c_name(path.as_os_str())?;
        let mut flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
        if !follow_last {
            flags |= libc::O_NOFOLLOW;
        }
        // SAFETY: `name` is a valid C string for the whole call.
        owned(unsafe { libc::open(name.as_ptr(), flags) }).map(Self)
    }

    /// Another handle on this same folder.
    pub(crate) fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    /// A folder in this one, never through a link.
    pub(crate) fn open_dir(&self, name: &OsStr) -> io::Result<Self> {
        let name = c_name(name)?;
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: `name` is a valid C string and `self` an open directory.
        owned(unsafe { libc::openat(self.0.as_raw_fd(), name.as_ptr(), flags) }).map(Self)
    }

    /// What `name` is in this folder, without following it.
    // `st_mode` is 16 bits on some systems and 32 on others.
    #[allow(clippy::unnecessary_cast)]
    pub(crate) fn kind(&self, name: &OsStr) -> io::Result<Kind> {
        let name = c_name(name)?;
        let mut found = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `name` is a valid C string, `self` an open directory, and `found` room for a `stat`.
        done(unsafe {
            libc::fstatat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                found.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;
        // SAFETY: `fstatat` filled it in.
        let found = unsafe { found.assume_init() };
        let mode = found.st_mode as u32;
        let shape = mode & libc::S_IFMT as u32;
        Ok(if shape == libc::S_IFDIR as u32 {
            Kind::Folder
        } else if shape == libc::S_IFREG as u32 {
            Kind::File(crate::input_limits::Perm {
                mode: mode & 0o777,
                group: Some(found.st_gid as u32),
            })
        } else if shape == libc::S_IFLNK as u32 {
            Kind::Link
        } else {
            Kind::Other
        })
    }

    /// Makes a folder in this one.
    pub(crate) fn make_dir(&self, name: &OsStr) -> io::Result<()> {
        let name = c_name(name)?;
        // SAFETY: `name` is a valid C string and `self` an open directory.
        done(unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o777) })
    }

    /// Where the link `name` points.
    pub(crate) fn read_link(&self, name: &OsStr) -> io::Result<OsString> {
        let name = c_name(name)?;
        let mut target = vec![0u8; 4096];
        // SAFETY: `name` is a valid C string, `self` an open directory, and `target` has the room it is told.
        let length = unsafe {
            libc::readlinkat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                target.as_mut_ptr().cast(),
                target.len(),
            )
        };
        let length = usize::try_from(length).map_err(|_| io::Error::last_os_error())?;
        if length == target.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a link too long",
            ));
        }
        target.truncate(length);
        Ok(OsString::from_vec(target))
    }

    /// Opens the file `name` for reading, never through a link. A pipe or device that took its
    /// place does not block the open.
    pub(crate) fn open_file(&self, name: &OsStr) -> io::Result<File> {
        let name = c_name(name)?;
        let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        // SAFETY: `name` is a valid C string and `self` an open directory.
        let fd = owned(unsafe { libc::openat(self.0.as_raw_fd(), name.as_ptr(), flags) })?;
        Ok(File::from(fd))
    }

    /// Creates the file `name` for writing, failing if anything is there. A system default is
    /// applied to `mode`, as it is for any new file.
    pub(crate) fn create_new(&self, name: &OsStr, mode: u32) -> io::Result<File> {
        let name = c_name(name)?;
        let flags =
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: `name` is a valid C string and `self` an open directory.
        let fd = owned(unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                flags,
                mode as libc::c_uint,
            )
        })?;
        Ok(File::from(fd))
    }

    /// Renames `from` to `to`, both in this folder.
    pub(crate) fn rename(&self, from: &OsStr, to: &OsStr) -> io::Result<()> {
        let (from, to) = (c_name(from)?, c_name(to)?);
        // SAFETY: both names are valid C strings and `self` an open directory.
        done(unsafe {
            libc::renameat(
                self.0.as_raw_fd(),
                from.as_ptr(),
                self.0.as_raw_fd(),
                to.as_ptr(),
            )
        })
    }

    /// Removes the file `name`.
    pub(crate) fn remove_file(&self, name: &OsStr) -> io::Result<()> {
        let name = c_name(name)?;
        // SAFETY: `name` is a valid C string and `self` an open directory.
        done(unsafe { libc::unlinkat(self.0.as_raw_fd(), name.as_ptr(), 0) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn a_name_is_looked_up_in_the_folder_that_was_opened_and_never_through_a_link() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("inside")).unwrap();
        std::fs::write(root.path().join("inside/a.txt"), "inside").unwrap();
        std::fs::write(elsewhere.path().join("a.txt"), "elsewhere").unwrap();
        let top = Dir::open_path(root.path(), true).unwrap();
        let inside = top.open_dir(OsStr::new("inside")).unwrap();
        // The folder is replaced by a link after it was opened: the open handle still names the old one.
        std::fs::rename(root.path().join("inside"), root.path().join("moved")).unwrap();
        symlink(elsewhere.path(), root.path().join("inside")).unwrap();
        let mut text = String::new();
        io::Read::read_to_string(
            &mut inside.open_file(OsStr::new("a.txt")).unwrap(),
            &mut text,
        )
        .unwrap();
        assert_eq!(text, "inside");
        // A fresh walk down the same name meets the link and refuses it.
        assert!(top.open_dir(OsStr::new("inside")).is_err());
        assert_eq!(top.kind(OsStr::new("inside")).unwrap(), Kind::Link);
        assert_eq!(top.kind(OsStr::new("moved")).unwrap(), Kind::Folder);
    }

    #[test]
    fn the_kind_of_a_name_reports_a_files_permission_bits_and_no_more() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("private.txt");
        std::fs::write(&path, "x").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o4750)).unwrap();
        let top = Dir::open_path(root.path(), true).unwrap();
        let Kind::File(found) = top.kind(OsStr::new("private.txt")).unwrap() else {
            panic!("a regular file is a file");
        };
        assert_eq!(found.mode, 0o750);
        assert!(found.group.is_some());
        assert!(top.kind(OsStr::new("absent")).is_err());
    }
}
