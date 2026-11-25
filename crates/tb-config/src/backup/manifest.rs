//! `manifest.toml` of one generation (ADR-0015): what was kept, for which config, and how to
//! check the kept bytes. Its shape is a contract with every earlier generation on disk, so
//! unknown fields are refused and `format` names the version of this shape.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{GenerationKind, Reason};

/// The only manifest shape this build reads and writes.
pub(super) const FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct Manifest {
    pub format: u32,
    /// The generation's directory name in the store; a generation moved or copied under
    /// another name does not verify.
    pub id: u64,
    /// The absolute path of the config file this generation belongs to.
    pub config: PathBuf,
    pub reason: Reason,
    pub kind: Kind,
    /// Permission bits of the kept file (0 for an absence).
    pub mode: u32,
    /// Length of the kept bytes (0 for an absence).
    pub len: u64,
    /// BLAKE3 of the kept bytes, lowercase hex.
    pub blake3: String,
    /// For an absence: whether the config directory existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir_existed: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Kind {
    File,
    Absent,
}

impl Manifest {
    pub(super) fn has_empty_hash(&self) -> bool {
        blake3::hash(&[]).to_hex().as_str() == self.blake3
    }

    /// The kind it records, or why the fields contradict each other.
    pub(super) fn kind(&self) -> Result<GenerationKind, String> {
        if self.format != FORMAT {
            return Err(format!("unknown manifest format {}", self.format));
        }
        if self.mode & !0o777 != 0 {
            return Err(format!("mode {:o} is not a set of permission bits", self.mode));
        }
        match (self.kind, self.dir_existed) {
            (Kind::File, None) => Ok(GenerationKind::File),
            (Kind::Absent, Some(dir_existed)) if self.len == 0 && self.mode == 0 => {
                Ok(GenerationKind::Absent { dir_existed })
            }
            (Kind::File, Some(_)) => Err("a kept file has no `dir-existed`".to_owned()),
            (Kind::Absent, _) => Err("an absence has `dir-existed`, no length and no mode".to_owned()),
        }
    }
}
