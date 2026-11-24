//! Raw DRM uapi: opcodes and `repr(C)` argument structs copied from
//! `/usr/include/drm/{drm.h,drm_mode.h}` (linux-api-headers 7.3-1). The size and offset asserts
//! fail the build if a layout drifts from the headers. Only `card.rs` passes these to the kernel.

#![allow(dead_code, reason = "unread fields keep each struct's uapi layout")]
#![allow(clippy::struct_field_names, reason = "field names are the headers' own")]

use std::mem::{offset_of, size_of};

use rustix::ioctl::{Opcode, opcode};

use super::{DirtyClip, ModeInfo};

const DRM: u8 = b'd';
pub(super) const VERSION: Opcode = opcode::read_write::<Version>(DRM, 0x00);
pub(super) const SET_CLIENT_CAP: Opcode = opcode::write::<ClientCap>(DRM, 0x0d);
pub(super) const SET_MASTER: Opcode = opcode::none(DRM, 0x1e);
pub(super) const DROP_MASTER: Opcode = opcode::none(DRM, 0x1f);
pub(super) const GET_RESOURCES: Opcode = opcode::read_write::<CardRes>(DRM, 0xA0);
pub(super) const GET_CRTC: Opcode = opcode::read_write::<Crtc>(DRM, 0xA1);
pub(super) const SET_CRTC: Opcode = opcode::read_write::<Crtc>(DRM, 0xA2);
pub(super) const GET_ENCODER: Opcode = opcode::read_write::<Encoder>(DRM, 0xA6);
pub(super) const GET_CONNECTOR: Opcode = opcode::read_write::<Connector>(DRM, 0xA7);
pub(super) const GET_PROPERTY: Opcode = opcode::read_write::<Property>(DRM, 0xAA);
pub(super) const RM_FB: Opcode = opcode::read_write::<u32>(DRM, 0xAF);
pub(super) const DIRTY_FB: Opcode = opcode::read_write::<FbDirty>(DRM, 0xB1);
pub(super) const CREATE_DUMB: Opcode = opcode::read_write::<CreateDumb>(DRM, 0xB2);
pub(super) const MAP_DUMB: Opcode = opcode::read_write::<MapDumb>(DRM, 0xB3);
pub(super) const DESTROY_DUMB: Opcode = opcode::read_write::<u32>(DRM, 0xB4);
pub(super) const GET_PLANE_RESOURCES: Opcode = opcode::read_write::<PlaneRes>(DRM, 0xB5);
pub(super) const GET_PLANE: Opcode = opcode::read_write::<Plane>(DRM, 0xB6);
pub(super) const ADD_FB2: Opcode = opcode::read_write::<FbCmd2>(DRM, 0xB8);
pub(super) const OBJ_GET_PROPERTIES: Opcode = opcode::read_write::<ObjProperties>(DRM, 0xB9);

/// `DRM_CLIENT_CAP_UNIVERSAL_PLANES`: list the primary plane too, so its formats can be read.
pub(super) const CAP_UNIVERSAL_PLANES: u64 = 2;
/// `DRM_MODE_OBJECT_PLANE`.
pub(super) const OBJECT_PLANE: u32 = 0xeeee_eeee;
/// `DRM_MODE_CONNECTED` (`connection` 1).
pub(super) const CONNECTED: u32 = 1;
/// `DRM_MODE_PROP_ENUM`.
pub(super) const PROP_ENUM: u32 = 1 << 3;

/// Upper bounds on what a query may report. The bar card has one of each object, two formats and
/// one mode; anything past these is refused, never allocated.
pub(super) const MAX_OBJECTS: u32 = 32;
pub(super) const MAX_MODES: u32 = 64;
pub(super) const MAX_PROPS: u32 = 64;
pub(super) const MAX_FORMATS: u32 = 256;
pub(super) const MAX_ENUMS: u32 = 64;

/// `struct drm_version`; the name buffer is ours, date and desc are not asked for.
#[repr(C)]
pub(super) struct Version {
    pub(super) major: i32,
    pub(super) minor: i32,
    pub(super) patchlevel: i32,
    pub(super) name_len: usize,
    pub(super) name: *mut u8,
    pub(super) date_len: usize,
    pub(super) date: *mut u8,
    pub(super) desc_len: usize,
    pub(super) desc: *mut u8,
}

/// `struct drm_set_client_cap`.
#[repr(C)]
pub(super) struct ClientCap {
    pub(super) capability: u64,
    pub(super) value: u64,
}

/// `struct drm_mode_card_res`.
#[repr(C)]
#[derive(Default)]
pub(super) struct CardRes {
    pub(super) fb_id_ptr: u64,
    pub(super) crtc_id_ptr: u64,
    pub(super) connector_id_ptr: u64,
    pub(super) encoder_id_ptr: u64,
    pub(super) count_fbs: u32,
    pub(super) count_crtcs: u32,
    pub(super) count_connectors: u32,
    pub(super) count_encoders: u32,
    pub(super) min_width: u32,
    pub(super) max_width: u32,
    pub(super) min_height: u32,
    pub(super) max_height: u32,
}

/// `struct drm_mode_crtc`.
#[repr(C)]
#[derive(Default)]
pub(super) struct Crtc {
    pub(super) set_connectors_ptr: u64,
    pub(super) count_connectors: u32,
    pub(super) crtc_id: u32,
    pub(super) fb_id: u32,
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) gamma_size: u32,
    pub(super) mode_valid: u32,
    pub(super) mode: ModeInfo,
}

/// `struct drm_mode_get_encoder`.
#[repr(C)]
#[derive(Default)]
pub(super) struct Encoder {
    pub(super) encoder_id: u32,
    pub(super) encoder_type: u32,
    pub(super) crtc_id: u32,
    pub(super) possible_crtcs: u32,
    pub(super) possible_clones: u32,
}

/// `struct drm_mode_get_connector`.
#[repr(C)]
#[derive(Default)]
pub(super) struct Connector {
    pub(super) encoders_ptr: u64,
    pub(super) modes_ptr: u64,
    pub(super) props_ptr: u64,
    pub(super) prop_values_ptr: u64,
    pub(super) count_modes: u32,
    pub(super) count_props: u32,
    pub(super) count_encoders: u32,
    pub(super) encoder_id: u32,
    pub(super) connector_id: u32,
    pub(super) connector_type: u32,
    pub(super) connector_type_id: u32,
    pub(super) connection: u32,
    pub(super) mm_width: u32,
    pub(super) mm_height: u32,
    pub(super) subpixel: u32,
    pub(super) pad: u32,
}

/// `struct drm_mode_get_property`.
#[repr(C)]
#[derive(Default)]
pub(super) struct Property {
    pub(super) values_ptr: u64,
    pub(super) enum_blob_ptr: u64,
    pub(super) prop_id: u32,
    pub(super) flags: u32,
    pub(super) name: [u8; 32],
    pub(super) count_values: u32,
    pub(super) count_enum_blobs: u32,
}

/// `struct drm_mode_property_enum`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct PropertyEnum {
    pub(super) value: u64,
    pub(super) name: [u8; 32],
}

/// `struct drm_mode_fb_dirty_cmd`.
#[repr(C)]
pub(super) struct FbDirty {
    pub(super) fb_id: u32,
    pub(super) flags: u32,
    pub(super) color: u32,
    pub(super) num_clips: u32,
    pub(super) clips_ptr: u64,
}

/// `struct drm_mode_create_dumb`.
#[repr(C)]
#[derive(Default)]
pub(super) struct CreateDumb {
    pub(super) height: u32,
    pub(super) width: u32,
    pub(super) bpp: u32,
    pub(super) flags: u32,
    pub(super) handle: u32,
    pub(super) pitch: u32,
    pub(super) size: u64,
}

/// `struct drm_mode_map_dumb`.
#[repr(C)]
#[derive(Default)]
pub(super) struct MapDumb {
    pub(super) handle: u32,
    pub(super) pad: u32,
    pub(super) offset: u64,
}

/// `struct drm_mode_get_plane_res`.
#[repr(C)]
#[derive(Default)]
pub(super) struct PlaneRes {
    pub(super) plane_id_ptr: u64,
    pub(super) count_planes: u32,
}

/// `struct drm_mode_get_plane`.
#[repr(C)]
#[derive(Default)]
pub(super) struct Plane {
    pub(super) plane_id: u32,
    pub(super) crtc_id: u32,
    pub(super) fb_id: u32,
    pub(super) possible_crtcs: u32,
    pub(super) gamma_size: u32,
    pub(super) count_format_types: u32,
    pub(super) format_type_ptr: u64,
}

/// `struct drm_mode_fb_cmd2`.
#[repr(C)]
#[derive(Default)]
pub(super) struct FbCmd2 {
    pub(super) fb_id: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) pixel_format: u32,
    pub(super) flags: u32,
    pub(super) handles: [u32; 4],
    pub(super) pitches: [u32; 4],
    pub(super) offsets: [u32; 4],
    pub(super) modifier: [u64; 4],
}

/// `struct drm_mode_obj_get_properties`.
#[repr(C)]
#[derive(Default)]
pub(super) struct ObjProperties {
    pub(super) props_ptr: u64,
    pub(super) prop_values_ptr: u64,
    pub(super) count_props: u32,
    pub(super) obj_id: u32,
    pub(super) obj_type: u32,
}

// The uapi layouts on x86_64 (the T1 MacBook Pro), from the headers above, cross-checked with
// `sizeof`/`offsetof` and the `DRM_IOCTL_*` values of a C program built against
// linux-api-headers 7.3-1. User pointers travel as `__u64`, so a pointer must fit in 64 bits.
const _: () = assert!(usize::BITS == 64);
const _: () = assert!(size_of::<ModeInfo>() == 68);
const _: () = assert!(offset_of!(ModeInfo, vdisplay) == 14 && offset_of!(ModeInfo, vrefresh) == 24);
const _: () = assert!(offset_of!(ModeInfo, name) == 36);
const _: () = assert!(size_of::<DirtyClip>() == 8);
const _: () = assert!(size_of::<Version>() == 64);
const _: () = assert!(offset_of!(Version, name_len) == 16 && offset_of!(Version, name) == 24);
const _: () = assert!(size_of::<ClientCap>() == 16);
const _: () = assert!(size_of::<CardRes>() == 64 && offset_of!(CardRes, count_fbs) == 32);
const _: () = assert!(size_of::<Crtc>() == 104);
const _: () = assert!(offset_of!(Crtc, mode_valid) == 32 && offset_of!(Crtc, mode) == 36);
const _: () = assert!(size_of::<Encoder>() == 20);
const _: () = assert!(size_of::<Connector>() == 80 && offset_of!(Connector, count_modes) == 32);
const _: () = assert!(offset_of!(Connector, encoder_id) == 44 && offset_of!(Connector, connection) == 60);
const _: () = assert!(size_of::<Property>() == 64);
const _: () = assert!(offset_of!(Property, name) == 24 && offset_of!(Property, count_values) == 56);
const _: () = assert!(size_of::<PropertyEnum>() == 40);
const _: () = assert!(size_of::<FbDirty>() == 24 && offset_of!(FbDirty, clips_ptr) == 16);
const _: () = assert!(size_of::<CreateDumb>() == 32);
const _: () = assert!(offset_of!(CreateDumb, handle) == 16 && offset_of!(CreateDumb, size) == 24);
const _: () = assert!(size_of::<MapDumb>() == 16 && offset_of!(MapDumb, offset) == 8);
const _: () = assert!(size_of::<PlaneRes>() == 16);
const _: () = assert!(size_of::<Plane>() == 32);
const _: () =
    assert!(offset_of!(Plane, count_format_types) == 20 && offset_of!(Plane, format_type_ptr) == 24);
const _: () = assert!(size_of::<FbCmd2>() == 104);
const _: () = assert!(offset_of!(FbCmd2, handles) == 20 && offset_of!(FbCmd2, pitches) == 36);
const _: () = assert!(offset_of!(FbCmd2, offsets) == 52 && offset_of!(FbCmd2, modifier) == 72);
const _: () = assert!(size_of::<ObjProperties>() == 32 && offset_of!(ObjProperties, obj_type) == 24);
// The request numbers, as the C preprocessor expands `DRM_IOCTL_*` on x86_64.
const _: () = assert!(VERSION == 0xc040_6400 && SET_CLIENT_CAP == 0x4010_640d);
const _: () = assert!(SET_MASTER == 0x641e && DROP_MASTER == 0x641f);
const _: () = assert!(GET_RESOURCES == 0xc040_64a0 && SET_CRTC == 0xc068_64a2);
const _: () = assert!(GET_CONNECTOR == 0xc050_64a7 && RM_FB == 0xc004_64af);
const _: () = assert!(DIRTY_FB == 0xc018_64b1 && CREATE_DUMB == 0xc020_64b2);
const _: () = assert!(DESTROY_DUMB == 0xc004_64b4 && ADD_FB2 == 0xc068_64b8);
const _: () = assert!(OBJ_GET_PROPERTIES == 0xc020_64b9);
