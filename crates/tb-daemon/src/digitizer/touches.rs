//! One followed contact from Linux MT-B reports. Only complete reports can lift a touch.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read};

use crate::evdev::{EV_SYN, Record, SYN_REPORT, read_record};
use crate::touch::{Touch, TouchError, TouchSource};

use super::{Calibration, EV_ABS, MT_SLOT, MT_TRACKING_ID};

const SYN_DROPPED: u16 = 3;
/// `TRKID_MAX` in Linux `include/linux/input/mt.h`.
const MAX_TRACKING_ID: i32 = 0xffff;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SlotId {
    Implicit,
    Explicit(i32),
}

#[derive(Debug, Default)]
struct Slot {
    id: Option<i32>,
    known_idle: bool,
    position: Option<u16>,
    position_before_id: Option<u16>,
    // A bad sample poisons this landing until an explicit lift or the end of the report.
    invalid: bool,
}

#[derive(Debug)]
struct Contact {
    slot: SlotId,
    reported: Option<u16>,
}

#[derive(Debug)]
enum Ending {
    Lift { slide: Option<u16> },
    Cancel,
}

/// Reads a single contact at a time, starting with no known slot or position.
#[derive(Debug)]
pub struct EvdevTouches<R: Read> {
    reader: R,
    calibration: Calibration,
    kernel_stream: bool,
    slot: Option<SlotId>,
    slots: HashMap<SlotId, Slot>,
    contact: Option<Contact>,
    // Only the contact already reported to the daemon may leave an ending here.
    ending: Option<Ending>,
    output: VecDeque<Touch>,
    discarding: bool,
    ended: bool,
    read_error: Option<io::Error>,
}

impl<R: Read> EvdevTouches<R> {
    /// Replays raw records, waiting for a selector at startup and after lost sync.
    pub fn new(reader: R, calibration: Calibration) -> Self {
        Self::with_stream(reader, calibration, false)
    }

    /// Reads unfiltered Linux evdev records after taking the grab and resetting any pending queue.
    /// The stream must include Linux's slot changes and suppress unchanged tracking IDs.
    pub fn from_grabbed(reader: R, calibration: Calibration) -> Self {
        Self::with_stream(reader, calibration, true)
    }

    fn with_stream(reader: R, calibration: Calibration, kernel_stream: bool) -> Self {
        Self {
            reader,
            calibration,
            kernel_stream,
            slot: kernel_stream.then_some(SlotId::Implicit),
            slots: HashMap::new(),
            contact: None,
            ending: None,
            output: VecDeque::new(),
            discarding: false,
            ended: false,
            read_error: None,
        }
    }

    fn record(&mut self, Record { kind, code, value }: Record) {
        match (kind, code) {
            (EV_SYN, SYN_DROPPED) => {
                self.discarding = true;
                self.slot = self.kernel_stream.then_some(SlotId::Implicit);
                self.slots.clear();
                if self.forget_touch() {
                    self.output.push_back(Touch::Cancel);
                }
            }
            (EV_SYN, SYN_REPORT) if self.discarding => self.discarding = false,
            (EV_SYN, SYN_REPORT) => self.report(),
            (EV_ABS, _) if !self.discarding => self.absolute(code, value),
            _ => {}
        }
    }

    fn forget_touch(&mut self) -> bool {
        let followed = self.contact.take().is_some();
        let ending = self.ending.take().is_some();
        followed || ending
    }

    fn absolute(&mut self, code: u16, value: i32) {
        if code == MT_SLOT {
            self.select_slot(value);
            return;
        }
        let Some(slot) = self.slot else { return };
        if code == MT_TRACKING_ID {
            self.tracking_id(slot, value);
        } else if code == self.calibration.axis.code {
            match self.calibration.column(value) {
                Some(x) => {
                    let state = self.slots.entry(slot).or_default();
                    if !state.invalid {
                        if state.id.is_some() {
                            state.position = Some(x);
                        } else if !self.kernel_stream || state.known_idle {
                            // In a live stream an unknown slot could still hold an older contact.
                            state.position = None;
                            state.position_before_id = Some(x);
                        }
                    }
                }
                None => self.invalidate_slot(slot),
            }
        }
    }

    fn select_slot(&mut self, value: i32) {
        if !(0..=self.calibration.last_slot).contains(&value) {
            self.slot = None;
            self.slots.clear();
            self.cancel_contact();
            return;
        }
        let next = SlotId::Explicit(value);
        match self.slot {
            // The first selector names a different slot. With two slots it identifies both.
            Some(SlotId::Implicit) if self.calibration.last_slot == 1 => {
                let previous = SlotId::Explicit(1 - value);
                if let Some(state) = self.slots.remove(&SlotId::Implicit) {
                    self.slots.insert(previous, state);
                }
                if let Some(contact) = &mut self.contact {
                    contact.slot = previous;
                }
            }
            // With more slots, updates on this first explicit slot still cannot affect the
            // implicit contact. The next switch could return to it, so cancel at that point.
            Some(SlotId::Implicit) if self.calibration.last_slot > 1 => {}
            Some(current) if current != next => {
                self.slots.remove(&SlotId::Implicit);
                if self.contact.as_ref().is_some_and(|contact| contact.slot == SlotId::Implicit) {
                    self.cancel_contact();
                }
            }
            _ => {}
        }
        self.slot = Some(next);
    }

    fn tracking_id(&mut self, slot: SlotId, id: i32) {
        match id {
            -1 => self.lift(slot),
            0..=MAX_TRACKING_ID => self.start_contact(slot, id),
            _ => {
                self.slots.entry(slot).or_default().known_idle = false;
                self.invalidate_slot(slot);
            }
        }
    }

    fn lift(&mut self, slot: SlotId) {
        let state = self.slots.entry(slot).or_default();
        let position = state.position;
        if self.kernel_stream {
            state.position = None;
        }
        state.id = None;
        state.known_idle = true;
        state.position_before_id = None;
        state.invalid = false;
        if let Some(contact) = self.contact.as_ref().filter(|contact| contact.slot == slot) {
            if let Some(reported) = contact.reported {
                let slide = position.filter(|x| *x != reported);
                self.ending = Some(Ending::Lift { slide });
            }
            self.contact = None;
        }
    }

    fn start_contact(&mut self, slot: SlotId, id: i32) {
        let state = self.slots.entry(slot).or_default();
        if state.id == Some(id) {
            return;
        }
        let before_id = state.position_before_id.take();
        if state.id.replace(id).is_some() {
            // Replacements need their own sample even if the old contact moved in this report.
            state.position = None;
        } else if let Some(x) = before_id {
            state.position = Some(x);
        }
        state.known_idle = false;
        let valid = !state.invalid;
        if self.contact.as_ref().is_some_and(|contact| contact.slot == slot) {
            self.cancel_contact();
        }
        if self.contact.is_none() && valid {
            self.contact = Some(Contact { slot, reported: None });
        }
    }

    fn invalidate_slot(&mut self, slot: SlotId) {
        let state = self.slots.entry(slot).or_default();
        state.position = None;
        state.position_before_id = None;
        state.invalid = true;
        if self.contact.as_ref().is_some_and(|contact| contact.slot == slot) {
            self.cancel_contact();
        }
    }

    fn cancel_contact(&mut self) {
        if self.contact.take().is_some_and(|contact| contact.reported.is_some()) {
            self.ending = Some(Ending::Cancel);
        }
    }

    fn report(&mut self) {
        for state in self.slots.values_mut() {
            state.position_before_id = None;
            state.invalid = false;
        }
        match self.ending.take() {
            Some(Ending::Lift { slide }) => {
                if let Some(x) = slide {
                    self.output.push_back(Touch::Down { x });
                }
                self.output.push_back(Touch::Up);
            }
            Some(Ending::Cancel) => self.output.push_back(Touch::Cancel),
            None => {}
        }
        if let Some(contact) = &mut self.contact
            && let Some(x) = self.slots.get(&contact.slot).and_then(|slot| slot.position)
            && contact.reported != Some(x)
        {
            contact.reported = Some(x);
            self.output.push_back(Touch::Down { x });
        }
    }
}

impl<R: Read> TouchSource for EvdevTouches<R> {
    fn next_touch(&mut self) -> Result<Option<Touch>, TouchError> {
        loop {
            if let Some(touch) = self.output.pop_front() {
                return Ok(Some(touch));
            }
            if self.ended {
                return self.read_error.take().map_or(Ok(None), |error| Err(error.into()));
            }
            match read_record(&mut self.reader) {
                Ok(Some(record)) => self.record(record),
                end => {
                    self.ended = true;
                    self.read_error = end.err();
                    if self.forget_touch() {
                        return Ok(Some(Touch::Cancel));
                    }
                }
            }
        }
    }
}
