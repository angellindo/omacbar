//! The store directory on disk. Once the `backups` directory is checked, every operation is
//! anchored to its file descriptor (`openat`, `mkdirat`, `renameat`), so a directory or symlink
//! swapped in under its path afterwards is never read from or written into.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{AtFlags, CWD, Dir, FileType, Mode, OFlags, RenameFlags};
use rustix::io::Errno;

use super::manifest::Manifest;
use super::{BackupError, GenerationId, GenerationKind, Health};
use crate::durable::parent_of;

/// Name of the kept bytes inside a generation.
pub(super) const DATA: &str = "config.toml";
pub(super) const MANIFEST: &str = "manifest.toml";

const DIR_FLAGS: OFlags =
    OFlags::RDONLY.union(OFlags::DIRECTORY).union(OFlags::NOFOLLOW).union(OFlags::CLOEXEC);

/// One generation as read back from the store.
pub(super) enum Inspected {
    /// No generation directory by that name.
    Missing,
    /// The manifest cannot be read or contradicts itself, so it names no config.
    Unreadable(String),
    Read {
        manifest: Manifest,
        kind: GenerationKind,
        health: Health,
        data: Option<Vec<u8>>,
    },
}

/// The opened `backups` directory.
pub(super) struct StoreDir {
    pub path: PathBuf,
    fd: OwnedFd,
}

impl StoreDir {
    /// Opens an existing store for reading; `None` when there is none yet.
    pub fn open(path: &Path) -> Result<Option<Self>, BackupError> {
        match rustix::fs::openat(CWD, path, DIR_FLAGS, Mode::empty()) {
            Ok(fd) => Ok(Some(Self { path: path.to_path_buf(), fd })),
            Err(Errno::NOENT) => Ok(None),
            Err(e) => Err(refusal(path, e)),
        }
    }

    /// Creates the store if needed (0700, every new directory durable) and opens it for
    /// writing, refusing a store this user cannot trust: a symlink, not a directory, owned by
    /// another user, open to other users, or under a parent owned by another user.
    pub fn open_for_write(path: &Path) -> Result<Self, BackupError> {
        let uid = rustix::process::geteuid().as_raw();
        let parent = open_owned_parent(path, uid)?;
        let fd = open_or_create_directory(&parent, path, DIR_FLAGS, 0o700, path, uid)?;
        let stat = rustix::fs::fstat(&fd).map_err(|e| store_io(path, e))?;
        if stat.st_mode & 0o077 != 0 {
            return Err(BackupError::Unsafe {
                path: path.to_path_buf(),
                why: format!("its mode {:o} lets other users in", stat.st_mode & 0o777),
                fix: format!("run `chmod 700 {}`", path.display()),
            });
        }
        Ok(Self { path: path.to_path_buf(), fd })
    }

    /// An exclusive `flock` on the store, released when the returned handle drops: allocating
    /// an id and renaming a generation into place never interleave with another writer.
    pub fn lock(&self) -> io::Result<File> {
        let handle = File::from(self.fd.try_clone()?);
        handle.lock()?;
        Ok(handle)
    }

    /// The ids in the store, ascending; temp directories (dot names) and other names are not
    /// generations.
    pub fn ids(&self) -> Result<Vec<GenerationId>, BackupError> {
        let dir = Dir::read_from(&self.fd).map_err(|e| self.io(e.into()))?;
        let mut ids = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|e| self.io(e.into()))?;
            if let Some(id) = entry.file_name().to_str().ok().and_then(GenerationId::parse) {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    /// Names of temp directories an interrupted write left (`.tmp-*`), sorted. They are not
    /// generations and are never removed by this library.
    pub fn temps(&self) -> Result<Vec<String>, BackupError> {
        let dir = Dir::read_from(&self.fd).map_err(|e| self.io(e.into()))?;
        let mut temps = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|e| self.io(e.into()))?;
            if let Ok(name) = entry.file_name().to_str()
                && name.starts_with(".tmp-")
            {
                temps.push(name.to_owned());
            }
        }
        temps.sort_unstable();
        Ok(temps)
    }

    /// Reads one generation back and checks it: manifest shape, its id, the kept bytes (a
    /// regular file, not a symlink, with the recorded length and hash) or their absence.
    pub fn inspect(&self, id: GenerationId) -> Inspected {
        let dir = match rustix::fs::openat(&self.fd, id.to_string(), DIR_FLAGS, Mode::empty()) {
            Ok(dir) => dir,
            Err(Errno::NOENT) => return Inspected::Missing,
            Err(e) => return Inspected::Unreadable(format!("cannot open it: {}", io::Error::from(e))),
        };
        let manifest = match read_regular_at(dir.as_fd(), MANIFEST) {
            Ok(bytes) => String::from_utf8(bytes)
                .map_err(|e| e.to_string())
                .and_then(|text| toml::from_str::<Manifest>(&text).map_err(|e| e.to_string())),
            Err(why) => Err(why),
        };
        let manifest = match manifest {
            Ok(manifest) => manifest,
            Err(why) => return Inspected::Unreadable(format!("{MANIFEST}: {why}")),
        };
        let kind = match manifest.kind() {
            Ok(kind) => kind,
            Err(why) => return Inspected::Unreadable(format!("{MANIFEST}: {why}")),
        };
        let (health, data) = match check_data(dir.as_fd(), id, &manifest, kind) {
            Ok(data) => (Health::Verified, data),
            Err(why) => (Health::Damaged(why), None),
        };
        Inspected::Read { manifest, kind, health, data }
    }

    /// Builds the generation `id` in a private temp directory, makes it durable, then renames
    /// it into place (never over an existing name) and makes the rename durable. A listing
    /// sees the whole generation or nothing. On a failure before the rename, this attempt's own
    /// temp directory is removed; nothing else in the store is ever removed.
    pub fn build(&self, id: GenerationId, manifest: &str, data: Option<&[u8]>) -> io::Result<()> {
        let temp = self.make_temp()?;
        let staged = (|| {
            let dir = rustix::fs::openat(&self.fd, temp.as_str(), DIR_FLAGS, Mode::empty())?;
            rustix::fs::fchmod(&dir, Mode::from_raw_mode(0o700))?;
            if let Some(bytes) = data {
                write_new_at(dir.as_fd(), DATA, bytes)?;
            }
            write_new_at(dir.as_fd(), MANIFEST, manifest.as_bytes())?;
            rustix::fs::fsync(&dir)?;
            rustix::fs::renameat_with(
                &self.fd,
                temp.as_str(),
                &self.fd,
                id.to_string(),
                RenameFlags::NOREPLACE,
            )?;
            Ok::<(), io::Error>(())
        })();
        if let Err(e) = staged {
            self.remove_temp(&temp);
            return Err(e);
        }
        rustix::fs::fsync(&self.fd).map_err(io::Error::from)
    }

    fn make_temp(&self) -> io::Result<String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let name = format!(".tmp-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
            match rustix::fs::mkdirat(&self.fd, name.as_str(), Mode::from_raw_mode(0o700)) {
                Ok(()) => return Ok(name),
                Err(Errno::EXIST) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Best effort: the build already failed and that error is the one reported.
    fn remove_temp(&self, temp: &str) {
        for name in [DATA, MANIFEST] {
            let _ = rustix::fs::unlinkat(&self.fd, format!("{temp}/{name}"), AtFlags::empty());
        }
        let _ = rustix::fs::unlinkat(&self.fd, temp, AtFlags::REMOVEDIR);
    }

    fn io(&self, source: io::Error) -> BackupError {
        BackupError::Io { path: self.path.clone(), source }
    }
}

/// Check the nearest existing directory before creating anything, then use that same
/// descriptor to create and open each missing component. Existing state paths may resolve
/// through symlinks, but a name appearing during creation must be an owned directory.
fn open_owned_parent(store: &Path, uid: u32) -> Result<OwnedFd, BackupError> {
    let flags = OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC;
    let mut ancestor = parent_of(store);
    let mut missing = Vec::new();
    let mut fd = loop {
        match rustix::fs::openat(CWD, ancestor, flags, Mode::empty()) {
            Ok(fd) => break fd,
            Err(Errno::NOENT) => {
                missing.push(ancestor);
                ancestor = parent_of(ancestor);
            }
            Err(e) => return Err(store_io(store, e)),
        }
    };
    check_owner(store, ancestor, rustix::fs::fstat(&fd).map_err(|e| store_io(store, e))?.st_uid, uid)?;
    for dir in missing.into_iter().rev() {
        fd = open_or_create_directory(&fd, dir, flags | OFlags::NOFOLLOW, 0o777, store, uid)?;
    }
    Ok(fd)
}

fn open_or_create_directory(
    parent: &OwnedFd,
    dir: &Path,
    flags: OFlags,
    mode: u32,
    store: &Path,
    uid: u32,
) -> Result<OwnedFd, BackupError> {
    let name = dir.file_name().ok_or_else(|| store_io(store, Errno::INVAL))?;
    let open = || rustix::fs::openat(parent, name, flags, Mode::empty());
    let (fd, needs_sync) = match open() {
        Ok(fd) => (fd, false),
        Err(Errno::NOENT) => {
            match rustix::fs::mkdirat(parent, name, Mode::from_raw_mode(mode)) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(e) => return Err(store_io(store, e)),
            }
            (open().map_err(|e| refusal(dir, e))?, true)
        }
        Err(e) => return Err(refusal(dir, e)),
    };
    check_owner(store, dir, rustix::fs::fstat(&fd).map_err(|e| store_io(store, e))?.st_uid, uid)?;
    if needs_sync {
        let sync =
            rustix::fs::openat(parent, ".", DIR_FLAGS, Mode::empty()).map_err(|e| store_io(store, e))?;
        rustix::fs::fsync(&sync).map_err(|e| store_io(store, e))?;
    }
    Ok(fd)
}

fn check_owner(store: &Path, dir: &Path, owner: u32, uid: u32) -> Result<(), BackupError> {
    if owner == uid {
        return Ok(());
    }
    Err(BackupError::Unsafe {
        path: store.to_path_buf(),
        why: format!("directory `{}` belongs to uid {owner}, not to this user (uid {uid})", dir.display()),
        fix: "run as the user who owns the directory, or point XDG_STATE_HOME at a directory of your own"
            .to_owned(),
    })
}

fn store_io(store: &Path, error: Errno) -> BackupError {
    BackupError::Io { path: store.to_path_buf(), source: error.into() }
}

/// Why the store path cannot be opened as a directory.
fn refusal(path: &Path, e: Errno) -> BackupError {
    let unsafe_store =
        |why: &str, fix: String| BackupError::Unsafe { path: path.to_path_buf(), why: why.to_owned(), fix };
    match e {
        Errno::LOOP => unsafe_store(
            "it is a symbolic link",
            format!("replace `{}` with a real directory (mode 700)", path.display()),
        ),
        Errno::NOTDIR => unsafe_store(
            "it is not a directory",
            format!("move `{}` away so a private directory can be created", path.display()),
        ),
        e => BackupError::Io { path: path.to_path_buf(), source: e.into() },
    }
}

/// The kept bytes of a `file` generation, checked against its manifest; `None` for an absence
/// (which must hold no data).
fn check_data(
    dir: BorrowedFd<'_>,
    id: GenerationId,
    manifest: &Manifest,
    kind: GenerationKind,
) -> Result<Option<Vec<u8>>, String> {
    if manifest.id != id.0 {
        return Err(format!("its manifest names generation {}", manifest.id));
    }
    match kind {
        GenerationKind::Absent { .. } => match rustix::fs::statat(dir, DATA, AtFlags::SYMLINK_NOFOLLOW) {
            Err(Errno::NOENT) => Ok(None),
            _ => Err(format!("an absence holds a {DATA}")),
        },
        GenerationKind::File => {
            let bytes = read_regular_at(dir, DATA).map_err(|why| format!("{DATA}: {why}"))?;
            if bytes.len() as u64 != manifest.len {
                return Err(format!("{DATA} has {} bytes, the manifest says {}", bytes.len(), manifest.len));
            }
            if blake3::hash(&bytes).to_hex().as_str() != manifest.blake3 {
                return Err(format!("{DATA} does not match the manifest's hash"));
            }
            Ok(Some(bytes))
        }
    }
}

/// Reads a regular file inside `dir` without following a symlink or blocking on a FIFO.
fn read_regular_at(dir: BorrowedFd<'_>, name: &str) -> Result<Vec<u8>, String> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC | OFlags::NOCTTY;
    let fd = match rustix::fs::openat(dir, name, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::LOOP) => return Err("is a symbolic link".to_owned()),
        Err(Errno::NOENT) => return Err("is missing".to_owned()),
        Err(e) => return Err(io::Error::from(e).to_string()),
    };
    let stat = rustix::fs::fstat(&fd).map_err(|e| io::Error::from(e).to_string())?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
        return Err("is not a regular file".to_owned());
    }
    let mut bytes = Vec::new();
    File::from(fd).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Creates `name` in `dir` exclusively (0600, whatever the umask), writes and syncs it.
fn write_new_at(dir: BorrowedFd<'_>, name: &str, bytes: &[u8]) -> io::Result<()> {
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let fd = rustix::fs::openat(dir, name, flags, Mode::from_raw_mode(0o600))?;
    rustix::fs::fchmod(&fd, Mode::from_raw_mode(0o600))?;
    let mut file = File::from(fd);
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::panic)]
    fn the_store_requires_the_effective_users_uid_in_directory_metadata() {
        let store = Path::new("/state/new/backups");
        let ancestor = Path::new("/state");
        let session_uid = 1000;
        assert!(check_owner(store, ancestor, session_uid, session_uid).is_ok());
        let Err(BackupError::Unsafe { path, why, fix }) = check_owner(store, ancestor, 1001, session_uid)
        else {
            panic!("a directory owned by another uid must be refused");
        };
        assert_eq!(path, store);
        assert!(why.contains("/state") && why.contains("uid 1001") && why.contains("uid 1000"));
        assert!(fix.contains("XDG_STATE_HOME"));
    }
}
