//! Shared `x86_64` evdev record and input capability parsing (ADR-0009).

use std::io::{self, ErrorKind, Read};

pub(crate) const EV_SYN: u16 = 0;
pub(crate) const SYN_REPORT: u16 = 0;

/// A native-endian `input_event` without its 16-byte timestamp.
pub(crate) struct Record {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

/// EOF between records is clean. A partial record is an error.
pub(crate) fn read_record(reader: &mut impl Read) -> io::Result<Option<Record>> {
    let mut record = [0; 24];
    let mut filled = 0;
    while filled < record.len() {
        match reader.read(&mut record[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(n) => filled += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let [.., t0, t1, c0, c1, v0, v1, v2, v3] = record;
    Ok(Some(Record {
        kind: u16::from_ne_bytes([t0, t1]),
        code: u16::from_ne_bytes([c0, c1]),
        value: i32::from_ne_bytes([v0, v1, v2, v3]),
    }))
}

pub(crate) fn is_event_name(name: &str) -> bool {
    name.strip_prefix("event").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Capabilities in `/proc` are 64-bit hex words, most significant first. A malformed bitmap has no bits.
pub(crate) fn has_bit(bitmap: &str, bit: usize) -> bool {
    let words: Option<Vec<u64>> = bitmap
        .split_whitespace()
        .map(|w| if w.len() <= 16 { u64::from_str_radix(w, 16).ok() } else { None })
        .collect();
    let Some(words) = words else { return false };
    words.iter().rev().nth(bit / 64).is_some_and(|word| word >> (bit % 64) & 1 == 1)
}
