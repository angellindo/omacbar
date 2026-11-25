//! Directory helpers shared by the config file and the generation store: create a hierarchy so
//! it survives a crash, and resolve a path through the symlinks of its existing ancestors.

use std::fs::{self, DirBuilder, File};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

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
