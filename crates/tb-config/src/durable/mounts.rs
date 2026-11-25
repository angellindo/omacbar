//! Resolve a descriptor into both its namespace path and its path relative to the
//! filesystem root. Walking `..` from a bind mount reaches the mountpoint's parent,
//! which need not be an ancestor of the directory the mount exposes.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use rustix::fs::{CWD, fstat};

use super::{open_directory, same_inode};

pub(crate) struct Mounts(Vec<Mount>);

struct Mount {
    id: u64,
    device: (u64, u64),
    root: PathBuf,
    point: PathBuf,
}

pub(crate) struct Tree<'a> {
    path: PathBuf,
    root: Coordinate,
    mounts: &'a [Mount],
}

struct Coordinate {
    device: (u64, u64),
    path: PathBuf,
}

impl Mounts {
    pub(crate) fn read() -> io::Result<Self> {
        #[cfg(test)]
        if let Some(result) = crate::recovery_regressions::write_identity::MOUNTS
            .with_borrow(|fixture| fixture.as_ref().map(|fixture| Self::parse(&fixture.mountinfo)))
        {
            return result;
        }
        Self::parse(&fs::read("/proc/self/mountinfo")?)
    }

    fn parse(bytes: &[u8]) -> io::Result<Self> {
        let rows = bytes
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty())
            .map(Mount::parse)
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self(rows))
    }

    /// The actual directory and (if absent) the names still to create beneath it.
    pub(crate) fn tree(&self, dir: impl AsFd, missing: &[OsString]) -> io::Result<Tree<'_>> {
        let dir = dir.as_fd();
        let actual = fs::read_link(format!("/proc/self/fd/{}", dir.as_raw_fd()))?;
        let id = descriptor_mount_id(dir, &actual)?;
        let mount = self.0.iter().find(|mount| mount.id == id).ok_or_else(unknown)?;
        let rest = actual.strip_prefix(&mount.point).map_err(|_| unknown())?;
        // A detached, replaced or moved directory may still have an open descriptor.
        // Require its name to resolve to the same inode *and* mount in this namespace.
        let named = open_directory(CWD, &actual)?;
        if !same_inode(&fstat(dir)?, &fstat(&named)?) || descriptor_mount_id(named.as_fd(), &actual)? != id {
            return Err(unknown());
        }
        let suffix = missing.iter().rev().collect::<PathBuf>();
        let path = actual.join(&suffix);
        let physical = mount.root.join(rest).join(suffix);
        Ok(Tree { path, root: Coordinate { device: mount.device, path: physical }, mounts: &self.0 })
    }
}

impl Tree<'_> {
    /// Does this subtree reach the other directory, through a mount in either path?
    /// A nested mount brings its filesystem subtree into this one as well.
    pub(crate) fn contains(&self, other: &Self) -> bool {
        other.path.starts_with(&self.path) || self.coordinates().any(|base| base.contains(&other.root))
    }

    pub(crate) fn intersects(&self, other: &Self) -> bool {
        self.coordinates()
            .any(|left| other.coordinates().any(|right| left.contains(&right) || right.contains(&left)))
    }

    fn coordinates(&self) -> impl Iterator<Item = Coordinate> {
        let nested = self.mounts.iter().filter(|mount| mount.point.starts_with(&self.path));
        std::iter::once(Coordinate { device: self.root.device, path: self.root.path.clone() })
            .chain(nested.map(|mount| Coordinate { device: mount.device, path: mount.root.clone() }))
    }
}

impl Coordinate {
    fn contains(&self, other: &Self) -> bool {
        self.device == other.device && other.path.starts_with(&self.path)
    }
}

impl Mount {
    fn parse(line: &[u8]) -> io::Result<Self> {
        let mut fields = line.split(|&b| b == b' ');
        let id = number(fields.next().ok_or_else(unknown)?)?;
        let _parent = number(fields.next().ok_or_else(unknown)?)?;
        let device = fields.next().ok_or_else(unknown)?;
        let colon = device.iter().position(|&b| b == b':').ok_or_else(unknown)?;
        let (major, minor) = device.split_at(colon);
        let device = (number(major)?, number(&minor[1..])?);
        let root = mount_path(fields.next().ok_or_else(unknown)?)?;
        let point = mount_path(fields.next().ok_or_else(unknown)?)?;
        Ok(Self { id, device, root, point })
    }
}

/// Two handles must name the same directory as well as the same mount view. Even bind
/// mounts of one inode can have different children mounted beneath them.
pub(crate) fn same_directory(left: impl AsFd, right: impl AsFd) -> io::Result<bool> {
    let (left, right) = (left.as_fd(), right.as_fd());
    Ok(same_inode(&fstat(left)?, &fstat(right)?) && mount_id(left)? == mount_id(right)?)
}

fn descriptor_mount_id(dir: BorrowedFd<'_>, path: &Path) -> io::Result<u64> {
    #[cfg(not(test))]
    let _ = path;
    #[cfg(test)]
    if let Some(id) = crate::recovery_regressions::write_identity::MOUNTS
        .with_borrow(|fixture| fixture.as_ref().and_then(|fixture| fixture.mount_id(path)))
    {
        return Ok(id);
    }
    mount_id(dir)
}

fn mount_id(dir: BorrowedFd<'_>) -> io::Result<u64> {
    let info = fs::read(format!("/proc/self/fdinfo/{}", dir.as_raw_fd()))?;
    let id = info.split(|&b| b == b'\n').find_map(|line| line.strip_prefix(b"mnt_id:\t"));
    number(id.ok_or_else(unknown)?)
}

fn number(bytes: &[u8]) -> io::Result<u64> {
    std::str::from_utf8(bytes).ok().and_then(|text| text.parse().ok()).ok_or_else(unknown)
}

fn mount_path(mut bytes: &[u8]) -> io::Result<PathBuf> {
    let mut path = Vec::new();
    while let Some((&byte, rest)) = bytes.split_first() {
        bytes = rest;
        if byte == b'\\' {
            let escape = bytes.get(..3).ok_or_else(unknown)?;
            let unescaped = match escape {
                b"040" => b' ',
                b"011" => b'\t',
                b"012" => b'\n',
                b"134" => b'\\',
                _ => return Err(unknown()),
            };
            path.push(unescaped);
            bytes = &bytes[3..];
        } else {
            path.push(byte);
        }
    }
    let path = PathBuf::from(OsString::from_vec(path));
    if !path.is_absolute() || path.components().any(|part| part.as_os_str() == OsStr::new("..")) {
        return Err(unknown());
    }
    Ok(path)
}

fn unknown() -> io::Error {
    io::Error::other("cannot establish the directory's mount identity")
}
