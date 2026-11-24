//! Validating a card's KMS metadata before anything is allocated: the one bar connector, its mode,
//! orientation and non-desktop flag, the CRTC and primary plane that drive it, and a fully
//! inactive prior state. Pure: metadata in, the chosen objects or a refusal out.
//!
//! Expected shape (appletbdrm.c L554–583, L613–622, L624–720): one USB connector with
//! `non-desktop` = 1 and panel orientation `RIGHT_UP` (named "Right Side Up" in Linux 7d0a66e4
//! `drm_connector.c` L1149–1154), one fixed portrait mode, one CRTC, one primary plane offering
//! XRGB8888 and/or BGR888. That the patched T1 driver keeps this shape is unverified (TB-10).

use tb_core::BarSize;

use super::convert::{Format, negotiate};
use super::{DrmError, KmsMetadata, ModeInfo};

/// `DRM_MODE_CONNECTOR_USB` in `drm_mode.h`.
pub(crate) const CONNECTOR_USB: u32 = 20;
/// The kernel's name for `DRM_MODE_PANEL_ORIENTATION_RIGHT_UP`.
const RIGHT_SIDE_UP: &str = "Right Side Up";

/// The KMS objects a start takes, and the format it writes.
#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub(crate) connector: u32,
    pub(crate) crtc: u32,
    pub(crate) mode: ModeInfo,
    pub(crate) format: Format,
}

/// The framebuffer size for a bar: portrait, as wide as the bar is tall.
pub(crate) fn fb_size(size: BarSize) -> (u16, u16) {
    (size.height, size.width)
}

/// Chooses the target, or refuses: `Unsupported` for a shape this backend does not drive,
/// `Active` when anything already scans out or holds the bar's objects.
pub(crate) fn select(meta: &KmsMetadata, size: BarSize) -> Result<Target, DrmError> {
    let unsupported = |reason: String| DrmError::Unsupported { reason };
    let [connector] = meta.connectors.as_slice() else {
        return Err(unsupported(format!("{} connectors, the bar has exactly one", meta.connectors.len())));
    };
    if connector.kind != CONNECTOR_USB || !connector.connected {
        return Err(unsupported(format!(
            "connector {} is type {} (connected: {}), not a connected USB connector",
            connector.id, connector.kind, connector.connected
        )));
    }
    if connector.non_desktop != Some(1) {
        return Err(unsupported(format!("non-desktop is {:?}, not 1", connector.non_desktop)));
    }
    let (fb_w, fb_h) = fb_size(size);
    let Some(mode) = connector.modes.iter().find(|m| (m.hdisplay, m.vdisplay) == (fb_w, fb_h)) else {
        let modes: Vec<_> = connector.modes.iter().map(|m| (m.hdisplay, m.vdisplay)).collect();
        return Err(unsupported(format!("no {fb_w}x{fb_h} mode among {modes:?}")));
    };
    if connector.orientation.as_deref() != Some(RIGHT_SIDE_UP) {
        return Err(unsupported(format!(
            "panel orientation {:?}, not {RIGHT_SIDE_UP:?}",
            connector.orientation
        )));
    }
    let can_drive = |mask: u32, index: usize| {
        u32::try_from(index).ok().and_then(|i| mask.checked_shr(i)).is_some_and(|m| m & 1 == 1)
    };
    let crtcs: Vec<_> =
        meta.crtcs.iter().enumerate().filter(|(i, _)| can_drive(connector.possible_crtcs, *i)).collect();
    let [(index, crtc)] = crtcs.as_slice() else {
        return Err(unsupported(format!("{} CRTCs can drive the connector, need exactly one", crtcs.len())));
    };
    let planes: Vec<_> =
        meta.planes.iter().filter(|p| p.primary && can_drive(p.possible_crtcs, *index)).collect();
    let [plane] = planes.as_slice() else {
        return Err(unsupported(format!("{} primary planes for the CRTC, need exactly one", planes.len())));
    };
    let format = negotiate(&plane.formats).ok_or_else(|| {
        unsupported(format!("primary plane formats {:x?} have neither XRGB8888 nor BGR888", plane.formats))
    })?;
    inactive(meta)?;
    Ok(Target { connector: connector.id, crtc: crtc.id, mode: *mode, format })
}

/// Only a proven inactive prior state is taken over: no CRTC with a framebuffer or a mode, no
/// plane with a framebuffer or a CRTC, no connector bound to a CRTC. A foreign frame cannot be
/// pinned by legacy KMS, so restoring it could not be promised.
fn inactive(meta: &KmsMetadata) -> Result<(), DrmError> {
    let active = |reason: String| Err(DrmError::Active { reason });
    if let Some(c) = meta.crtcs.iter().find(|c| c.fb != 0 || c.mode.is_some()) {
        return active(format!("CRTC {} shows framebuffer {} (mode set: {})", c.id, c.fb, c.mode.is_some()));
    }
    if let Some(p) = meta.planes.iter().find(|p| p.fb != 0 || p.crtc != 0) {
        return active(format!("plane {} holds framebuffer {} on CRTC {}", p.id, p.fb, p.crtc));
    }
    if let Some(c) = meta.connectors.iter().find(|c| c.bound_crtc != 0) {
        return active(format!("connector {} is bound to CRTC {}", c.id, c.bound_crtc));
    }
    Ok(())
}
