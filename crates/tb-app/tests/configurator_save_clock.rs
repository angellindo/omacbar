//! The sole new public test seam: a tiny injected save clock, never a fake wall-clock read.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}
use std::sync::atomic::{AtomicU64, Ordering};
use support::configurator::*;

static READS: AtomicU64 = AtomicU64::new(0);
// Match the injected public clock signature while supplying deterministic available times.
#[allow(clippy::unnecessary_wraps)]
fn clock() -> Option<u64> {
    // First explicit save: 01:02:03 UTC; second: 04:05:06 UTC.
    Some(if READS.fetch_add(1, Ordering::SeqCst) == 0 { 3723 } else { 14706 })
}

#[test]
fn only_successful_explicit_saves_sample_the_injected_clock_and_update_the_displayed_utc_time() {
    READS.store(0, Ordering::SeqCst);
    let (mut rt, path) = open_config("save_clock", &fixture(), (1200.0, 760.0));
    rt.state.set_save_clock(clock);
    settle(&mut rt);
    assert_eq!(READS.load(Ordering::SeqCst), 0, "opening and drawing are not user saves");
    assert!(words(&rt, "last-saved").to_lowercase().contains("not saved this session"));
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "saved once");
    operate(&mut rt, "save");
    let saved = words(&rt, "last-saved");
    assert!(saved.contains("01:02:03") && saved.contains("UTC"), "{saved}");
    assert_eq!(READS.load(Ordering::SeqCst), 1, "one event dispatch, one sample");
    edit(&mut rt, "field-width", "invalid");
    save_shortcut(&mut rt);
    assert_eq!(READS.load(Ordering::SeqCst), 1, "blocked saves do not sample");
    assert_eq!(words(&rt, "last-saved"), saved);
    edit(&mut rt, "field-width", "auto");
    operate(&mut rt, "field-label");
    save_shortcut(&mut rt);
    let saved = words(&rt, "last-saved");
    assert!(saved.contains("04:05:06") && saved.contains("UTC"), "{saved}");
    assert_eq!(READS.load(Ordering::SeqCst), 2, "focused shortcut saves once");
    let dir = path.parent().unwrap().to_owned();
    let backup = dir.with_extension("clock-backup");
    if backup.exists() {
        std::fs::remove_dir_all(&backup).unwrap();
    }
    std::fs::rename(&dir, &backup).unwrap();
    std::fs::write(&dir, "not a directory").unwrap();
    edit(&mut rt, "field-label", "cannot save");
    operate(&mut rt, "save");
    std::fs::remove_file(&dir).unwrap();
    std::fs::rename(&backup, &dir).unwrap();
    assert_eq!(READS.load(Ordering::SeqCst), 2, "failed writes never sample the save clock");
    assert_eq!(words(&rt, "last-saved"), saved);
    assert_dirty(&rt, 1);
}
