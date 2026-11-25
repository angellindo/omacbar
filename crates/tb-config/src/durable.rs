//! Directory helpers shared by the config file and the generation store: create a hierarchy so
//! it survives a crash, and resolve a path through the symlinks of its existing ancestors.

use std::ffi::OsString;
use std::fs::{self, DirBuilder, File};
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use rustix::fs::{CWD, Mode, OFlags, Stat, fstat, openat};

/// Creates `dir` (mode `leaf_mode`, before the umask) and its missing ancestors (default mode).
/// Each new directory is fsynced into its parent, so a file later made durable inside it is not
/// lost with its directory entry on a crash. Existing directories are left as they are; one
/// created meanwhile by another writer is fine. Returns whether `dir` itself was created.
pub(crate) fn create_dir_all_durably(dir: &Path, leaf_mode: u32) -> io::Result<bool> {
    let mut missing = Vec::new();
    let mut next = Some(dir);
    while let Some(d) = next.filter(|d| !d.as_os_str().is_empty()) {
        match fs::symlink_metadata(d) {
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                missing.push(d);
                next = d.parent();
            }
            Err(e) => return Err(e),
        }
    }
    let mut created = false;
    for d in missing.into_iter().rev() {
        let leaf = d == dir;
        match DirBuilder::new().mode(if leaf { leaf_mode } else { 0o777 }).create(d) {
            Ok(()) => created = leaf,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
        sync_dir(parent_of(d))?;
    }
    Ok(created)
}

/// Makes creations, removals and renames in `dir` durable.
pub(crate) fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// `path` with its longest existing ancestor resolved through symlinks (lexically absolute
/// otherwise), so two paths reaching one directory by different names compare equal.
pub(crate) fn resolved(path: &Path) -> PathBuf {
    let Ok(absolute) = std::path::absolute(path) else { return path.to_path_buf() };
    for ancestor in absolute.ancestors() {
        if let Ok(real) = fs::canonicalize(ancestor) {
            let rest = absolute.strip_prefix(ancestor).unwrap_or(Path::new(""));
            return if rest.as_os_str().is_empty() { real } else { real.join(rest) };
        }
    }
    absolute
}

/// The directory holding `path`; a bare name is in the current directory (`.`).
pub(crate) fn parent_of(path: &Path) -> &Path {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
}

/// The closest directory we can open, and the missing names from leaf to ancestor.
pub(crate) fn directory_prefix(path: &Path) -> io::Result<(OwnedFd, Vec<OsString>)> {
    let mut at = path;
    let mut missing = Vec::new();
    loop {
        match open_directory(CWD, at) {
            Ok(dir) => return Ok((dir, missing)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let name = at.file_name().ok_or(e)?;
                missing.push(name.to_os_string());
                at = parent_of(at);
            }
            Err(e) => return Err(e),
        }
    }
}

pub(crate) fn is_inside(dir: impl AsFd, ancestor: &Stat) -> io::Result<bool> {
    let mut dir = dir.as_fd().try_clone_to_owned()?;
    let mut stat = fstat(&dir)?;
    loop {
        if same_inode(&stat, ancestor) {
            return Ok(true);
        }
        dir = open_directory(&dir, Path::new(".."))?;
        let parent = fstat(&dir)?;
        if same_inode(&stat, &parent) {
            return Ok(false);
        }
        stat = parent;
    }
}

pub(crate) fn same_inode(left: &Stat, right: &Stat) -> bool {
    left.st_dev == right.st_dev && left.st_ino == right.st_ino
}

pub(crate) fn open_directory(dir: impl AsFd, path: &Path) -> io::Result<OwnedFd> {
    let flags = OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC;
    openat(dir, path, flags, Mode::empty()).map_err(Into::into)
}
