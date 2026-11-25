//! T1 digitizer discovery, verified calibration, and MT-B reports as bar touches.

mod device;
mod touches;

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use tb_core::BarSize;
use thiserror::Error;

use crate::evdev::{has_bit, is_event_name};
use crate::target::{NodeId, TargetError, parse_dev, read_trimmed};

pub use device::DigitizerNode;
pub use touches::EvdevTouches;

// The T1 iBridge identity from research 0001. The axis calibration is still unknown (TB-45).
const T1_ID: (u16, u16, u16) = (0x0003, 0x05ac, 0x8600);
const DEV_INPUT: &str = "/dev/input/";
const EV_ABS: u16 = 3;
const MT_SLOT: u16 = 0x2f;
const MT_X: u16 = 0x35;
const MT_Y: u16 = 0x36;
const MT_TRACKING_ID: u16 = 0x39;

/// Finds the sole candidate in `/proc/bus/input/devices` text by identity and MT-B capabilities.
pub fn find_digitizer(devices: &str) -> Result<PathBuf, TargetError> {
    let mut candidates = candidates(devices);
    match candidates.len() {
        0 => Err(TargetError::NotFound),
        1 => Ok(candidates.remove(0)),
        _ => Err(TargetError::Ambiguous(candidates)),
    }
}

/// Checks an explicit path against the same candidates used for automatic discovery.
pub fn check_digitizer(devices: &str, node: &Path) -> Result<PathBuf, TargetError> {
    event_name(node)?;
    candidates(devices)
        .into_iter()
        .find(|p| p == node)
        .ok_or_else(|| TargetError::NotADigitizer { path: node.to_owned() })
}

fn event_name(node: &Path) -> Result<&str, TargetError> {
    node.to_str()
        .and_then(|p| p.strip_prefix(DEV_INPUT))
        .filter(|name| is_event_name(name))
        .ok_or_else(|| TargetError::NotAnEventPath { path: node.to_owned() })
}

fn candidates(devices: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut block = Vec::new();
    for line in devices.lines().chain([""]) {
        let blank = line.trim().is_empty();
        if blank || line.starts_with("I: ") {
            if let Some(name) = candidate(&block) {
                paths.push(Path::new(DEV_INPUT).join(name));
            }
            block.clear();
        }
        if !blank {
            block.push(line);
        }
    }
    paths
}

fn candidate<'a>(block: &[&'a str]) -> Option<&'a str> {
    let field = |prefix: &str| block.iter().find_map(|line| line.strip_prefix(prefix));
    let ids = field("I: ")?.split_whitespace();
    let id = |prefix: &str| {
        let word = ids.clone().find_map(|word| word.strip_prefix(prefix))?;
        u16::from_str_radix(word, 16).ok()
    };
    if (id("Bus=")?, id("Vendor=")?, id("Product=")?) != T1_ID
        || !has_bit(field("B: EV=")?, usize::from(EV_ABS))
    {
        return None;
    }
    let axes = field("B: ABS=")?;
    if ![MT_SLOT, MT_TRACKING_ID, MT_X, MT_Y].into_iter().all(|code| has_bit(axes, usize::from(code))) {
        return None;
    }
    field("H: Handlers=")?.split_whitespace().find(|name| is_event_name(name))
}

/// Binds the opened character device to the current sysfs event node and its T1 input identity.
pub fn confirm_digitizer_node(sysfs: &Path, node: &Path, id: NodeId) -> Result<PathBuf, TargetError> {
    let event = sysfs.join("class/input").join(event_name(node)?);
    // Resolve the class link once so both queries address the same event directory.
    let event = std::fs::canonicalize(&event).map_err(|source| TargetError::Io { path: event, source })?;
    let expected = read_trimmed(&event.join("dev"))?;
    if !id.char_device || parse_dev(&expected) != Some((id.major, id.minor)) {
        return Err(TargetError::DigitizerChanged { path: node.to_owned(), expected, found: id });
    }
    let ids = event.join("device/id");
    let read_id = |name| {
        let path = ids.join(name);
        let text = read_trimmed(&path)?;
        u16::from_str_radix(&text, 16)
            .map_err(|e| TargetError::Io { path, source: io::Error::new(io::ErrorKind::InvalidData, e) })
    };
    if (read_id("bustype")?, read_id("vendor")?, read_id("product")?) != T1_ID {
        return Err(TargetError::NotADigitizer { path: node.to_owned() });
    }
    Ok(node.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AbsAxis {
    pub code: u16,
    pub min: i32,
    pub max: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DigitizerMeta {
    pub bus: u16,
    pub vendor: u16,
    pub product: u16,
    pub name: String,
    pub axes: Vec<AbsAxis>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DigitizerProfile {
    pub bus: u16,
    pub vendor: u16,
    pub product: u16,
    pub name: &'static str,
    pub axis: AbsAxis,
    pub reversed: bool,
}

/// TB-45 adds a profile once the physical T1 identity, range and orientation are verified.
pub const VERIFIED_DIGITIZERS: &[DigitizerProfile] = &[];

#[derive(Debug, Error)]
pub enum DigitizerError {
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("cannot {operation} digitizer {}: {source}", path.display())]
    Device { operation: &'static str, path: PathBuf, source: io::Error },
    #[error("digitizer is already grabbed")]
    Busy,
    #[error(
        "digitizer bus {bus:04x} {vendor:04x}:{product:04x} {name:?} with ABS axes {axes:?} has no verified calibration"
    )]
    Unverified { bus: u16, vendor: u16, product: u16, name: String, axes: Vec<AbsAxis> },
    #[error("a digitizer calibration needs a nonzero bar width")]
    EmptyBar,
}

impl DigitizerError {
    fn unverified(meta: &DigitizerMeta) -> Self {
        Self::Unverified {
            bus: meta.bus,
            vendor: meta.vendor,
            product: meta.product,
            name: meta.name.clone(),
            axes: meta.axes.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Calibration {
    axis: AbsAxis,
    last_column: u16,
    reversed: bool,
}

impl Calibration {
    fn column(self, value: i32) -> Option<u16> {
        if !(self.axis.min..=self.axis.max).contains(&value) {
            return None;
        }
        let offset = i64::from(value) - i64::from(self.axis.min);
        let span = i64::from(self.axis.max) - i64::from(self.axis.min);
        let x = u16::try_from(offset * i64::from(self.last_column) / span).ok()?;
        Some(if self.reversed { self.last_column - x } else { x })
    }
}

/// Matches the full observed identity and exact range of a profile's chosen position axis.
pub fn calibrate(
    meta: &DigitizerMeta,
    profiles: &[DigitizerProfile],
    width: u16,
) -> Result<Calibration, DigitizerError> {
    let last_column = width.checked_sub(1).ok_or(DigitizerError::EmptyBar)?;
    let supports = |code| meta.axes.iter().any(|axis| axis.code == code);
    if !supports(MT_SLOT) || !supports(MT_TRACKING_ID) {
        return Err(DigitizerError::unverified(meta));
    }
    let profile = profiles
        .iter()
        .find(|profile| {
            (profile.bus, profile.vendor, profile.product, profile.name)
                == (meta.bus, meta.vendor, meta.product, meta.name.as_str())
                && matches!(profile.axis.code, MT_X | MT_Y)
                && profile.axis.min < profile.axis.max
                && meta.axes.contains(&profile.axis)
        })
        .ok_or_else(|| DigitizerError::unverified(meta))?;
    Ok(Calibration { axis: profile.axis, last_column, reversed: profile.reversed })
}

/// Opens only the requested event path and grabs it only after binding and exact calibration.
pub fn open_digitizer(
    sysfs: &Path,
    node: &Path,
    profiles: &[DigitizerProfile],
) -> Result<EvdevTouches<File>, DigitizerError> {
    event_name(node)?;
    let error = |operation, source| DigitizerError::Device { operation, path: node.to_owned(), source };
    let opened = DigitizerNode::open(node).map_err(|source| error("open", source))?;
    let id = opened.node_id().map_err(|source| error("identify", source))?;
    confirm_digitizer_node(sysfs, node, id)?;
    let meta = opened.metadata().map_err(|source| error("read metadata from", source))?;
    if (meta.bus, meta.vendor, meta.product) != T1_ID {
        return Err(DigitizerError::unverified(&meta));
    }
    let calibration = calibrate(&meta, profiles, BarSize::MACBOOK_PRO.width)?;
    let reader = opened.grab().map_err(|source| {
        if source.raw_os_error() == Some(rustix::io::Errno::BUSY.raw_os_error()) {
            DigitizerError::Busy
        } else {
            error("grab", source)
        }
    })?;
    Ok(EvdevTouches::new(reader, calibration))
}
