//! The disk side of a compare-and-swap save (ADR-0013): the version on disk, the directory lock,
//! the atomic write and the private backup of the bytes an acknowledged overwrite replaces; for
//! protected writes (ADR-0015) also what is at the path, read without following a final symlink
//! or blocking on a FIFO.

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags, Stat};
use rustix::io::Errno;

/// One version of the config file on disk: absent, or exactly these bytes. Content, not size or
/// time, is the version, so an identical rewrite is the same version and any edit is a new one.
#[derive(Clone, PartialEq, Eq)]
pub struct DiskVersion(pub(super) Option<Vec<u8>>);

impl DiskVersion {
    pub(super) const ABSENT: Self = Self(None);

    /// No file was at the path (a missing symlink target counts as absent).
    pub fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}

impl fmt::Debug for DiskVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            None => f.write_str("DiskVersion(absent)"),
            Some(bytes) => write!(f, "DiskVersion({} bytes)", bytes.len()),
        }
    }
}

/// The version at `path` now; reads follow symlinks, as loading does.
pub(super) fn version_at(path: &Path) -> io::Result<DiskVersion> {
    match fs::read(path) {
        Ok(bytes) => Ok(DiskVersion(Some(bytes))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(DiskVersion::ABSENT),
        Err(e) => Err(e),
    }
}

/// What is at the config path, for a protected read or write.
#[derive(PartialEq, Eq)]
pub(super) enum Found {
    Absent,
    File { bytes: Vec<u8>, mode: u32 },
}

impl Found {
    pub(super) fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Absent => None,
            Self::File { bytes, .. } => Some(bytes),
        }
    }

    pub(super) fn into_version(self) -> DiskVersion {
        match self {
            Self::Absent => DiskVersion::ABSENT,
            Self::File { bytes, .. } => DiskVersion(Some(bytes)),
        }
    }
}

/// Why a protected read or write refuses the path.
pub(super) enum Refused {
    Symlink,
    Unsupported(String),
    Io(io::Error),
}

impl From<Errno> for Refused {
    fn from(e: Errno) -> Self {
        Self::Io(e.into())
    }
}

/// What a protected write would replace, never following a final symlink: nothing, or a regular
/// file with exactly one name (its bytes and permission bits, read from one descriptor). A
/// symlink (dangling or not), a hard-linked file, a directory, FIFO, socket or device is refused
/// without being opened, so nothing blocks and nothing outside the path is touched.
pub(super) fn probe(path: &Path) -> Result<Found, Refused> {
    probe_at(CWD, path)
}

pub(super) fn probe_at(dir: impl AsFd, path: &Path) -> Result<Found, Refused> {
    let dir = dir.as_fd();
    let stat = match rustix::fs::statat(dir, path, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(Found::Absent),
        Err(e) => return Err(e.into()),
    };
    match FileType::from_raw_mode(stat.st_mode) {
        FileType::Symlink => return Err(Refused::Symlink),
        FileType::RegularFile => {}
        other => return Err(Refused::Unsupported(kind_name(other).to_owned())),
    }
    let (bytes, stat) = read_regular(dir, path, OFlags::NOFOLLOW)?;
    if stat.st_nlink != 1 {
        return Err(Refused::Unsupported(format!("file with {} names (hard links)", stat.st_nlink)));
    }
    Ok(Found::File { bytes, mode: stat.st_mode & 0o7777 })
}

/// The version at `path` for a protected read: like [`version_at`] (a symlink is followed, a
/// missing target is absent), but anything other than a regular file is refused unopened.
pub(super) fn version_followed(path: &Path) -> Result<DiskVersion, Refused> {
    let stat = match rustix::fs::statat(CWD, path, AtFlags::empty()) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(DiskVersion::ABSENT),
        Err(e) => return Err(e.into()),
    };
    match FileType::from_raw_mode(stat.st_mode) {
        FileType::RegularFile => Ok(DiskVersion(Some(read_regular(CWD, path, OFlags::empty())?.0))),
        other => Err(Refused::Unsupported(kind_name(other).to_owned())),
    }
}

/// Opens without blocking and reads `path`, checking on the descriptor that it is still a
/// regular file (it may have been swapped since it was looked at).
fn read_regular(dir: impl AsFd, path: &Path, extra: OFlags) -> Result<(Vec<u8>, Stat), Refused> {
    let flags = OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC | OFlags::NOCTTY | extra;
    let fd = match rustix::fs::openat(dir, path, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::LOOP) if extra.contains(OFlags::NOFOLLOW) => return Err(Refused::Symlink),
        Err(e) => return Err(e.into()),
    };
    let stat = rustix::fs::fstat(&fd)?;
    let kind = FileType::from_raw_mode(stat.st_mode);
    if kind != FileType::RegularFile {
        return Err(Refused::Unsupported(kind_name(kind).to_owned()));
    }
    let mut bytes = Vec::new();
    let mut file = fs::File::from(fd);
    file.read_to_end(&mut bytes).map_err(Refused::Io)?;
    Ok((bytes, rustix::fs::fstat(file)?))
}

fn kind_name(kind: FileType) -> &'static str {
    match kind {
        FileType::Directory => "directory",
        FileType::Fifo => "FIFO (named pipe)",
        FileType::Socket => "socket",
        FileType::CharacterDevice | FileType::BlockDevice => "device",
        FileType::Symlink => "symbolic link",
        _ => "special file",
    }
}

/// An exclusive `flock` on the config's directory, released when dropped. Every library writer
/// takes it, so a check of the version and the rename that follows are never interleaved with
/// another library save. The directory itself is locked, so no lock file is ever left behind.
pub(super) struct DirLock(fs::File);

impl DirLock {
    pub(super) fn take(dir: &Path) -> io::Result<Self> {
        let handle = rustix::fs::openat(
            CWD,
            dir,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Self::from_dir(handle)
    }

    pub(super) fn from_dir(dir: OwnedFd) -> io::Result<Self> {
        let file = fs::File::from(dir);
        file.lock()?;
        Ok(Self(file))
    }

    pub(super) fn fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }

    /// Makes renames and creations in the directory durable.
    pub(super) fn sync(&self) -> io::Result<()> {
        self.0.sync_all()
    }
}

/// Writes `bytes` (the version being replaced) to a new private file next to `path`:
/// `<name>.bak`, or `<name>.bak.<n>` when that name is taken. Names are created exclusively, so
/// an existing backup or a symlink at a backup name is never replaced or written through. Its
/// mode is the replaced file's owner bits plus owner read: never readable by more than the
/// replaced file was.
pub(super) fn write_backup(dir: impl AsFd, path: &Path, bytes: &[u8]) -> io::Result<(PathBuf, fs::File)> {
    let dir = dir.as_fd();
    let source = rustix::fs::statat(dir, path, AtFlags::empty())?.st_mode;
    let mode = (source & 0o600) | 0o400;
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC;
    let name = file_name(path);
    for n in 0u32.. {
        let backup = if n == 0 {
            path.with_file_name(format!("{name}.bak"))
        } else {
            path.with_file_name(format!("{name}.bak.{n}"))
        };
        match rustix::fs::openat(dir, &backup, flags, Mode::from_raw_mode(mode)) {
            Ok(fd) => {
                let mut file = fs::File::from(fd);
                let written = file
                    .set_permissions(fs::Permissions::from_mode(mode))
                    .and_then(|()| file.write_all(bytes))
                    .and_then(|()| file.sync_all());
                if let Err(e) = written {
                    // Best effort: an incomplete backup is worse than none; report the write error.
                    remove_owned(dir, &backup, &file);
                    return Err(e);
                }
                return Ok((backup, file));
            }
            Err(Errno::EXIST) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free backup name"))
}

/// A synced replacement in the locked config directory. Drop only removes this attempt's
/// temporary file, through the same descriptor used to create and publish it.
pub(super) struct Replacement<'a> {
    dir: BorrowedFd<'a>,
    temp: PathBuf,
    file: OwnedFd,
}

impl<'a> Replacement<'a> {
    pub(super) fn stage(
        dir: BorrowedFd<'a>,
        path: &Path,
        bytes: &[u8],
        mode: Option<u32>,
    ) -> io::Result<Self> {
        let (temp, mut file) = create_temp(dir, path, mode)?;
        let pin = (|| {
            write_synced(&mut file, bytes)?;
            #[cfg(test)]
            crate::recovery_regressions::fail_config_temp()?;
            let flags = OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC;
            let pinned = rustix::fs::openat(dir, &temp, flags, Mode::empty())?;
            if !crate::durable::same_inode(&rustix::fs::fstat(&file)?, &rustix::fs::fstat(&pinned)?) {
                return Err(io::Error::other("the staged config file changed"));
            }
            Ok(pinned)
        })();
        match pin {
            // Close the writable descriptor before rename, so the watch observes just the rename.
            Ok(pinned) => Ok(Self { dir, temp, file: pinned }),
            Err(e) => {
                remove_owned(dir, &temp, &file);
                Err(e)
            }
        }
    }

    pub(super) fn check_name(&self) -> io::Result<()> {
        let named = rustix::fs::statat(self.dir, &self.temp, AtFlags::SYMLINK_NOFOLLOW)?;
        let pinned = rustix::fs::fstat(&self.file)?;
        if !crate::durable::same_inode(&named, &pinned) {
            return Err(io::Error::other("the staged config file changed"));
        }
        Ok(())
    }

    pub(super) fn put(self, path: &Path) -> io::Result<()> {
        rustix::fs::renameat(self.dir, &self.temp, self.dir, path).map_err(Into::into)
    }
}

impl Drop for Replacement<'_> {
    fn drop(&mut self) {
        remove_owned(self.dir, &self.temp, &self.file);
    }
}

pub(super) fn remove_owned(dir: BorrowedFd<'_>, path: &Path, file: impl AsFd) {
    let named = rustix::fs::statat(dir, path, AtFlags::SYMLINK_NOFOLLOW);
    if let (Ok(named), Ok(pinned)) = (named, rustix::fs::fstat(file))
        && crate::durable::same_inode(&named, &pinned)
    {
        let _ = rustix::fs::unlinkat(dir, path, AtFlags::empty());
    }
}

/// Publishes a complete private receipt at a fresh path. Existing files (including a
/// config, generation or symlink) must never be replaced by an auxiliary write.
pub(super) fn write_new_atomically(dir: impl AsFd, name: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let dir = dir.as_fd();
    let (temp, mut file) = create_temp(dir, name, Some(mode))?;
    let result = write_synced(&mut file, bytes).and_then(|()| {
        rustix::fs::renameat_with(dir, &temp, dir, name, rustix::fs::RenameFlags::NOREPLACE)
            .map_err(io::Error::from)
    });
    if result.is_err() {
        let _ = rustix::fs::unlinkat(dir, temp, AtFlags::empty());
    }
    result
}

fn write_synced(file: &mut fs::File, bytes: &[u8]) -> io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(Default::default, |n| n.to_string_lossy().into_owned())
}

/// Numbers the temp files of this process, so concurrent saves never share one.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Creates a new temp file `.<file name>.<pid>.<n>.tmp` next to `path`: same filesystem (so
/// `rename` is atomic). It is created exclusively, so no other save (this process or another)
/// can write into it; a name already taken (a stale temp) is skipped.
fn create_temp(dir: impl AsFd, path: &Path, mode: Option<u32>) -> io::Result<(PathBuf, fs::File)> {
    let dir = dir.as_fd();
    let name = file_name(path);
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC;
    let permissions = Mode::from_raw_mode(if mode.is_some() { 0o600 } else { 0o666 });
    loop {
        let n = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let temp = path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()));
        match rustix::fs::openat(dir, &temp, flags, permissions) {
            Ok(fd) => {
                let file = fs::File::from(fd);
                if let Some(mode) = mode
                    && let Err(e) = file.set_permissions(fs::Permissions::from_mode(mode))
                {
                    let _ = rustix::fs::unlinkat(dir, &temp, AtFlags::empty());
                    return Err(e);
                }
                return Ok((temp, file));
            }
            Err(Errno::EXIST) => {}
            Err(e) => return Err(e.into()),
        }
    }
}
