//! The simulator display backend: a present shows only its damage on the simulated panel and
//! writes the whole screen as the next PNG of a `frame-NNNN.png` sequence. Frames are built by
//! hand (no GPU); rects are arbitrary bar pixels on the 2170 x 60 bar.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::File;
use std::path::{Path, PathBuf};

use tb_core::{BarSize, Rect};
use tb_daemon::{Display, DisplayError, SimDisplay};
use tb_render::Frame;

const BAR: BarSize = BarSize::MACBOOK_PRO;
const WHOLE_BAR: Rect = Rect { x: 0, y: 0, width: 2170, height: 60 };
const BLACK: [u8; 4] = [0, 0, 0, 255];
const RED: [u8; 4] = [200, 30, 40, 255];
const BLUE: [u8; 4] = [20, 60, 220, 255];

/// A fresh, empty directory for one test (tests run in parallel).
fn fresh_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sim_display").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Frame {
    Frame { width, height, rgba: rgba.repeat((width * height) as usize) }
}

fn bar_frame(rgba: [u8; 4]) -> Frame {
    solid(u32::from(BAR.width), u32::from(BAR.height), rgba)
}

fn inside(rect: Rect, x: u32, y: u32) -> bool {
    let (rx, ry) = (u32::from(rect.x), u32::from(rect.y));
    x >= rx && x < rx + u32::from(rect.width) && y >= ry && y < ry + u32::from(rect.height)
}

/// Every pixel of `screen` is `in_damage` inside one of `damage` and `elsewhere` outside all.
fn assert_screen(screen: &Frame, damage: &[Rect], in_damage: [u8; 4], elsewhere: [u8; 4]) {
    assert_eq!((screen.width, screen.height), (2170, 60), "screen size");
    for y in 0..screen.height {
        for x in 0..screen.width {
            let want = if damage.iter().any(|&r| inside(r, x, y)) { in_damage } else { elsewhere };
            assert_eq!(screen.pixel(x, y), want, "pixel ({x}, {y})");
        }
    }
}

/// The `frame-*.png` file names in `dir`, sorted.
fn frame_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("frame-") && Path::new(n).extension().is_some_and(|e| e == "png"))
        .collect();
    names.sort();
    names
}

fn decode_png(path: &Path) -> Frame {
    let decoder = png::Decoder::new(File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba, "{}", path.display());
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "{}", path.display());
    buf.truncate(info.buffer_size());
    Frame { width: info.width, height: info.height, rgba: buf }
}

#[test]
fn the_simulated_screen_starts_opaque_black() {
    let sim = SimDisplay::new(BAR, fresh_dir("starts_black")).unwrap();
    assert_screen(sim.screen(), &[], BLACK, BLACK);
}

#[test]
fn a_first_present_with_partial_damage_leaves_the_rest_of_the_screen_black() {
    let mut sim = SimDisplay::new(BAR, fresh_dir("first_partial")).unwrap();
    let damage = [Rect { x: 300, y: 8, width: 160, height: 44 }];
    sim.present(&bar_frame(RED), &damage).unwrap();
    assert_screen(sim.screen(), &damage, RED, BLACK);
}

#[test]
fn a_present_shows_the_new_frame_only_inside_its_damage_rects() {
    let mut sim = SimDisplay::new(BAR, fresh_dir("partial_apply")).unwrap();
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    assert_screen(sim.screen(), &[WHOLE_BAR], RED, RED);

    let damage = [
        Rect { x: 826, y: 8, width: 160, height: 44 },
        Rect { x: 0, y: 0, width: 1, height: 1 },
        Rect { x: 2010, y: 0, width: 160, height: 60 },
    ];
    sim.present(&bar_frame(BLUE), &damage).unwrap();
    assert_screen(sim.screen(), &damage, BLUE, RED);
}

#[test]
fn each_present_with_damage_writes_the_whole_screen_as_the_next_numbered_png() {
    let dir = fresh_dir("png_sequence").join("nested");
    let mut sim = SimDisplay::new(BAR, &dir).unwrap();
    assert!(dir.is_dir(), "new creates the directory");
    assert_eq!(frame_files(&dir), Vec::<String>::new(), "nothing presented yet");

    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    let first = sim.screen().clone();
    let damage = [Rect { x: 826, y: 8, width: 160, height: 44 }];
    sim.present(&bar_frame(BLUE), &damage).unwrap();

    assert_eq!(frame_files(&dir), ["frame-0001.png", "frame-0002.png"]);
    assert_eq!(decode_png(&dir.join("frame-0001.png")), first);
    let second = decode_png(&dir.join("frame-0002.png"));
    assert_eq!(&second, sim.screen());
    assert_screen(&second, &damage, BLUE, RED);
}

#[test]
fn a_present_with_empty_damage_shows_nothing_and_writes_no_png() {
    let dir = fresh_dir("empty_damage");
    let mut sim = SimDisplay::new(BAR, &dir).unwrap();
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();

    sim.present(&bar_frame(BLUE), &[]).unwrap();
    assert_screen(sim.screen(), &[], RED, RED);
    assert_eq!(frame_files(&dir), ["frame-0001.png"]);

    sim.present(&bar_frame(BLUE), &[WHOLE_BAR]).unwrap();
    assert_eq!(frame_files(&dir), ["frame-0001.png", "frame-0002.png"], "numbering skips nothing");
}

#[test]
fn damage_reaching_past_the_right_edge_is_refused_and_nothing_is_shown() {
    let dir = fresh_dir("past_right_edge");
    let mut sim = SimDisplay::new(BAR, &dir).unwrap();
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();

    let past = Rect { x: 2100, y: 0, width: 71, height: 60 };
    let valid = Rect { x: 10, y: 8, width: 100, height: 44 };
    let err = sim.present(&bar_frame(BLUE), &[valid, past]).unwrap_err();

    match err {
        DisplayError::DamageOutsideBar { rect } => assert_eq!(rect, past),
        other => panic!("expected DamageOutsideBar, got {other:?}"),
    }
    assert_screen(sim.screen(), &[], RED, RED);
    assert_eq!(frame_files(&dir), ["frame-0001.png"]);
}

#[test]
fn damage_reaching_past_the_bottom_edge_is_refused() {
    let mut sim = SimDisplay::new(BAR, fresh_dir("past_bottom_edge")).unwrap();
    let past = Rect { x: 0, y: 50, width: 10, height: 11 };
    let err = sim.present(&bar_frame(BLUE), &[past]).unwrap_err();
    match err {
        DisplayError::DamageOutsideBar { rect } => assert_eq!(rect, past),
        other => panic!("expected DamageOutsideBar, got {other:?}"),
    }
    assert_screen(sim.screen(), &[], BLACK, BLACK);
}

#[test]
fn damage_ending_exactly_at_the_bar_edges_is_shown() {
    let mut sim = SimDisplay::new(BAR, fresh_dir("at_the_edges")).unwrap();
    let corner = [Rect { x: 2070, y: 30, width: 100, height: 30 }];
    sim.present(&bar_frame(BLUE), &corner).unwrap();
    assert_screen(sim.screen(), &corner, BLUE, BLACK);
}

#[test]
fn a_frame_that_is_not_the_bar_size_is_refused_and_nothing_is_shown() {
    let dir = fresh_dir("frame_size");
    let mut sim = SimDisplay::new(BAR, &dir).unwrap();
    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();

    let small = solid(100, 60, BLUE);
    let err = sim.present(&small, &[Rect { x: 0, y: 0, width: 100, height: 60 }]).unwrap_err();

    assert!(matches!(err, DisplayError::FrameSize { .. }), "got {err:?}");
    assert_screen(sim.screen(), &[], RED, RED);
    assert_eq!(frame_files(&dir), ["frame-0001.png"]);
}

#[test]
fn a_new_simulator_removes_earlier_frames_and_numbers_from_one() {
    let dir = fresh_dir("old_frames");
    std::fs::create_dir_all(&dir).unwrap();
    for old in ["frame-0001.png", "frame-0002.png", "frame-0017.png"] {
        std::fs::write(dir.join(old), b"stale").unwrap();
    }
    std::fs::write(dir.join("notes.txt"), b"keep me").unwrap();

    let mut sim = SimDisplay::new(BAR, &dir).unwrap();
    assert_eq!(frame_files(&dir), Vec::<String>::new(), "earlier frames are gone");
    assert_eq!(std::fs::read(dir.join("notes.txt")).unwrap(), b"keep me", "other files stay");

    sim.present(&bar_frame(RED), &[WHOLE_BAR]).unwrap();
    assert_eq!(frame_files(&dir), ["frame-0001.png"]);
    assert_eq!(&decode_png(&dir.join("frame-0001.png")), sim.screen());
}
