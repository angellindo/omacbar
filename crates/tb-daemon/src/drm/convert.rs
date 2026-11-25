//! Pure pixel work of the KMS display: format choice, buffer bounds, rotation and damage clips.
//!
//! Geometry (research 0001; appletbdrm.c L409–427, L681–686): the panel reports a portrait
//! framebuffer as wide as the bar is tall (60) and as tall as the bar is wide (2170). The driver
//! maps fb (col, row) to device (row, H − 1 − col), so bar pixel (bx, by) lands at fb column
//! `fb_w − 1 − by`, row `bx`. Byte order per `/usr/include/drm/drm_fourcc.h` (little-endian):
//! XRGB8888 `[31:0] x:R:G:B` is bytes B, G, R, x; BGR888 `[23:0] B:G:R` is bytes R, G, B.
//! Which physical end is the bar's left edge stays hardware-unverified (TB-10).

use tb_core::Rect;
use tb_render::Frame;

use super::DirtyClip;

/// `DRM_FORMAT_XRGB8888` = fourcc `XR24`.
pub(crate) const XRGB8888: u32 = u32::from_le_bytes(*b"XR24");
/// `DRM_FORMAT_BGR888` = fourcc `BG24`.
pub(crate) const BGR888: u32 = u32::from_le_bytes(*b"BG24");
/// `DRM_MODE_FB_DIRTY_MAX_CLIPS` in `drm_mode.h`.
const MAX_CLIPS: usize = 256;
/// A dumb buffer larger than this is refused before it is mapped: the bar needs at most
/// 2170 × 4 KiB rows; a bigger size is a driver answer this code does not trust.
const MAX_MAP: u64 = 64 << 20;

/// A framebuffer format this backend can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Xrgb8888,
    Bgr888,
}

impl Format {
    pub(crate) fn fourcc(self) -> u32 {
        match self {
            Format::Xrgb8888 => XRGB8888,
            Format::Bgr888 => BGR888,
        }
    }

    pub(crate) fn bytes(self) -> u32 {
        match self {
            Format::Xrgb8888 => 4,
            Format::Bgr888 => 3,
        }
    }

    /// One RGBA pixel in this format's byte order; alpha is dropped (frames are opaque).
    fn write(self, dst: &mut [u8], [r, g, b, _]: [u8; 4]) -> Option<()> {
        match self {
            Format::Xrgb8888 => dst.get_mut(..4)?.copy_from_slice(&[b, g, r, 0xff]),
            Format::Bgr888 => dst.get_mut(..3)?.copy_from_slice(&[r, g, b]),
        }
        Some(())
    }
}

/// XRGB8888 when the plane offers it (the tiny-dfr-proven path), else BGR888, else none.
pub(crate) fn negotiate(formats: &[u32]) -> Option<Format> {
    [Format::Xrgb8888, Format::Bgr888].into_iter().find(|f| formats.contains(&f.fourcc()))
}

/// Checks the kernel's dumb-buffer answer against the framebuffer size: the pitch holds a row,
/// `pitch × fb_h` fits in `size`, and `size` is mappable. Returns `(pitch, size)` as `usize`.
pub(crate) fn check_buffer(
    fb: (u32, u32),
    format: Format,
    pitch: u32,
    size: u64,
) -> Result<(usize, usize), String> {
    let (fb_w, fb_h) = fb;
    let row = fb_w.checked_mul(format.bytes()).ok_or("row size overflows")?;
    if pitch < row {
        return Err(format!("kernel pitch {pitch} is below one {fb_w}-pixel row ({row} bytes)"));
    }
    let needed = u64::from(pitch).checked_mul(u64::from(fb_h)).ok_or("pitch × height overflows")?;
    if size < needed {
        return Err(format!("dumb buffer of {size} bytes is below pitch {pitch} × {fb_h} rows"));
    }
    if size > MAX_MAP {
        return Err(format!("dumb buffer of {size} bytes exceeds the {MAX_MAP}-byte limit"));
    }
    let pitch = usize::try_from(pitch).map_err(|_| "pitch does not fit in memory")?;
    let size = usize::try_from(size).map_err(|_| "buffer does not fit in memory")?;
    Ok((pitch, size))
}

/// Where the bar's pixels go in a mapped framebuffer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    pub(crate) format: Format,
    /// Framebuffer width in pixels = the bar's height.
    pub(crate) fb_w: usize,
    /// Bytes per framebuffer row, from the kernel; bytes past `fb_w × bpp` are never written.
    pub(crate) pitch: usize,
}

impl Layout {
    /// Writes `rect` of the bar `frame` into `pixels`, rotated. `None` if a computed offset falls
    /// outside `frame` or `pixels` (the caller checked the rect and the buffer, so it never does).
    pub(crate) fn write_rect(&self, frame: &Frame, pixels: &mut [u8], rect: Rect) -> Option<()> {
        if !has_area(rect) {
            return Some(());
        }
        let bar_w = usize::try_from(frame.width).ok()?;
        let bpp = usize::try_from(self.format.bytes()).ok()?;
        let row_bytes = self.fb_w.checked_mul(bpp)?;
        let (x0, y0) = (usize::from(rect.x), usize::from(rect.y));
        for bx in x0..x0 + usize::from(rect.width) {
            let start = bx.checked_mul(self.pitch)?;
            let row = pixels.get_mut(start..start.checked_add(row_bytes)?)?;
            for by in y0..y0 + usize::from(rect.height) {
                let col = self.fb_w.checked_sub(1)?.checked_sub(by)?;
                let src = (by.checked_mul(bar_w)?.checked_add(bx)?).checked_mul(4)?;
                let rgba: [u8; 4] = frame.rgba.get(src..src + 4)?.try_into().ok()?;
                self.format.write(row.get_mut(col * bpp..)?, rgba)?;
            }
        }
        Some(())
    }
}

pub(super) fn has_area(rect: Rect) -> bool {
    rect.width > 0 && rect.height > 0
}

/// The DIRTYFB clips for bar `damage` (each already checked to fit the bar), in fb coordinates.
/// More than `DRM_MODE_FB_DIRTY_MAX_CLIPS` rects become one whole-framebuffer clip.
pub(crate) fn clips(fb_w: u16, fb_h: u16, damage: &[Rect]) -> Vec<DirtyClip> {
    let damage = damage.iter().copied().filter(|&rect| has_area(rect));
    if damage.clone().count() > MAX_CLIPS {
        return vec![DirtyClip { x1: 0, y1: 0, x2: fb_w, y2: fb_h }];
    }
    damage
        .map(|r| DirtyClip {
            x1: fb_w.saturating_sub(r.y.saturating_add(r.height)),
            y1: r.x,
            x2: fb_w.saturating_sub(r.y),
            y2: r.x.saturating_add(r.width),
        })
        .collect()
}
