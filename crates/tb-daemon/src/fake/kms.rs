//! `RecordingKms`: a scripted KMS card for display tests. Its default metadata is the cited
//! `appletbdrm` shape (appletbdrm.c L554–583, L613–622, L624–720); builders change one fact each,
//! `fail_at` makes one call fail with an errno, and every attempted call is recorded in order.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use crate::drm::{
    ConnectorInfo, CrtcInfo, DirtyClip, DumbBuffer, FbSpec, Kms, KmsMetadata, ModeInfo, PlaneInfo,
};

/// Every byte of a new dumb buffer starts as this, so untouched padding is visible.
pub const DUMB_FILL: u8 = 0xA5;

const XRGB8888: u32 = u32::from_le_bytes(*b"XR24");
const BGR888: u32 = u32::from_le_bytes(*b"BG24");
const PLANE: u32 = 51;
const FIRST_HANDLE: u32 = 7;
const FIRST_FB: u32 = 61;
/// `DRM_MODE_CONNECTOR_USB`.
const CONNECTOR_USB: u32 = 20;

/// The CRTC state before the display starts: its framebuffer (0: none) and mode, if set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorCrtc {
    pub fb: u32,
    pub mode: Option<(u16, u16)>,
}

/// A KMS call `fail_at` can make fail. `Metadata` is the whole metadata read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KmsStep {
    Version,
    SetMaster,
    DropMaster,
    Metadata,
    GetFb,
    CreateDumb,
    MapDumb,
    Unmap,
    DestroyDumb,
    AddFb2,
    SetCrtc,
    Dirty,
    RmFb,
}

/// One attempted call, failing ones included. `mode` is `(hdisplay, vdisplay)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KmsCall {
    Version,
    Metadata,
    SetMaster,
    DropMaster,
    CreateDumb {
        width: u32,
        height: u32,
        bpp: u32,
    },
    MapDumb,
    Unmap,
    DestroyDumb,
    AddFb2 {
        width: u32,
        height: u32,
        format: u32,
        pitch: u32,
        fb: u32,
    },
    SetCrtc {
        crtc: u32,
        fb: u32,
        connectors: Vec<u32>,
        mode: Option<(u16, u16)>,
    },
    Dirty {
        fb: u32,
        clips: Vec<DirtyClip>,
    },
    RmFb {
        fb: u32,
    },
    /// The card was closed (the `RecordingKms` was dropped).
    Close,
}

#[derive(Debug, Default)]
struct Shared {
    calls: Vec<KmsCall>,
    pixels: Vec<u8>,
}

/// What a `RecordingKms` did; outlives it. Clones share the record.
#[derive(Debug, Clone, Default)]
pub struct KmsLog {
    shared: Arc<Mutex<Shared>>,
}

impl KmsLog {
    /// Every attempted call so far, oldest first.
    pub fn calls(&self) -> Vec<KmsCall> {
        self.lock().calls.clone()
    }

    /// The whole mapped buffer, padding included; still readable after it was destroyed.
    pub fn pixels(&self) -> Vec<u8> {
        self.lock().pixels.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, call: KmsCall) {
        self.lock().calls.push(call);
    }
}

/// A scripted card. See the module doc.
#[derive(Debug)]
pub struct RecordingKms {
    log: KmsLog,
    driver: String,
    connectors: usize,
    non_desktop: Option<u64>,
    modes: Vec<(u16, u16)>,
    orientation: Option<String>,
    formats: Vec<u32>,
    prior: PriorCrtc,
    plane_fb: u32,
    bound: bool,
    pitch: Option<u32>,
    size: Option<u64>,
    fails: Vec<(KmsStep, i32)>,
    next_fb: u32,
    mapped: bool,
}

impl RecordingKms {
    pub const CONNECTOR: u32 = 31;
    pub const CRTC: u32 = 41;

    /// The cited appletbdrm shape: driver "appletbdrm", one connected USB connector `CONNECTOR`
    /// with non-desktop 1, orientation "Right Side Up", modes `[(60, 2170)]`, CRTC inactive,
    /// plane fb 0, formats `[XRGB8888, BGR888]`, tight pitch (60 × bpp), size pitch × height.
    pub fn touch_bar() -> Self {
        Self {
            log: KmsLog::default(),
            driver: "appletbdrm".to_owned(),
            connectors: 1,
            non_desktop: Some(1),
            modes: vec![(60, 2170)],
            orientation: Some("Right Side Up".to_owned()),
            formats: vec![XRGB8888, BGR888],
            prior: PriorCrtc { fb: 0, mode: None },
            plane_fb: 0,
            bound: false,
            pitch: None,
            size: None,
            fails: Vec::new(),
            next_fb: FIRST_FB,
            mapped: false,
        }
    }

    #[must_use]
    pub fn with_driver(mut self, driver: &str) -> Self {
        driver.clone_into(&mut self.driver);
        self
    }

    /// `n` connected USB connectors, ids `CONNECTOR`, `CONNECTOR + 1`, …
    #[must_use]
    pub fn with_connectors(mut self, n: usize) -> Self {
        self.connectors = n;
        self
    }

    #[must_use]
    pub fn with_non_desktop(mut self, value: Option<u64>) -> Self {
        self.non_desktop = value;
        self
    }

    /// The connector's modes as `(hdisplay, vdisplay)`.
    #[must_use]
    pub fn with_modes(mut self, modes: &[(u16, u16)]) -> Self {
        self.modes = modes.to_vec();
        self
    }

    #[must_use]
    pub fn with_orientation(mut self, orientation: Option<&str>) -> Self {
        self.orientation = orientation.map(str::to_owned);
        self
    }

    #[must_use]
    pub fn with_formats(mut self, formats: &[u32]) -> Self {
        self.formats = formats.to_vec();
        self
    }

    #[must_use]
    pub fn with_prior_crtc(mut self, prior: PriorCrtc) -> Self {
        self.prior = prior;
        self
    }

    #[must_use]
    pub fn with_plane_fb(mut self, fb: u32) -> Self {
        self.plane_fb = fb;
        self
    }

    /// The connector's encoder already drives `CRTC`.
    #[must_use]
    pub fn with_connector_bound(mut self, bound: bool) -> Self {
        self.bound = bound;
        self
    }

    /// The pitch `CREATE_DUMB` answers; the size follows as pitch × height unless set.
    #[must_use]
    pub fn with_dumb_pitch(mut self, pitch: u32) -> Self {
        self.pitch = Some(pitch);
        self
    }

    #[must_use]
    pub fn with_dumb_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// The first `step` call fails with `errno`; chainable.
    #[must_use]
    pub fn fail_at(mut self, step: KmsStep, errno: i32) -> Self {
        self.fails.push((step, errno));
        self
    }

    pub fn log(&self) -> KmsLog {
        self.log.clone()
    }

    /// Records `call`, then fails it if `step` is scripted to fail (once).
    fn attempt(&mut self, step: KmsStep, call: KmsCall) -> io::Result<()> {
        self.log.push(call);
        match self.fails.iter().position(|(s, _)| *s == step) {
            Some(i) => Err(io::Error::from_raw_os_error(self.fails.remove(i).1)),
            None => Ok(()),
        }
    }

    fn mode(hdisplay: u16, vdisplay: u16) -> ModeInfo {
        ModeInfo { hdisplay, vdisplay, vrefresh: 60, ..ModeInfo::default() }
    }

    fn scripted_metadata(&self) -> KmsMetadata {
        let connectors = (0..self.connectors)
            .map(|i| ConnectorInfo {
                id: Self::CONNECTOR + u32::try_from(i).unwrap_or(u32::MAX - Self::CONNECTOR),
                kind: CONNECTOR_USB,
                connected: true,
                non_desktop: self.non_desktop,
                orientation: self.orientation.clone(),
                modes: self.modes.iter().map(|(h, v)| Self::mode(*h, *v)).collect(),
                bound_crtc: if self.bound { Self::CRTC } else { 0 },
                possible_crtcs: 1,
            })
            .collect();
        let crtc = CrtcInfo {
            id: Self::CRTC,
            fb: self.prior.fb,
            mode: self.prior.mode.map(|(h, v)| Self::mode(h, v)),
        };
        let plane = PlaneInfo {
            id: PLANE,
            primary: true,
            crtc: if self.plane_fb == 0 { 0 } else { Self::CRTC },
            fb: self.plane_fb,
            possible_crtcs: 1,
            formats: self.formats.clone(),
        };
        KmsMetadata { connectors, crtcs: vec![crtc], planes: vec![plane] }
    }
}

impl Kms for RecordingKms {
    fn driver_name(&mut self) -> io::Result<String> {
        self.attempt(KmsStep::Version, KmsCall::Version)?;
        Ok(self.driver.clone())
    }

    fn set_master(&mut self) -> io::Result<()> {
        self.attempt(KmsStep::SetMaster, KmsCall::SetMaster)
    }

    fn drop_master(&mut self) -> io::Result<()> {
        self.attempt(KmsStep::DropMaster, KmsCall::DropMaster)
    }

    fn metadata(&mut self) -> io::Result<KmsMetadata> {
        self.attempt(KmsStep::Metadata, KmsCall::Metadata)?;
        Ok(self.scripted_metadata())
    }

    fn create_dumb(&mut self, width: u32, height: u32, bpp: u32) -> io::Result<DumbBuffer> {
        self.attempt(KmsStep::CreateDumb, KmsCall::CreateDumb { width, height, bpp })?;
        let pitch = self.pitch.unwrap_or(width.saturating_mul(bpp / 8));
        let size = self.size.unwrap_or(u64::from(pitch) * u64::from(height));
        Ok(DumbBuffer { handle: FIRST_HANDLE, pitch, size })
    }

    /// A buffer of exactly `len` bytes, all `DUMB_FILL`: an out-of-bounds write panics the test.
    fn map_dumb(&mut self, _handle: u32, len: usize) -> io::Result<()> {
        self.attempt(KmsStep::MapDumb, KmsCall::MapDumb)?;
        self.log.lock().pixels = vec![DUMB_FILL; len];
        self.mapped = true;
        Ok(())
    }

    fn with_pixels(&mut self, write: &mut dyn FnMut(&mut [u8])) {
        if self.mapped {
            write(&mut self.log.lock().pixels);
        } else {
            write(&mut []);
        }
    }

    fn unmap(&mut self) -> io::Result<()> {
        self.mapped = false;
        self.attempt(KmsStep::Unmap, KmsCall::Unmap)
    }

    fn destroy_dumb(&mut self, _handle: u32) -> io::Result<()> {
        self.attempt(KmsStep::DestroyDumb, KmsCall::DestroyDumb)
    }

    fn add_fb2(&mut self, fb: &FbSpec) -> io::Result<u32> {
        let id = self.next_fb;
        let call = KmsCall::AddFb2 {
            width: fb.width,
            height: fb.height,
            format: fb.format,
            pitch: fb.pitch,
            fb: id,
        };
        self.attempt(KmsStep::AddFb2, call)?;
        self.next_fb += 1;
        Ok(id)
    }

    fn set_crtc(
        &mut self,
        crtc: u32,
        fb: u32,
        connectors: &[u32],
        mode: Option<&ModeInfo>,
    ) -> io::Result<()> {
        let mode = mode.map(|m| (m.hdisplay, m.vdisplay));
        self.attempt(KmsStep::SetCrtc, KmsCall::SetCrtc { crtc, fb, connectors: connectors.to_vec(), mode })
    }

    fn dirty(&mut self, fb: u32, clips: &[DirtyClip]) -> io::Result<()> {
        self.attempt(KmsStep::Dirty, KmsCall::Dirty { fb, clips: clips.to_vec() })
    }

    fn rm_fb(&mut self, fb: u32) -> io::Result<()> {
        self.attempt(KmsStep::RmFb, KmsCall::RmFb { fb })
    }
}

impl Drop for RecordingKms {
    fn drop(&mut self) {
        self.log.push(KmsCall::Close);
    }
}
