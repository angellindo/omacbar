//! The Fn keyboard is the input device whose key capability bitmap has `KEY_FN` (464), found in
//! the text of `/proc/bus/input/devices` (research 0002: by capability, never by event number).
//! Bitmaps are 64-bit hex words, most significant first, leading zero words left out, so bit 464
//! is bit 16 of the eighth word from the right. Every device list here is synthetic text; no test
//! reads the machine's devices.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use tb_daemon::find_fn_keyboard;

/// One device block as `/proc/bus/input/devices` prints it (trailing space after the handlers).
fn device(name: &str, handlers: &str, key: &str) -> String {
    format!(
        "I: Bus=0019 Vendor=0000 Product=0000 Version=0000\nN: Name=\"{name}\"\nH: Handlers={handlers} \nB: PROP=0\nB: EV=120013\nB: KEY={key}\n\n"
    )
}

/// The SPI keyboard's bitmap: Fn in the top word, the usual keyboard keys below.
const SPI_KEY: &str = "10000 0 0 0 1000000000007 ff9f207ac14057ff febeffdfffefffff fffffffffffffffe";
/// The iBridge (Touch Bar) keyboard: function and media keys, no Fn.
const IBRIDGE_KEY: &str = "1006300001000 3800000000 10e00000180001f f800000000000002";

fn power_button() -> String {
    device("Power Button", "kbd event0", "10000000000000 0")
}

fn video_bus() -> String {
    device("Video Bus", "kbd event4", "3e000b00000000 0 0 0")
}

fn ibridge() -> String {
    device("Apple Inc. iBridge", "sysrq kbd leds event15", IBRIDGE_KEY)
}

fn spi_keyboard(event: &str) -> String {
    device("Apple SPI Keyboard", &format!("sysrq kbd leds {event} tbkbd"), SPI_KEY)
}

#[test]
fn the_keyboard_reporting_fn_is_picked_among_the_other_devices() {
    let devices = [power_button(), video_bus(), spi_keyboard("event5"), ibridge()].concat();
    let kb = find_fn_keyboard(&devices).expect("the SPI keyboard reports Fn");
    assert_eq!(kb.name, "Apple SPI Keyboard");
    assert_eq!(kb.path, Path::new("/dev/input/event5"));
}

#[test]
fn the_event_device_comes_from_the_handlers_line() {
    let devices = [power_button(), ibridge(), spi_keyboard("event12")].concat();
    let kb = find_fn_keyboard(&devices).expect("the SPI keyboard reports Fn");
    assert_eq!(kb.name, "Apple SPI Keyboard");
    assert_eq!(kb.path, Path::new("/dev/input/event12"));
}

#[test]
fn no_device_reporting_fn_means_no_fn_keyboard() {
    let devices = [power_button(), video_bus(), ibridge()].concat();
    assert!(find_fn_keyboard(&devices).is_none(), "the iBridge keyboard has no Fn");
    assert!(find_fn_keyboard("").is_none(), "an empty device list");
}

#[test]
fn a_bitmap_is_read_from_its_least_significant_word_on_the_right() {
    // Bit 448 and bit 16: read from the left, the eighth word would look like bit 464.
    let from_left = device("Reversed", "kbd event7", "1 0 0 0 0 0 0 10000");
    // Bit 400 only: seven words, so the word with bit 16 set is the seventh from the right.
    let short = device("Seven words", "kbd event8", "10000 0 0 0 0 0 0");
    assert!(find_fn_keyboard(&[from_left, short].concat()).is_none());

    // Nine words: bit 512 and bit 464 (the eighth word from the right).
    let wide = device("Nine words", "kbd event9", "1 10000 0 0 0 0 0 0 0");
    let kb = find_fn_keyboard(&wide).expect("bit 464 is set");
    assert_eq!(kb.name, "Nine words");
    assert_eq!(kb.path, Path::new("/dev/input/event9"));
}

#[test]
fn a_fn_device_without_an_event_handler_is_skipped() {
    let no_event = device("Fn without events", "sysrq kbd leds", SPI_KEY);
    assert!(find_fn_keyboard(&no_event).is_none());

    let devices = [no_event, spi_keyboard("event5")].concat();
    let kb = find_fn_keyboard(&devices).expect("the next Fn keyboard has an event handler");
    assert_eq!(kb.name, "Apple SPI Keyboard");
    assert_eq!(kb.path, Path::new("/dev/input/event5"));
}

#[test]
fn malformed_devices_are_skipped_and_a_later_fn_keyboard_is_still_found() {
    let malformed = [
        device("Empty bitmap", "kbd event20", ""),
        device("Not hex", "kbd event21", "zz 0 0 0 0 0 0 0"),
        device("Word too wide", "kbd event22", "100000000000000000 0 0 0 0 0 0 0"),
        "N: Name=\"No key bitmap\"\nH: Handlers=kbd event23 \nB: EV=3\n\n".to_owned(),
        "N: Name=\"Cut short\nH: Handlers=kbd event24\nB: KEY=\n".to_owned(),
        "garbage without a prefix\n\n\nB: KEY=10000 0 0 0 0 0 0 0\n\n".to_owned(),
    ]
    .concat();
    assert!(find_fn_keyboard(&malformed).is_none(), "no well-formed device reports Fn");

    let devices = [malformed, spi_keyboard("event5")].concat();
    let kb = find_fn_keyboard(&devices).expect("the SPI keyboard after the malformed devices");
    assert_eq!(kb.name, "Apple SPI Keyboard");
    assert_eq!(kb.path, Path::new("/dev/input/event5"));
}

#[test]
fn of_two_fn_keyboards_the_first_listed_is_picked() {
    let external = device("Apple Inc. Magic Keyboard", "sysrq kbd leds event20", SPI_KEY);

    let kb = find_fn_keyboard(&[spi_keyboard("event5"), external.clone()].concat()).unwrap();
    assert_eq!((kb.name.as_str(), kb.path.as_path()), ("Apple SPI Keyboard", Path::new("/dev/input/event5")));

    let kb = find_fn_keyboard(&[external, spi_keyboard("event5")].concat()).unwrap();
    assert_eq!(
        (kb.name.as_str(), kb.path.as_path()),
        ("Apple Inc. Magic Keyboard", Path::new("/dev/input/event20"))
    );
}
