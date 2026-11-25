//! The store directory on disk. Once the `backups` directory is checked, every operation is
//! anchored to its file descriptor (`openat`, `mkdirat`, `renameat`), so a directory or symlink
//! swapped in under its path afterwards is never read from or written into.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{AtFlags, CWD, Dir, FileType, Mode, OFlags, RenameFlags, Stat};
use rustix::io::Errno;

use super::manifest::Manifest;
use super::{BackupError, GenerationId, GenerationKind, Health};
use crate::durable::{parent_of, same_inode};

#[cfg(test)]
use crate::recovery_regressions::sync_backup_dir;
#[cfg(not(test))]
use rustix::fs::fsync as sync_backup_dir;

/// Name of the kept bytes inside a generation.
pub(super) const DATA: &str = "config.toml";
pub(super) const MANIFEST: &str = "manifest.toml";

const DIR_FLAGS: OFlags =
    OFlags::RDONLY.union(OFlags::DIRECTORY).union(OFlags::NOFOLLOW).union(OFlags::CLOEXEC);

const PATH_DIR_FLAGS: OFlags = OFlags::PATH.union(OFlags::DIRECTORY).union(OFlags::CLOEXEC);

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
        let parent = parent_of(path);
        let fd = match rustix::fs::openat(CWD, parent, PATH_DIR_FLAGS, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::NOENT) => return Ok(None),
            Err(e) => return Err(store_io(path, e)),
        };
        let name = path.file_name().ok_or_else(|| store_io(path, Errno::INVAL))?;
        match rustix::fs::openat(&fd, name, DIR_FLAGS, Mode::empty()) {
            Ok(store) => {
                let uid = rustix::process::geteuid().as_raw();
                check_owner(
                    path,
                    parent,
                    rustix::fs::fstat(&fd).map_err(|e| store_io(path, e))?.st_uid,
                    uid,
                )?;
                Self::checked(path, store, uid).map(Some)
            }
            Err(Errno::NOENT) => Ok(None),
            Err(e) => Err(refusal(path, e)),
        }
    }

    /// Creates the private store if needed, refusing untrusted directories as on reads.
    pub fn open_for_write(
        path: &Path,
        mut check_parent: impl FnMut(&OwnedFd, &[OsString]) -> Result<(), BackupError>,
    ) -> Result<Self, BackupError> {
        let uid = rustix::process::geteuid().as_raw();
        let parent = open_owned_parent(path, uid, &mut check_parent)?;
        let name = path.file_name().ok_or_else(|| store_io(path, Errno::INVAL))?;
        let fd = open_or_create_directory(&parent, path, DIR_FLAGS, 0o700, path, uid, || {
            check_parent(&parent, &[name.to_os_string()])
        })?;
        Self::checked(path, fd, uid)
    }

    pub fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    fn checked(path: &Path, fd: OwnedFd, uid: u32) -> Result<Self, BackupError> {
        let stat = rustix::fs::fstat(&fd).map_err(|e| store_io(path, e))?;
        check_owner(path, path, stat.st_uid, uid)?;
        if let Err(why) = private_mode(&stat) {
            return Err(BackupError::Unsafe {
                path: path.to_path_buf(),
                why,
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
        let stat = match rustix::fs::fstat(&dir) {
            Ok(stat) => stat,
            Err(e) => {
                return Inspected::Unreadable(format!(
                    "cannot inspect its directory: {}",
                    io::Error::from(e)
                ));
            }
        };
        if let Err(why) = private_member(&stat) {
            return Inspected::Unreadable(format!("generation directory: {why}"));
        }
        let (manifest_file, stat) = match open_regular_at(dir.as_fd(), MANIFEST) {
            Ok(entry) => entry,
            Err(why) => return Inspected::Unreadable(format!("{MANIFEST}: {why}")),
        };
        if let Err(why) = private_file(&stat) {
            return Inspected::Unreadable(format!("{MANIFEST}: {why}"));
        }
        let manifest = read_bytes(manifest_file)
            .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
            .and_then(|text| toml::from_str::<Manifest>(&text).map_err(|e| e.to_string()));
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
    pub fn build<E: From<BackupError>>(
        &self,
        id: GenerationId,
        manifest: &str,
        data: Option<&[u8]>,
        before_publish: impl FnOnce() -> Result<(), E>,
    ) -> Result<(), E> {
        let temp = self.make_temp().map_err(|e| self.io(e))?;
        let dir = rustix::fs::openat(&self.fd, temp.as_str(), DIR_FLAGS, Mode::empty())
            .map_err(|e| self.io(e.into()))?;
        let staged = Self::stage(&dir, manifest, data)
            .map_err(|e| E::from(self.io(e)))
            .and_then(|()| before_publish())
            .and_then(|()| {
                let named = rustix::fs::statat(&self.fd, temp.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(|e| E::from(self.io(e.into())))?;
                let pinned = rustix::fs::fstat(&dir).map_err(|e| E::from(self.io(e.into())))?;
                if !same_inode(&named, &pinned) {
                    return Err(self.io(io::Error::other("the staged generation directory changed")).into());
                }
                rustix::fs::renameat_with(
                    &self.fd,
                    temp.as_str(),
                    &self.fd,
                    id.to_string(),
                    RenameFlags::NOREPLACE,
                )
                .map_err(|e| E::from(self.io(e.into())))
            });
        if let Err(e) = staged {
            self.remove_temp(&temp, &dir);
            return Err(e);
        }
        self.sync().map_err(|e| self.io(e))?;
        Ok(())
    }

    fn stage(dir: &OwnedFd, manifest: &str, data: Option<&[u8]>) -> io::Result<()> {
        rustix::fs::fchmod(dir, Mode::from_raw_mode(0o700))?;
        if let Some(bytes) = data {
            write_new_at(dir.as_fd(), DATA, bytes)?;
        }
        write_new_at(dir.as_fd(), MANIFEST, manifest.as_bytes())?;
        sync_backup_dir(dir)?;
        Ok(())
    }

    /// Sync every directory that could have been created by an interrupted keep, even on
    /// retries that see it already present. Only owned ancestors can contain entries we made.
    pub fn sync(&self) -> io::Result<()> {
        let uid = rustix::process::geteuid().as_raw();
        let mut dir = self.fd.try_clone()?;
        loop {
            sync_backup_dir(&dir)?;
            let stat = rustix::fs::fstat(&dir)?;
            let parent = rustix::fs::openat(&dir, "..", PATH_DIR_FLAGS, Mode::empty())?;
            let above = rustix::fs::fstat(&parent)?;
            if above.st_uid != uid || above.st_dev != stat.st_dev || above.st_ino == stat.st_ino {
                return Ok(());
            }
            dir = rustix::fs::openat(&parent, ".", DIR_FLAGS, Mode::empty())?;
        }
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
    fn remove_temp(&self, temp: &str, dir: &OwnedFd) {
        let named = rustix::fs::statat(&self.fd, temp, AtFlags::SYMLINK_NOFOLLOW);
        if let (Ok(named), Ok(pinned)) = (named, rustix::fs::fstat(dir))
            && same_inode(&named, &pinned)
        {
            for name in [DATA, MANIFEST] {
                let _ = rustix::fs::unlinkat(dir, name, AtFlags::empty());
            }
            let _ = rustix::fs::unlinkat(&self.fd, temp, AtFlags::REMOVEDIR);
        }
    }

    fn io(&self, source: io::Error) -> BackupError {
        BackupError::Io { path: self.path.clone(), source }
    }
}

/// Check the nearest existing directory before creating anything, then use that same
/// descriptor to create and open each missing component. Existing state paths may resolve
/// through symlinks, but a name appearing during creation must be an owned directory.
fn open_owned_parent(
    store: &Path,
    uid: u32,
    mut check_parent: impl FnMut(&OwnedFd, &[OsString]) -> Result<(), BackupError>,
) -> Result<OwnedFd, BackupError> {
    let mut ancestor = parent_of(store);
    let mut missing = Vec::new();
    let mut fd = loop {
        match rustix::fs::openat(CWD, ancestor, PATH_DIR_FLAGS, Mode::empty()) {
            Ok(fd) => break fd,
            Err(Errno::NOENT) => {
                missing.push(ancestor);
                ancestor = parent_of(ancestor);
            }
            Err(e) => return Err(store_io(store, e)),
        }
    };
    check_owner(store, ancestor, rustix::fs::fstat(&fd).map_err(|e| store_io(store, e))?.st_uid, uid)?;
    let mut remaining = std::iter::once(store)
        .chain(missing.iter().copied())
        .map(|path| path.file_name().map(OsString::from).ok_or_else(|| store_io(path, Errno::INVAL)))
        .collect::<Result<Vec<_>, _>>()?;
    for dir in missing.into_iter().rev() {
        fd =
            open_or_create_directory(&fd, dir, PATH_DIR_FLAGS | OFlags::NOFOLLOW, 0o777, store, uid, || {
                check_parent(&fd, &remaining)
            })?;
        remaining.pop();
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
    before_create: impl FnOnce() -> Result<(), BackupError>,
) -> Result<OwnedFd, BackupError> {
    let name = dir.file_name().ok_or_else(|| store_io(store, Errno::INVAL))?;
    let open = || rustix::fs::openat(parent, name, flags, Mode::empty());
    let fd = match open() {
        Ok(fd) => fd,
        Err(Errno::NOENT) => {
            before_create()?;
            match rustix::fs::mkdirat(parent, name, Mode::from_raw_mode(mode)) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(e) => return Err(store_io(store, e)),
            }
            open().map_err(|e| refusal(dir, e))?
        }
        Err(e) => return Err(refusal(dir, e)),
    };
    check_owner(store, dir, rustix::fs::fstat(&fd).map_err(|e| store_io(store, e))?.st_uid, uid)?;
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
        GenerationKind::Absent { .. } if !manifest.has_empty_hash() => {
            Err("the hash of an absence must be the BLAKE3 hash of empty bytes".to_owned())
        }
        GenerationKind::Absent { .. } => match rustix::fs::statat(dir, DATA, AtFlags::SYMLINK_NOFOLLOW) {
            Err(Errno::NOENT) => Ok(None),
            _ => Err(format!("an absence holds a {DATA}")),
        },
        GenerationKind::File => {
            let bytes = (|| {
                let (file, stat) = open_regular_at(dir, DATA)?;
                private_file(&stat)?;
                read_bytes(file)
            })()
            .map_err(|why| format!("{DATA}: {why}"))?;
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
fn open_regular_at(dir: BorrowedFd<'_>, name: &str) -> Result<(File, Stat), String> {
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
    Ok((File::from(fd), stat))
}

fn read_bytes(mut file: File) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn private_mode(stat: &Stat) -> Result<(), String> {
    if stat.st_mode & 0o077 != 0 {
        Err(format!("its mode {:o} lets other users in", stat.st_mode & 0o777))
    } else {
        Ok(())
    }
}

fn private_member(stat: &Stat) -> Result<(), String> {
    let uid = rustix::process::geteuid().as_raw();
    if stat.st_uid != uid {
        return Err(format!("belongs to uid {}, expected uid {uid}", stat.st_uid));
    }
    private_mode(stat)
}

fn private_file(stat: &Stat) -> Result<(), String> {
    private_member(stat)?;
    if stat.st_nlink != 1 {
        return Err(format!("has {} names (hard links), expected one", stat.st_nlink));
    }
    Ok(())
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
