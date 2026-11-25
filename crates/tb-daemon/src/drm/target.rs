//! Choosing the Touch Bar card from sysfs before any device node is opened, and confirming that an
//! opened node is still that card. Nothing here opens `/dev`; only sysfs text and links are read.
//!
//! Identity (research 0001, 0010; appletbdrm.c L817–820): the card's `device` is a USB interface
//! bound to the `appletbdrm` driver, and the USB device above that interface is the T1 `iBridge`
//! display `05ac:8600`. The driver name alone would also match a T2. Only these
//! attributes and the card's `dev` (major:minor) are read; serials never are.

use std::io;
use std::path::{Path, PathBuf};

use crate::target::{DRM_DRIVER, NodeId, SysfsDir, T1_PRODUCT, T1_VENDOR, TargetError, parse_dev};

/// Where userspace opens DRM cards; a target path must be exactly this plus a number.
const CARD_PREFIX: &str = "/dev/dri/card";
const DRI_DIR: &str = "/dev/dri";

/// `auto`: the one Touch Bar card among `sysfs/class/drm/card*`. Connector (`cardN-*`) and render
/// (`renderD*`) entries are never considered; a card that is not the T1 bar is skipped without
/// opening anything. No match is `NotFound`, more than one is `Ambiguous` (never a guess).
pub fn find_touch_bar_card(sysfs: &Path) -> Result<PathBuf, TargetError> {
    let class = sysfs.join("class/drm");
    let entries =
        std::fs::read_dir(&class).map_err(|source| TargetError::Io { path: class.clone(), source })?;
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| TargetError::Io { path: class.clone(), source })?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
        if is_card_name(&name) && identify(sysfs, &name).is_ok() {
            found.push(Path::new(DRI_DIR).join(&name));
        }
    }
    found.sort();
    match found.len() {
        0 => Err(TargetError::NotFound),
        1 => Ok(found.remove(0)),
        _ => Err(TargetError::Ambiguous(found)),
    }
}

/// An explicit `card`: it must be lexically `/dev/dri/card<digits>` (checked before sysfs is read)
/// and sysfs must name it the Touch Bar card. Returns `card` unchanged.
pub fn check_touch_bar_card(sysfs: &Path, card: &Path) -> Result<PathBuf, TargetError> {
    let name = card_name(card)?;
    identify(sysfs, name)?;
    Ok(card.to_owned())
}

/// Before acquiring master, checks the opened character device against one sysfs card and
/// refuses it if the card disappears or its class entry changes during confirmation.
pub fn confirm_touch_bar_node(sysfs: &Path, card: &Path, node: NodeId) -> Result<PathBuf, TargetError> {
    let name = card_name(card)?;
    let entry = identify(sysfs, name)?;
    let expected = entry.read("dev")?;
    let matches = parse_dev(&expected) == Some((node.major, node.minor));
    if !(node.char_device && matches) {
        return Err(TargetError::Changed { card: name.to_owned(), expected, found: node });
    }
    entry.confirm_current()?;
    Ok(card.to_owned())
}

/// `cardN` from `/dev/dri/cardN`, lexically; anything else (relative, `..`, render or connector
/// names, other dirs) is `NotACardPath`.
fn card_name(card: &Path) -> Result<&str, TargetError> {
    let refuse = || TargetError::NotACardPath { path: card.to_owned() };
    let text = card.to_str().ok_or_else(refuse)?;
    let digits = text.strip_prefix(CARD_PREFIX).ok_or_else(refuse)?;
    let name = text.strip_prefix("/dev/dri/").ok_or_else(refuse)?;
    if is_digits(digits) { Ok(name) } else { Err(refuse()) }
}

fn is_card_name(name: &str) -> bool {
    name.strip_prefix("card").is_some_and(is_digits)
}

/// One to four ASCII digits: DRM minors are below 1024 (`DRM_MINOR_LIMIT`-sized ranges).
fn is_digits(text: &str) -> bool {
    (1..=4).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

/// Pins `class/drm/<name>` and checks the driver and USB identity through that directory.
fn identify(sysfs: &Path, name: &str) -> Result<SysfsDir, TargetError> {
    let card = SysfsDir::open(&sysfs.join("class/drm").join(name))?;
    let interface = card.child("device")?;
    let driver = match interface.link("driver") {
        Ok(target) => target.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned(),
        Err(TargetError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            return Err(TargetError::Unbound { card: name.to_owned() });
        }
        Err(error) => return Err(error),
    };
    if driver != DRM_DRIVER {
        return Err(TargetError::WrongDriver { driver });
    }
    let usb = interface.child("..")?;
    let vendor = usb.read("idVendor")?;
    let product = usb.read("idProduct")?;
    if (vendor.as_str(), product.as_str()) != (T1_VENDOR, T1_PRODUCT) {
        return Err(TargetError::WrongDevice { vendor, product });
    }
    Ok(card)
}
