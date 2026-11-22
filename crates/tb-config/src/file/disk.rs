//! The disk side of a compare-and-swap save (ADR-0013): the version on disk, the directory lock,
//! the atomic write and the private backup of the bytes an acknowledged overwrite replaces.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// An exclusive `flock` on the config's directory, released when dropped. Every library writer
/// takes it, so a check of the version and the rename that follows are never interleaved with
/// another library save. The directory itself is locked, so no lock file is ever left behind.
pub(super) struct DirLock(fs::File);

impl DirLock {
    pub(super) fn take(dir: &Path) -> io::Result<Self> {
        let handle = fs::File::open(dir)?;
        handle.lock()?;
        Ok(Self(handle))
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
pub(super) fn write_backup(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    let source = fs::metadata(path)?.permissions().mode();
    let mode = (source & 0o600) | 0o400;
    let name = file_name(path);
    for n in 0u32.. {
        let backup = if n == 0 {
            path.with_file_name(format!("{name}.bak"))
        } else {
            path.with_file_name(format!("{name}.bak.{n}"))
        };
        match fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&backup) {
            Ok(mut file) => {
                let written = file
                    .set_permissions(fs::Permissions::from_mode(mode))
                    .and_then(|()| file.write_all(bytes))
                    .and_then(|()| file.sync_all());
                if let Err(e) = written {
                    // Best effort: an incomplete backup is worse than none; report the write error.
                    let _ = fs::remove_file(&backup);
                    return Err(e);
                }
                return Ok(backup);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free backup name"))
}

/// Writes `text` to a temp file next to `path`, syncs it, runs `before_rename` (the last version
/// check) and renames it over `path`, so a reader sees the old file or the new one, never half of
/// it. On any failure, including a refusal by `before_rename`, the temp file is removed.
pub(super) fn write_atomically<E>(
    path: &Path,
    text: &str,
    io_at: impl Fn(&Path, io::Error) -> E,
    before_rename: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    let dir = super::dir_of(path);
    let (temp, file) = create_temp(path).map_err(|e| io_at(dir, e))?;
    let written = write_synced(file, text)
        .map_err(|e| io_at(&temp, e))
        .and_then(|()| before_rename())
        .and_then(|()| fs::rename(&temp, path).map_err(|e| io_at(path, e)));
    if written.is_err() {
        // Best effort: the write already failed, and that error is the one worth reporting.
        let _ = fs::remove_file(&temp);
    }
    written
}

fn write_synced(mut file: fs::File, text: &str) -> io::Result<()> {
    file.write_all(text.as_bytes())?;
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
fn create_temp(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    let name = file_name(path);
    loop {
        let n = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let temp = path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()));
        match fs::OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
}
