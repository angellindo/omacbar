//! One followed contact from Linux MT-B reports. Only complete reports can lift a touch.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read};

use crate::evdev::{EV_SYN, Record, SYN_REPORT, read_record};
use crate::touch::{Touch, TouchError, TouchSource};

use super::{Calibration, EV_ABS, MT_SLOT, MT_TRACKING_ID};

const SYN_DROPPED: u16 = 3;
/// `TRKID_MAX` in Linux `include/linux/input/mt.h`.
const MAX_TRACKING_ID: i32 = 0xffff;

#[derive(Debug, Default)]
struct Slot {
    id: Option<i32>,
    position: Option<u16>,
    position_before_id: Option<u16>,
    // A bad sample poisons this landing until an explicit lift or the end of the report.
    invalid: bool,
}

#[derive(Debug)]
struct Contact {
    slot: i32,
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
    slot: Option<i32>,
    slots: HashMap<i32, Slot>,
    contact: Option<Contact>,
    // Only the contact already reported to the daemon may leave an ending here.
    ending: Option<Ending>,
    output: VecDeque<Touch>,
    discarding: bool,
    ended: bool,
    read_error: Option<io::Error>,
}

impl<R: Read> EvdevTouches<R> {
    pub fn new(reader: R, calibration: Calibration) -> Self {
        Self {
            reader,
            calibration,
            slot: None,
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
                self.slot = None;
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
            self.slot = (0..=self.calibration.last_slot).contains(&value).then_some(value);
            if self.slot.is_none() {
                self.slots.clear();
                self.cancel_contact();
            }
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
                        } else {
                            // An idle or unknown slot can supply a position for a landing in this report.
                            state.position = None;
                            state.position_before_id = Some(x);
                        }
                    }
                }
                None => self.invalidate_slot(slot),
            }
        }
    }

    fn tracking_id(&mut self, slot: i32, id: i32) {
        match id {
            -1 => self.lift(slot),
            0..=MAX_TRACKING_ID => self.start_contact(slot, id),
            _ => self.invalidate_slot(slot),
        }
    }

    fn lift(&mut self, slot: i32) {
        let state = self.slots.entry(slot).or_default();
        state.id = None;
        state.position_before_id = None;
        state.invalid = false;
        if let Some(contact) = self.contact.as_ref().filter(|contact| contact.slot == slot) {
            if let Some(reported) = contact.reported {
                let slide = state.position.filter(|x| *x != reported);
                self.ending = Some(Ending::Lift { slide });
            }
            self.contact = None;
        }
    }

    fn start_contact(&mut self, slot: i32, id: i32) {
        let state = self.slots.entry(slot).or_default();
        if state.id == Some(id) {
            return;
        }
        if state.id.replace(id).is_some() {
            // Without an explicit lift the samples up to this ID belong to the old contact.
            state.position = None;
        } else if let Some(x) = state.position_before_id.take() {
            state.position = Some(x);
        }
        let valid = !state.invalid;
        if self.contact.as_ref().is_some_and(|contact| contact.slot == slot) {
            self.cancel_contact();
        }
        if self.contact.is_none() && valid {
            self.contact = Some(Contact { slot, reported: None });
        }
    }

    fn invalidate_slot(&mut self, slot: i32) {
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
