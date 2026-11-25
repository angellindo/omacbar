//! One followed contact from Linux MT-B reports. Only complete reports can lift a touch.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read};

use crate::evdev::{EV_SYN, Record, SYN_REPORT, read_record};
use crate::touch::{Touch, TouchError, TouchSource};

use super::{Calibration, EV_ABS, MT_SLOT, MT_TRACKING_ID};

const SYN_DROPPED: u16 = 3;

#[derive(Debug)]
struct Contact {
    slot: i32,
    id: i32,
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
    positions: HashMap<i32, u16>,
    contact: Option<Contact>,
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
            positions: HashMap::new(),
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
                self.positions.clear();
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
            self.slot = (value >= 0).then_some(value);
            return;
        }
        let Some(slot) = self.slot else { return };
        if code == MT_TRACKING_ID {
            self.tracking_id(slot, value);
        } else if code == self.calibration.axis.code {
            if let Some(x) = self.calibration.column(value) {
                self.positions.insert(slot, x);
            } else {
                self.positions.remove(&slot);
                if self.contact.as_ref().is_some_and(|contact| contact.slot == slot) {
                    self.contact = None;
                    self.ending = Some(Ending::Cancel);
                }
            }
        }
    }

    fn tracking_id(&mut self, slot: i32, id: i32) {
        if let Some(contact) = &self.contact {
            if contact.slot != slot || contact.id == id {
                return;
            }
            if id == -1 {
                if let Some(reported) = contact.reported {
                    let slide = self.positions.get(&slot).copied().filter(|x| *x != reported);
                    self.ending = Some(Ending::Lift { slide });
                }
                self.contact = None;
            } else if id >= 0 {
                self.ending = Some(Ending::Cancel);
                self.contact = None;
            }
        }
        if self.contact.is_none() && id >= 0 {
            self.contact = Some(Contact { slot, id, reported: None });
        }
    }

    fn report(&mut self) {
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
            && let Some(&x) = self.positions.get(&contact.slot)
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
