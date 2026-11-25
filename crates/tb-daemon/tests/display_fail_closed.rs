#![allow(clippy::unwrap_used, clippy::panic)]

use std::io;
use std::sync::Arc;

use tb_core::{BarSize, BarTheme, Chord, Key, Rect, SpacingRules, place, presets};
use tb_daemon::fake::{
    FlatRenderer, KmsCall, RecordingDisplay, RecordingEmitter, RecordingKms, RecordingSpawner,
};
use tb_daemon::{Actions, Daemon, DirtyClip, Display, DisplayError, DrmDisplay, Event, Touch};
use tb_render::Frame;

const SIZE: BarSize = BarSize::MACBOOK_PRO;
const ZERO_WIDTH: Rect = Rect { x: 0, y: 0, width: 0, height: 10 };
const ZERO_HEIGHT: Rect = Rect { x: 10, y: 59, width: 2, height: 0 };
const PIXEL: Rect = Rect { x: 100, y: 10, width: 1, height: 1 };

#[test]
fn zero_area_damage_never_modesets() {
    let kms = RecordingKms::touch_bar();
    let log = kms.log();
    let mut display = DrmDisplay::start(kms, SIZE).unwrap();
    let calls = log.calls();
    let bytes = log.pixels();
    display.present(&solid([0, 0, 0, 255]), &[ZERO_WIDTH, ZERO_HEIGHT]).unwrap();
    assert_eq!(log.calls(), calls);
    assert_eq!(log.pixels(), bytes);
}

#[test]
fn zero_area_damage_never_sends_dirty_clips() {
    let kms = RecordingKms::touch_bar();
    let log = kms.log();
    let mut display = DrmDisplay::start(kms, SIZE).unwrap();
    display.present(&solid([0, 0, 0, 255]), &[PIXEL]).unwrap();
    let calls = log.calls();
    let bytes = log.pixels();
    display.present(&solid([255, 255, 255, 255]), &[ZERO_WIDTH, ZERO_HEIGHT]).unwrap();
    assert_eq!(log.calls(), calls);
    assert_eq!(log.pixels(), bytes);
}

#[test]
fn zero_area_rects_do_not_count_toward_the_dirty_clip_limit() {
    let kms = RecordingKms::touch_bar();
    let log = kms.log();
    let mut display = DrmDisplay::start(kms, SIZE).unwrap();
    let black = solid([0, 0, 0, 255]);
    display.present(&black, &[PIXEL]).unwrap();

    let mut mixed = vec![ZERO_WIDTH; 256];
    mixed.push(PIXEL);
    display.present(&black, &mixed).unwrap();
    match log.calls().last().unwrap() {
        KmsCall::Dirty { clips, .. } => {
            assert_eq!(clips, &[DirtyClip { x1: 49, y1: 100, x2: 50, y2: 101 }]);
        }
        call => panic!("expected one dirty clip for the changed pixel, got {call:?}"),
    }
}

fn solid(rgba: [u8; 4]) -> Frame {
    Frame {
        width: u32::from(SIZE.width),
        height: u32::from(SIZE.height),
        rgba: rgba.repeat(usize::from(SIZE.width) * usize::from(SIZE.height)),
    }
}

struct RejectFirstPress {
    display: RecordingDisplay,
    rejected: bool,
}

impl Display for RejectFirstPress {
    fn present(&mut self, frame: &Frame, damage: &[Rect]) -> Result<(), DisplayError> {
        if !self.rejected && !self.display.presents().is_empty() {
            self.rejected = true;
            return Err(DisplayError::Device { source: io::Error::other("press refused"), gone: false });
        }
        self.display.present(frame, damage)
    }
}

#[test]
fn a_lift_after_a_refused_press_does_not_run_the_action() {
    let layout = presets::default_layout();
    let placed = place(layout.layer(Chord::Default).unwrap(), SIZE, &SpacingRules::STANDARD).unwrap();
    let f5 = placed.iter().find(|button| button.id.as_str() == "f5").unwrap().rect;
    let x = f5.x + f5.width / 2;
    let display = RejectFirstPress { display: RecordingDisplay::new(), rejected: false };
    let emitter = RecordingEmitter::new();
    let actions = Actions::new(
        Some(Box::new(emitter.clone())),
        Box::new(RecordingSpawner::new()),
        Arc::new(|err| panic!("{err}")),
    );
    let mut daemon =
        Daemon::new(layout, BarTheme::omarchy_fallback(), FlatRenderer, display).with_actions(actions);
    daemon.run([]).unwrap();
    assert!(daemon.handle(Event::Touch(Touch::Down { x })).is_err());
    // The source can send more Down events as the same finger moves before it lifts.
    daemon.handle(Event::Touch(Touch::Down { x: x + 1 })).unwrap();
    daemon.handle(Event::Touch(Touch::Up)).unwrap();
    let keys = emitter.emitted();
    assert!(keys.is_empty(), "the rejected touch emitted {keys:?}");
    assert_eq!(daemon.display().display.presents().len(), 1);

    daemon.handle(Event::Touch(Touch::Down { x })).unwrap();
    daemon.handle(Event::Touch(Touch::Up)).unwrap();
    assert_eq!(emitter.emitted(), [vec![Key::named("F5").unwrap()]]);
}
