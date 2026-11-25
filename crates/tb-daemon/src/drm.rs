//! The KMS display backend (ADR-0002): presents frames to the Touch Bar's `appletbdrm` card with
//! legacy KMS, and restores the card's prior, inactive presentation when done.
//!
//! `start` validates and allocates but shows nothing; the first non-empty present writes the
//! whole frame and modesets; later presents write the damage and send it as DIRTYFB clips.
//! A successful SETCRTC or DIRTYFB means the frame was *submitted*: the driver's plane update
//! discards the USB transfer result (appletbdrm.c at stable 497791a0, L474–505), so a lit, legible
//! frame is hardware evidence (TB-10), not something this code can know.

mod card;
mod convert;
mod select;
mod target;
mod uapi;

use std::io;

use tb_core::{BarSize, Rect};
use tb_render::Frame;
use thiserror::Error;

use crate::display::{Display, DisplayError, check};
use crate::target::DRM_DRIVER;
use convert::{Layout, clips};
use select::{Target, fb_size, select};

pub use card::CardKms;
pub use target::{check_touch_bar_card, confirm_touch_bar_node, find_touch_bar_card};

/// `EBUSY` and `ENODEV` in `asm-generic/errno-base.h`.
const EBUSY: i32 = 16;
const ENODEV: i32 = 19;

/// `struct drm_mode_modeinfo` (`drm_mode.h`), as GETCONNECTOR lists it and SETCRTC takes it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModeInfo {
    pub clock: u32,
    pub hdisplay: u16,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub hskew: u16,
    pub vdisplay: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    pub vscan: u16,
    pub vrefresh: u32,
    pub flags: u32,
    pub kind: u32,
    pub name: [u8; 32],
}

/// One damaged area in framebuffer coordinates, end-exclusive: `struct drm_clip_rect` (`drm.h`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyClip {
    pub x1: u16,
    pub y1: u16,
    pub x2: u16,
    pub y2: u16,
}

/// What a card reports about its display pipeline, read before anything is changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KmsMetadata {
    pub connectors: Vec<ConnectorInfo>,
    pub crtcs: Vec<CrtcInfo>,
    pub planes: Vec<PlaneInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectorInfo {
    pub id: u32,
    /// `DRM_MODE_CONNECTOR_*`.
    pub kind: u32,
    pub connected: bool,
    /// The `non-desktop` property, if the connector has one.
    pub non_desktop: Option<u64>,
    /// The `panel orientation` enum name, if the connector has one.
    pub orientation: Option<String>,
    pub modes: Vec<ModeInfo>,
    /// The CRTC its current encoder drives (0: none).
    pub bound_crtc: u32,
    /// Union of its encoders' `possible_crtcs`: bit i = `crtcs[i]`.
    pub possible_crtcs: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrtcInfo {
    pub id: u32,
    pub fb: u32,
    pub mode: Option<ModeInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaneInfo {
    pub id: u32,
    /// Its `type` property is "Primary".
    pub primary: bool,
    pub crtc: u32,
    pub fb: u32,
    pub possible_crtcs: u32,
    pub formats: Vec<u32>,
}

/// A dumb buffer as `CREATE_DUMB` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DumbBuffer {
    pub handle: u32,
    pub pitch: u32,
    pub size: u64,
}

/// The framebuffer ADDFB2 registers: one plane, no modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FbSpec {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub handle: u32,
    pub pitch: u32,
}

/// The KMS calls the display makes, one method per ioctl group. Closing is dropping. `CardKms`
/// is the device; `fake::RecordingKms` scripts metadata and records calls for tests.
pub trait Kms {
    /// `DRM_IOCTL_VERSION`: the driver's name.
    fn driver_name(&mut self) -> io::Result<String>;
    fn set_master(&mut self) -> io::Result<()>;
    fn drop_master(&mut self) -> io::Result<()>;
    /// Connectors, CRTCs and planes with the properties `select` needs; changes nothing visible.
    fn metadata(&mut self) -> io::Result<KmsMetadata>;
    fn create_dumb(&mut self, width: u32, height: u32, bpp: u32) -> io::Result<DumbBuffer>;
    /// `MAP_DUMB`, then maps `len` bytes of the buffer for writing.
    fn map_dumb(&mut self, handle: u32, len: usize) -> io::Result<()>;
    /// Calls `write` with the mapped bytes (empty when nothing is mapped).
    fn with_pixels(&mut self, write: &mut dyn FnMut(&mut [u8]));
    fn unmap(&mut self) -> io::Result<()>;
    fn destroy_dumb(&mut self, handle: u32) -> io::Result<()>;
    /// ADDFB2: the new framebuffer's id.
    fn add_fb2(&mut self, fb: &FbSpec) -> io::Result<u32>;
    /// SETCRTC; `fb` 0 with no connectors and no mode disables the CRTC.
    fn set_crtc(&mut self, crtc: u32, fb: u32, connectors: &[u32], mode: Option<&ModeInfo>)
    -> io::Result<()>;
    fn dirty(&mut self, fb: u32, clips: &[DirtyClip]) -> io::Result<()>;
    fn rm_fb(&mut self, fb: u32) -> io::Result<()>;
}

/// One failed release step during a restore.
#[derive(Debug)]
pub struct StepFailure {
    pub step: &'static str,
    pub source: io::Error,
}

#[derive(Debug, Error)]
pub enum DrmError {
    #[error("the card's driver is {driver:?}, not appletbdrm; closed without taking it")]
    WrongDriver { driver: String },
    #[error("another client holds the card as DRM master; closed untouched: {source}")]
    Busy { source: io::Error },
    #[error("not a supported Touch Bar panel: {reason}; released untouched")]
    Unsupported { reason: String },
    #[error("the card already presents something ({reason}); only an inactive panel is taken over")]
    Active { reason: String },
    #[error("KMS {step} failed: {source}; released untouched")]
    Device { step: &'static str, source: io::Error },
    #[error("the card is gone; nothing to restore")]
    Gone { changed: bool },
    #[error("restore failed at {}", .failures.iter().map(|f| format!("{}: {}", f.step, f.source)).collect::<Vec<_>>().join(", "))]
    Restore { failures: Vec<StepFailure>, changed: bool },
}

impl DrmError {
    /// Whether this session modeset the card and could not cleanly undo everything. Every start
    /// refusal is `false`: nothing visible was changed.
    pub fn changed(&self) -> bool {
        match self {
            DrmError::Gone { changed } | DrmError::Restore { changed, .. } => *changed,
            _ => false,
        }
    }
}

/// Where a started display is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Allocated, nothing shown yet.
    Ready,
    /// The modeset happened: the bar shows our framebuffer.
    Shown,
    /// ENODEV: the card went away; `shown` tells whether a modeset had happened.
    Gone { shown: bool },
    /// `restore` ran; every later call is a no-op.
    Restored,
}

/// A started KMS display. `restore` (or dropping it) disables the CRTC if it was modeset, then
/// releases the framebuffer, mapping, buffer and master, in that order, exactly once.
/// Deliberately not `Debug`: the frozen contract tests unwrap refusals with `.err().expect()`,
/// which clippy rejects (`err_expect`) once the `Ok` type is `Debug`.
pub struct DrmDisplay<K: Kms> {
    kms: K,
    size: BarSize,
    target: Target,
    layout: Layout,
    handle: u32,
    fb: u32,
    phase: Phase,
}

impl<K: Kms> DrmDisplay<K> {
    /// Checks the driver, takes master, validates the metadata (`select`), and allocates and
    /// registers a framebuffer, showing nothing. Any refusal releases the completed steps in
    /// reverse and drops `kms` (closing the card). Release errors while unwinding are not
    /// reported: closing the file releases master and every buffer it owns anyway.
    pub fn start(mut kms: K, size: BarSize) -> Result<Self, DrmError> {
        let driver = kms.driver_name().map_err(|source| DrmError::Device { step: "version", source })?;
        if driver != DRM_DRIVER {
            return Err(DrmError::WrongDriver { driver });
        }
        if let Err(source) = kms.set_master() {
            return Err(if source.raw_os_error() == Some(EBUSY) {
                DrmError::Busy { source }
            } else {
                DrmError::Device { step: "set_master", source }
            });
        }
        match allocate(&mut kms, size) {
            Ok((target, layout, handle, fb)) => {
                Ok(Self { kms, size, target, layout, handle, fb, phase: Phase::Ready })
            }
            Err(err) => {
                let _ = kms.drop_master();
                Err(err)
            }
        }
    }

    /// Disables the CRTC if this display modeset it (the prior state was inactive, so that is
    /// the prior presentation), then RMFB, unmap, `DESTROY_DUMB` and `DROP_MASTER`. Every step is
    /// attempted; failures are returned together. After ENODEV nothing is restored: `Gone`.
    /// Runs once; later calls return `Ok` and make no call.
    pub fn restore(&mut self) -> Result<(), DrmError> {
        let phase = std::mem::replace(&mut self.phase, Phase::Restored);
        let (shown, gone) = match phase {
            Phase::Restored => return Ok(()),
            Phase::Ready => (false, false),
            Phase::Shown => (true, false),
            Phase::Gone { shown } => (shown, true),
        };
        let mut failures = Vec::new();
        let mut attempt = |step, result: io::Result<()>| {
            if let Err(source) = result {
                failures.push(StepFailure { step, source });
            }
        };
        if shown && !gone {
            attempt("disable crtc", self.kms.set_crtc(self.target.crtc, 0, &[], None));
        }
        attempt("rm_fb", self.kms.rm_fb(self.fb));
        attempt("unmap", self.kms.unmap());
        attempt("destroy_dumb", self.kms.destroy_dumb(self.handle));
        attempt("drop_master", self.kms.drop_master());
        let gone = gone || failures.iter().any(|f| f.source.raw_os_error() == Some(ENODEV));
        if gone {
            Err(DrmError::Gone { changed: shown })
        } else if failures.is_empty() {
            Ok(())
        } else {
            Err(DrmError::Restore { failures, changed: shown })
        }
    }

    fn write(&mut self, frame: &Frame, rects: &[Rect]) -> Result<(), DisplayError> {
        let layout = self.layout;
        let mut written = true;
        self.kms.with_pixels(&mut |pixels| {
            written = rects.iter().all(|r| layout.write_rect(frame, pixels, *r).is_some());
        });
        if written {
            Ok(())
        } else {
            let source = io::Error::new(
                io::ErrorKind::InvalidData,
                "the mapped framebuffer is smaller than validated",
            );
            Err(DisplayError::Device { source, gone: false })
        }
    }

    /// Maps a failed call to a display error, remembering ENODEV for `restore`.
    fn failed(&mut self, source: io::Error) -> DisplayError {
        let gone = source.raw_os_error() == Some(ENODEV);
        if gone {
            self.phase = Phase::Gone { shown: self.phase == Phase::Shown };
        }
        DisplayError::Device { source, gone }
    }
}

/// The steps of `start` after master: metadata, validation, dumb buffer, mapping, framebuffer.
/// Each failure releases what this function took, in reverse.
fn allocate<K: Kms>(kms: &mut K, size: BarSize) -> Result<(Target, Layout, u32, u32), DrmError> {
    let meta = kms.metadata().map_err(|source| DrmError::Device { step: "metadata", source })?;
    let target = select(&meta, size)?;
    let (fb_w, fb_h) = fb_size(size);
    let (width, height) = (u32::from(fb_w), u32::from(fb_h));
    let bpp = target.format.bytes() * 8;
    let dumb = kms
        .create_dumb(width, height, bpp)
        .map_err(|source| DrmError::Device { step: "create_dumb", source })?;
    let checked =
        convert::check_buffer((width, height), target.format, dumb.pitch, dumb.size).and_then(|bounds| {
            if dumb.handle == 0 { Err("CREATE_DUMB returned handle 0".to_owned()) } else { Ok(bounds) }
        });
    let (pitch, len) = match checked {
        Ok(bounds) => bounds,
        Err(reason) => {
            let _ = kms.destroy_dumb(dumb.handle);
            return Err(DrmError::Unsupported { reason });
        }
    };
    if let Err(source) = kms.map_dumb(dumb.handle, len) {
        let _ = kms.destroy_dumb(dumb.handle);
        return Err(DrmError::Device { step: "map_dumb", source });
    }
    let spec =
        FbSpec { width, height, format: target.format.fourcc(), handle: dumb.handle, pitch: dumb.pitch };
    match kms.add_fb2(&spec) {
        Ok(fb) if fb != 0 => {
            let layout = Layout { format: target.format, fb_w: usize::from(fb_w), pitch };
            Ok((target, layout, dumb.handle, fb))
        }
        result => {
            let source = result
                .err()
                .unwrap_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ADDFB2 returned id 0"));
            let _ = kms.unmap();
            let _ = kms.destroy_dumb(dumb.handle);
            Err(DrmError::Device { step: "add_fb2", source })
        }
    }
}

impl<K: Kms> Display for DrmDisplay<K> {
    /// Checks the frame and damage before any byte or call. The first non-empty present writes
    /// the whole frame and modesets; later ones write the damage and send DIRTYFB.
    fn present(&mut self, frame: &Frame, damage: &[Rect]) -> Result<(), DisplayError> {
        check((u32::from(self.size.width), u32::from(self.size.height)), frame, damage)?;
        if damage.is_empty() {
            return Ok(());
        }
        match self.phase {
            Phase::Ready => {
                let whole = Rect { x: 0, y: 0, width: self.size.width, height: self.size.height };
                self.write(frame, &[whole])?;
                let t = &self.target;
                match self.kms.set_crtc(t.crtc, self.fb, &[t.connector], Some(&t.mode)) {
                    Ok(()) => self.phase = Phase::Shown,
                    Err(source) => return Err(self.failed(source)),
                }
                Ok(())
            }
            Phase::Shown => {
                self.write(frame, damage)?;
                let (fb_w, fb_h) = fb_size(self.size);
                let clips = clips(fb_w, fb_h, damage);
                self.kms.dirty(self.fb, &clips).map_err(|source| self.failed(source))
            }
            Phase::Gone { .. } => {
                Err(DisplayError::Device { source: io::Error::from_raw_os_error(ENODEV), gone: true })
            }
            Phase::Restored => {
                let source = io::Error::new(io::ErrorKind::NotConnected, "the display was restored");
                Err(DisplayError::Device { source, gone: false })
            }
        }
    }
}

impl<K: Kms> Drop for DrmDisplay<K> {
    /// The fallback for an unwinding or forgotten owner: restore once (its result has nowhere to
    /// go), then `kms` is dropped, closing the card.
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
