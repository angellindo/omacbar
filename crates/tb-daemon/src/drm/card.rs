//! The real KMS boundary: DRM ioctls on an opened card node, and the dumb buffer's mapping.
//! This is the only module of the display backend allowed `unsafe` (R7; ADR-0014 records it).
//!
//! The structs and opcodes are in `uapi.rs`, layout-asserted against the headers. Every count the
//! kernel reports is bounded before a buffer is allocated, and a count that grew between the two
//! calls of a query is refused rather than trusted (the kernel copies only what fits).
#![allow(unsafe_code, reason = "DRM ioctls and the dumb-buffer mmap, ADR-0014")]

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::path::Path;
use std::ptr::{NonNull, null_mut};

use rustix::fs::{Mode, OFlags};
use rustix::ioctl::{NoArg, Opcode, Setter, Updater, ioctl};
use rustix::mm::{MapFlags, ProtFlags};

use super::uapi::{
    ADD_FB2, CAP_UNIVERSAL_PLANES, CONNECTED, CREATE_DUMB, CardRes, ClientCap, Connector, CreateDumb, Crtc,
    DESTROY_DUMB, DIRTY_FB, DROP_MASTER, Encoder, FbCmd2, FbDirty, GET_CONNECTOR, GET_CRTC, GET_ENCODER,
    GET_PLANE, GET_PLANE_RESOURCES, GET_PROPERTY, GET_RESOURCES, MAP_DUMB, MAX_ENUMS, MAX_FORMATS, MAX_MODES,
    MAX_OBJECTS, MAX_PROPS, MapDumb, OBJ_GET_PROPERTIES, OBJECT_PLANE, ObjProperties, PROP_ENUM, Plane,
    PlaneRes, Property, PropertyEnum, RM_FB, SET_CLIENT_CAP, SET_CRTC, SET_MASTER, VERSION, Version,
};
use super::{ConnectorInfo, CrtcInfo, DirtyClip, DumbBuffer, FbSpec, Kms, KmsMetadata, ModeInfo, PlaneInfo};
use crate::target::NodeId;

/// The address of a buffer the kernel reads or fills, as uapi `__u64`.
fn addr<T>(ptr: *const T) -> u64 {
    // Lossless: pointers are 64-bit (asserted in `uapi.rs`). Exposed: the kernel dereferences it.
    ptr.expose_provenance() as u64
}

/// `IOWR` with `value` both ways.
///
/// # Safety
/// `OP` must name an ioctl whose argument is exactly `T` (`repr(C)`, layout asserted above), and
/// every user pointer inside `value` must point to a live buffer holding at least the count the
/// same struct gives for it, writable when the kernel fills it, for the duration of the call.
unsafe fn update<const OP: Opcode, T>(file: &File, value: &mut T) -> io::Result<()> {
    // SAFETY: forwarded from this function's contract; `Updater` borrows `value` for the call.
    unsafe { ioctl(file, Updater::<OP, T>::new(value)) }.map_err(io::Error::from)
}

/// A count from the kernel, refused above `max`.
fn bounded(count: u32, max: u32, what: &str) -> io::Result<usize> {
    if count > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{count} {what} reported, at most {max} accepted"),
        ));
    }
    usize::try_from(count).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The second call of a two-call query: the kernel reports `got`, filled up to `room`. More than
/// `room` means the object changed between the calls; it is refused, not retried.
fn filled<T>(mut items: Vec<T>, got: u32, what: &str) -> io::Result<Vec<T>> {
    let got = usize::try_from(got).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if got > items.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{what} changed while being read")));
    }
    items.truncate(got);
    Ok(items)
}

/// The text of a NUL-padded uapi name.
fn name_of(bytes: &[u8]) -> &[u8] {
    bytes.split(|b| *b == 0).next().unwrap_or_default()
}

/// A property's name, its value, and the value's enum name when it is an enum.
type NamedProperty = (Vec<u8>, u64, Option<String>);

/// The dumb buffer mapped for writing.
struct Mapping {
    ptr: NonNull<u8>,
    len: usize,
}

/// An opened DRM card node. Opening does nothing else; dropping unmaps (if still mapped) and
/// closes the file, which releases master and every buffer and framebuffer this file made.
pub struct CardKms {
    file: File,
    mapping: Option<Mapping>,
}

impl std::fmt::Debug for CardKms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardKms")
            .field("file", &self.file)
            .field("mapped", &self.mapping.as_ref().map(|m| m.len))
            .finish()
    }
}

impl CardKms {
    /// Opens `path` read-write, close-on-exec, without following a final symlink, without
    /// becoming its controlling terminal, and never creating it. No ioctl is made.
    pub fn open(path: &Path) -> io::Result<CardKms> {
        let flags = OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NOCTTY;
        let fd = rustix::fs::open(path, flags, Mode::empty())?;
        Ok(CardKms { file: File::from(fd), mapping: None })
    }

    /// What the opened file is, by `fstat`: its device number and whether it is a character device.
    pub fn node_id(&self) -> io::Result<NodeId> {
        NodeId::of(&self.file)
    }

    fn resources(&self) -> io::Result<(Vec<u32>, Vec<u32>, Vec<u32>)> {
        let mut res = CardRes::default();
        // SAFETY: GETRESOURCES takes `drm_mode_card_res`; every pointer is 0 with count 0.
        unsafe { update::<GET_RESOURCES, _>(&self.file, &mut res) }?;
        let mut crtcs = vec![0u32; bounded(res.count_crtcs, MAX_OBJECTS, "CRTCs")?];
        let mut connectors = vec![0u32; bounded(res.count_connectors, MAX_OBJECTS, "connectors")?];
        let mut encoders = vec![0u32; bounded(res.count_encoders, MAX_OBJECTS, "encoders")?];
        let mut res = CardRes {
            crtc_id_ptr: addr(crtcs.as_mut_ptr()),
            connector_id_ptr: addr(connectors.as_mut_ptr()),
            encoder_id_ptr: addr(encoders.as_mut_ptr()),
            count_crtcs: res.count_crtcs,
            count_connectors: res.count_connectors,
            count_encoders: res.count_encoders,
            ..CardRes::default()
        };
        // SAFETY: each id pointer is a live `u32` vec holding exactly its count; fbs are not asked.
        unsafe { update::<GET_RESOURCES, _>(&self.file, &mut res) }?;
        Ok((
            filled(crtcs, res.count_crtcs, "CRTC list")?,
            filled(connectors, res.count_connectors, "connector list")?,
            filled(encoders, res.count_encoders, "encoder list")?,
        ))
    }

    fn encoder(&self, id: u32) -> io::Result<Encoder> {
        let mut enc = Encoder { encoder_id: id, ..Encoder::default() };
        // SAFETY: GETENCODER takes `drm_mode_get_encoder`, which has no pointers.
        unsafe { update::<GET_ENCODER, _>(&self.file, &mut enc) }?;
        Ok(enc)
    }

    /// A property's name and flags, plus its enum entries when it is an enum.
    fn property(&self, id: u32) -> io::Result<(Vec<u8>, Vec<PropertyEnum>)> {
        let mut prop = Property { prop_id: id, ..Property::default() };
        // SAFETY: GETPROPERTY takes `drm_mode_get_property`; both pointers are 0 with count 0.
        unsafe { update::<GET_PROPERTY, _>(&self.file, &mut prop) }?;
        let name = name_of(&prop.name).to_vec();
        if prop.flags & PROP_ENUM == 0 {
            return Ok((name, Vec::new()));
        }
        let mut values = vec![0u64; bounded(prop.count_values, MAX_ENUMS, "property values")?];
        let mut enums =
            vec![PropertyEnum::default(); bounded(prop.count_enum_blobs, MAX_ENUMS, "enum entries")?];
        let mut prop = Property {
            prop_id: id,
            values_ptr: addr(values.as_mut_ptr()),
            enum_blob_ptr: addr(enums.as_mut_ptr()),
            count_values: prop.count_values,
            count_enum_blobs: prop.count_enum_blobs,
            ..Property::default()
        };
        // SAFETY: `values` holds `count_values` u64s and `enums` `count_enum_blobs` entries.
        unsafe { update::<GET_PROPERTY, _>(&self.file, &mut prop) }?;
        filled(values, prop.count_values, "property values")?;
        Ok((name, filled(enums, prop.count_enum_blobs, "enum entries")?))
    }

    /// `(name, value, enum name)` for each of `ids`/`values`.
    fn properties(&self, ids: &[u32], values: &[u64]) -> io::Result<Vec<NamedProperty>> {
        let mut out = Vec::with_capacity(ids.len());
        for (id, value) in ids.iter().zip(values) {
            let (name, enums) = self.property(*id)?;
            let label = enums
                .iter()
                .find(|e| e.value == *value)
                .map(|e| String::from_utf8_lossy(name_of(&e.name)).into_owned());
            out.push((name, *value, label));
        }
        Ok(out)
    }

    fn connector(&self, id: u32, crtc_ids: &[u32]) -> io::Result<ConnectorInfo> {
        let mut conn = Connector { connector_id: id, ..Connector::default() };
        // SAFETY: GETCONNECTOR takes `drm_mode_get_connector`; all pointers 0 with count 0.
        unsafe { update::<GET_CONNECTOR, _>(&self.file, &mut conn) }?;
        let mut modes = vec![ModeInfo::default(); bounded(conn.count_modes, MAX_MODES, "modes")?];
        let mut props = vec![0u32; bounded(conn.count_props, MAX_PROPS, "properties")?];
        let mut values = vec![0u64; props.len()];
        let mut encoders = vec![0u32; bounded(conn.count_encoders, MAX_OBJECTS, "encoders")?];
        let mut conn = Connector {
            connector_id: id,
            modes_ptr: addr(modes.as_mut_ptr()),
            props_ptr: addr(props.as_mut_ptr()),
            prop_values_ptr: addr(values.as_mut_ptr()),
            encoders_ptr: addr(encoders.as_mut_ptr()),
            count_modes: conn.count_modes,
            count_props: conn.count_props,
            count_encoders: conn.count_encoders,
            ..Connector::default()
        };
        // SAFETY: `modes` holds `count_modes` modeinfos, `props`/`values` `count_props` ids and
        // values, `encoders` `count_encoders` ids; all live and writable for the call.
        unsafe { update::<GET_CONNECTOR, _>(&self.file, &mut conn) }?;
        let modes = filled(modes, conn.count_modes, "mode list")?;
        let props = filled(props, conn.count_props, "property list")?;
        let values = filled(values, conn.count_props, "property values")?;
        let encoders = filled(encoders, conn.count_encoders, "encoder list")?;

        let mut info = ConnectorInfo {
            id,
            kind: conn.connector_type,
            connected: conn.connection == CONNECTED,
            modes,
            ..ConnectorInfo::default()
        };
        for enc in encoders {
            info.possible_crtcs |= self.encoder(enc)?.possible_crtcs;
        }
        if conn.encoder_id != 0 {
            info.bound_crtc = self.encoder(conn.encoder_id)?.crtc_id;
        }
        for (name, value, label) in self.properties(&props, &values)? {
            match name.as_slice() {
                b"non-desktop" => info.non_desktop = Some(value),
                b"panel orientation" => {
                    info.orientation = Some(label.unwrap_or_else(|| format!("<value {value}>")));
                }
                _ => {}
            }
        }
        // `possible_crtcs` bits index the resources' CRTC list; a bit past it is not trusted.
        let known =
            u32::try_from(crtc_ids.len()).ok().and_then(|n| 1u32.checked_shl(n)).map_or(u32::MAX, |b| b - 1);
        info.possible_crtcs &= known;
        Ok(info)
    }

    fn crtc(&self, id: u32) -> io::Result<CrtcInfo> {
        let mut crtc = Crtc { crtc_id: id, ..Crtc::default() };
        // SAFETY: GETCRTC takes `drm_mode_crtc`; `set_connectors_ptr` is 0 and unused by a get.
        unsafe { update::<GET_CRTC, _>(&self.file, &mut crtc) }?;
        Ok(CrtcInfo { id, fb: crtc.fb_id, mode: (crtc.mode_valid != 0).then_some(crtc.mode) })
    }

    fn planes(&self) -> io::Result<Vec<PlaneInfo>> {
        let mut res = PlaneRes::default();
        // SAFETY: GETPLANERESOURCES takes `drm_mode_get_plane_res`; pointer 0 with count 0.
        unsafe { update::<GET_PLANE_RESOURCES, _>(&self.file, &mut res) }?;
        let mut ids = vec![0u32; bounded(res.count_planes, MAX_OBJECTS, "planes")?];
        let mut res = PlaneRes { plane_id_ptr: addr(ids.as_mut_ptr()), count_planes: res.count_planes };
        // SAFETY: `ids` holds `count_planes` u32s, live and writable for the call.
        unsafe { update::<GET_PLANE_RESOURCES, _>(&self.file, &mut res) }?;
        filled(ids, res.count_planes, "plane list")?.into_iter().map(|id| self.plane(id)).collect()
    }

    fn plane(&self, id: u32) -> io::Result<PlaneInfo> {
        let mut plane = Plane { plane_id: id, ..Plane::default() };
        // SAFETY: GETPLANE takes `drm_mode_get_plane`; format pointer 0 with count 0.
        unsafe { update::<GET_PLANE, _>(&self.file, &mut plane) }?;
        let mut formats = vec![0u32; bounded(plane.count_format_types, MAX_FORMATS, "formats")?];
        let mut plane = Plane {
            plane_id: id,
            format_type_ptr: addr(formats.as_mut_ptr()),
            count_format_types: plane.count_format_types,
            ..Plane::default()
        };
        // SAFETY: `formats` holds `count_format_types` u32s, live and writable for the call.
        unsafe { update::<GET_PLANE, _>(&self.file, &mut plane) }?;
        let formats = filled(formats, plane.count_format_types, "format list")?;

        let mut obj = ObjProperties { obj_id: id, obj_type: OBJECT_PLANE, ..ObjProperties::default() };
        // SAFETY: OBJ_GETPROPERTIES takes `drm_mode_obj_get_properties`; pointers 0, count 0.
        unsafe { update::<OBJ_GET_PROPERTIES, _>(&self.file, &mut obj) }?;
        let mut props = vec![0u32; bounded(obj.count_props, MAX_PROPS, "plane properties")?];
        let mut values = vec![0u64; props.len()];
        let mut obj = ObjProperties {
            props_ptr: addr(props.as_mut_ptr()),
            prop_values_ptr: addr(values.as_mut_ptr()),
            count_props: obj.count_props,
            obj_id: id,
            obj_type: OBJECT_PLANE,
        };
        // SAFETY: `props` and `values` each hold `count_props` entries, live and writable.
        unsafe { update::<OBJ_GET_PROPERTIES, _>(&self.file, &mut obj) }?;
        let props = filled(props, obj.count_props, "plane property list")?;
        let values = filled(values, obj.count_props, "plane property values")?;
        let primary = self
            .properties(&props, &values)?
            .iter()
            .any(|(name, _, label)| name == b"type" && label.as_deref() == Some("Primary"));
        Ok(PlaneInfo {
            id,
            primary,
            crtc: plane.crtc_id,
            fb: plane.fb_id,
            possible_crtcs: plane.possible_crtcs,
            formats,
        })
    }
}

impl Kms for CardKms {
    fn driver_name(&mut self) -> io::Result<String> {
        let mut name = [0u8; 64];
        let mut version = Version {
            major: 0,
            minor: 0,
            patchlevel: 0,
            name_len: name.len(),
            name: name.as_mut_ptr(),
            date_len: 0,
            date: null_mut(),
            desc_len: 0,
            desc: null_mut(),
        };
        // SAFETY: VERSION takes `drm_version`; `name` is a live 64-byte buffer and `name_len` says
        // so (the kernel copies at most that), date and desc have length 0.
        unsafe { update::<VERSION, _>(&self.file, &mut version) }?;
        let len = version.name_len.min(name.len());
        let text = String::from_utf8_lossy(&name[..len]).into_owned();
        // A longer name was cut; mark it so it can never equal the expected one.
        Ok(if version.name_len > name.len() { text + "…" } else { text })
    }

    fn set_master(&mut self) -> io::Result<()> {
        // SAFETY: SET_MASTER is `DRM_IO(0x1e)` and takes no argument.
        unsafe { ioctl(&self.file, NoArg::<SET_MASTER>::new()) }.map_err(io::Error::from)
    }

    fn drop_master(&mut self) -> io::Result<()> {
        // SAFETY: DROP_MASTER is `DRM_IO(0x1f)` and takes no argument.
        unsafe { ioctl(&self.file, NoArg::<DROP_MASTER>::new()) }.map_err(io::Error::from)
    }

    fn metadata(&mut self) -> io::Result<KmsMetadata> {
        let cap = ClientCap { capability: CAP_UNIVERSAL_PLANES, value: 1 };
        // SAFETY: SET_CLIENT_CAP is `DRM_IOW(0x0d, struct drm_set_client_cap)`; `Setter` owns the
        // struct, which has no pointers, and the kernel only reads it.
        unsafe { ioctl(&self.file, Setter::<SET_CLIENT_CAP, ClientCap>::new(cap)) }?;
        let (crtc_ids, connector_ids, _) = self.resources()?;
        let connectors =
            connector_ids.iter().map(|id| self.connector(*id, &crtc_ids)).collect::<io::Result<_>>()?;
        let crtcs = crtc_ids.iter().map(|id| self.crtc(*id)).collect::<io::Result<_>>()?;
        Ok(KmsMetadata { connectors, crtcs, planes: self.planes()? })
    }

    fn create_dumb(&mut self, width: u32, height: u32, bpp: u32) -> io::Result<DumbBuffer> {
        let mut dumb = CreateDumb { height, width, bpp, ..CreateDumb::default() };
        // SAFETY: CREATE_DUMB takes `drm_mode_create_dumb`, which has no pointers.
        unsafe { update::<CREATE_DUMB, _>(&self.file, &mut dumb) }?;
        Ok(DumbBuffer { handle: dumb.handle, pitch: dumb.pitch, size: dumb.size })
    }

    fn map_dumb(&mut self, handle: u32, len: usize) -> io::Result<()> {
        if self.mapping.is_some() || len == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "already mapped, or nothing to map"));
        }
        let mut map = MapDumb { handle, ..MapDumb::default() };
        // SAFETY: MAP_DUMB takes `drm_mode_map_dumb`, which has no pointers.
        unsafe { update::<MAP_DUMB, _>(&self.file, &mut map) }?;
        // SAFETY: a fresh shared mapping at an address the kernel picks (null hint, no FIXED), so
        // no existing memory is replaced; `len` is within the buffer's size (checked by the
        // caller) and the offset is MAP_DUMB's fake offset for this file.
        let ptr = unsafe {
            rustix::mm::mmap(
                null_mut(),
                len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                &self.file,
                map.offset,
            )
        }?;
        let ptr = NonNull::new(ptr.cast::<u8>()).ok_or_else(|| io::Error::other("mmap returned null"))?;
        self.mapping = Some(Mapping { ptr, len });
        Ok(())
    }

    fn with_pixels(&mut self, write: &mut dyn FnMut(&mut [u8])) {
        match &self.mapping {
            // SAFETY: `ptr` is a live `len`-byte read-write mapping owned by this value; only
            // `unmap` (which needs `&mut self`) ends it, and `&mut self` here makes this the only
            // slice over it.
            Some(m) => write(unsafe { std::slice::from_raw_parts_mut(m.ptr.as_ptr(), m.len) }),
            None => write(&mut []),
        }
    }

    fn unmap(&mut self) -> io::Result<()> {
        let Some(m) = self.mapping.take() else { return Ok(()) };
        // SAFETY: `m` is the mapping `map_dumb` made, unmapped once (taken out of `self`), and no
        // slice over it outlives `with_pixels`.
        unsafe { rustix::mm::munmap(m.ptr.as_ptr().cast::<c_void>(), m.len) }.map_err(io::Error::from)
    }

    fn destroy_dumb(&mut self, handle: u32) -> io::Result<()> {
        let mut handle = handle;
        // SAFETY: DESTROY_DUMB takes `drm_mode_destroy_dumb`, a single `u32` handle.
        unsafe { update::<DESTROY_DUMB, _>(&self.file, &mut handle) }
    }

    fn add_fb2(&mut self, fb: &FbSpec) -> io::Result<u32> {
        let mut cmd = FbCmd2 {
            width: fb.width,
            height: fb.height,
            pixel_format: fb.format,
            handles: [fb.handle, 0, 0, 0],
            pitches: [fb.pitch, 0, 0, 0],
            ..FbCmd2::default()
        };
        // SAFETY: ADDFB2 takes `drm_mode_fb_cmd2`, which has no pointers.
        unsafe { update::<ADD_FB2, _>(&self.file, &mut cmd) }?;
        Ok(cmd.fb_id)
    }

    fn set_crtc(
        &mut self,
        crtc: u32,
        fb: u32,
        connectors: &[u32],
        mode: Option<&ModeInfo>,
    ) -> io::Result<()> {
        let mut cmd = Crtc {
            set_connectors_ptr: if connectors.is_empty() { 0 } else { addr(connectors.as_ptr()) },
            count_connectors: u32::try_from(connectors.len())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
            crtc_id: crtc,
            fb_id: fb,
            mode_valid: u32::from(mode.is_some()),
            mode: mode.copied().unwrap_or_default(),
            ..Crtc::default()
        };
        // SAFETY: SETCRTC takes `drm_mode_crtc`; `set_connectors_ptr` points to `connectors`,
        // borrowed for the call and holding `count_connectors` ids (the kernel only reads them).
        unsafe { update::<SET_CRTC, _>(&self.file, &mut cmd) }
    }

    fn dirty(&mut self, fb: u32, clips: &[DirtyClip]) -> io::Result<()> {
        let mut cmd = FbDirty {
            fb_id: fb,
            flags: 0,
            color: 0,
            num_clips: u32::try_from(clips.len())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?,
            clips_ptr: addr(clips.as_ptr()),
        };
        // SAFETY: DIRTYFB takes `drm_mode_fb_dirty_cmd`; `clips_ptr` points to `clips`
        // (`DirtyClip` is `drm_clip_rect`, asserted 8 bytes), borrowed for the call, `num_clips` long.
        unsafe { update::<DIRTY_FB, _>(&self.file, &mut cmd) }
    }

    fn rm_fb(&mut self, fb: u32) -> io::Result<()> {
        let mut fb = fb;
        // SAFETY: RMFB is `DRM_IOWR(0xAF, unsigned int)`: the framebuffer id itself.
        unsafe { update::<RM_FB, _>(&self.file, &mut fb) }
    }
}

impl Drop for CardKms {
    fn drop(&mut self) {
        // Nothing to report to: the file closes next, which releases the rest.
        let _ = self.unmap();
    }
}
