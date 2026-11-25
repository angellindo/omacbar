//! `touchbard`'s flags. The six simulator cases are the existing ones; only their `sim` helper now
//! takes every other field from the defaults, so a field the device flags add does not break them,
//! and the defaults they used to spell out are asserted explicitly. The opt-in device run is in
//! `hardware_tests.rs`.

use super::*;

fn parse(args: &[&str]) -> Option<Args> {
    parse_args(args.iter().map(|a| (*a).to_owned()))
}

/// The default arguments with `sim_dir` and `keys` set; every other field keeps its default.
fn sim(dir: &str, keys: Option<Keys>) -> Args {
    Args { sim_dir: PathBuf::from(dir), keys, ..parse(&[]).unwrap() }
}

fn windowed(args: Args) -> Args {
    Args { window: true, ..args }
}

#[test]
fn no_arguments_use_the_default_simulator_directory_and_read_no_keyboard() {
    let default = parse(&[]).unwrap();
    assert_eq!(default.sim_dir, PathBuf::from("out/touchbard"));
    assert_eq!(default.keys, None);
    assert_eq!(default.emit, None);
    assert!(!default.window);
    assert_eq!(parse(&[]), Some(sim("out/touchbard", None)));
}

#[test]
fn sim_takes_exactly_one_directory() {
    assert_eq!(parse(&["--sim", "x/y"]), Some(sim("x/y", None)));
    assert_eq!(parse(&["--sim"]), None);
    assert_eq!(parse(&["--sim", "x", "extra"]), None);
    assert_eq!(parse(&["--sim", "x", "--sim", "y"]), None);
    assert_eq!(parse(&["x"]), None);
}

#[test]
fn keys_is_auto_or_a_path_in_either_order_with_sim() {
    assert_eq!(parse(&["--keys", "auto"]), Some(sim("out/touchbard", Some(Keys::Auto))));
    assert_eq!(
        parse(&["--keys", "k.bin", "--sim", "s"]),
        Some(sim("s", Some(Keys::Path(PathBuf::from("k.bin")))))
    );
    assert_eq!(parse(&["--sim", "s", "--keys", "auto"]), Some(sim("s", Some(Keys::Auto))));
    assert_eq!(parse(&["--keys", "auto", "--keys", "auto"]), None);
}

#[test]
fn sim_window_takes_no_value_and_combines_with_sim_and_keys_in_any_order() {
    assert_eq!(parse(&["--sim-window"]), Some(windowed(sim("out/touchbard", None))));
    assert_eq!(
        parse(&["--sim", "s", "--sim-window", "--keys", "auto"]),
        Some(windowed(sim("s", Some(Keys::Auto))))
    );
    assert_eq!(
        parse(&["--keys", "auto", "--sim-window"]),
        Some(windowed(sim("out/touchbard", Some(Keys::Auto))))
    );
    assert_eq!(parse(&["--sim-window", "--sim-window"]), None);
    assert_eq!(parse(&["--sim-window", "extra"]), None);
}

fn emitting(args: Args, emit: Emit) -> Args {
    Args { emit: Some(emit), ..args }
}

#[test]
fn emit_is_uinput_or_a_path_in_any_order_with_the_other_flags() {
    assert_eq!(parse(&["--emit", "uinput"]), Some(emitting(sim("out/touchbard", None), Emit::Uinput)));
    assert_eq!(
        parse(&["--keys", "auto", "--emit", "k.bin", "--sim", "s"]),
        Some(emitting(sim("s", Some(Keys::Auto)), Emit::Path(PathBuf::from("k.bin"))))
    );
    assert_eq!(
        parse(&["--sim-window", "--emit", "uinput"]),
        Some(emitting(windowed(sim("out/touchbard", None)), Emit::Uinput))
    );
}

#[test]
fn emit_takes_exactly_one_value_once() {
    assert_eq!(parse(&["--emit"]), None);
    assert_eq!(parse(&["--emit", "a", "--emit", "b"]), None);
    assert_eq!(parse(&["--emit", "uinput", "--emit", "uinput"]), None);
}

// ---------- the opt-in device flags ----------

/// Which device each flag selects is observed through the run (`hardware_tests.rs`: the path the
/// card or digitizer opener is asked for); here only that they parse, are told apart, and when
/// they are usage errors.
#[test]
fn drm_and_touch_are_opt_in_flags_given_at_most_once_and_never_with_the_simulator_window() {
    let default = parse(&[]).unwrap();
    let selected: Vec<Args> = [
        ["--drm", "auto"],
        ["--drm", "/dev/dri/card1"],
        ["--touch", "auto"],
        ["--touch", "/dev/input/event7"],
    ]
    .iter()
    .map(|flags| parse(flags).unwrap_or_else(|| panic!("{flags:?} must parse")))
    .collect();
    for (i, args) in selected.iter().enumerate() {
        assert_ne!(*args, default, "row {i} selects a device");
        for other in &selected[i + 1..] {
            assert_ne!(args, other, "each flag and value is kept apart");
        }
    }
    assert!(parse(&["--emit", "k.bin", "--drm", "auto", "--keys", "auto", "--touch", "auto"]).is_some());
    assert!(parse(&["--touch", "/dev/input/event7", "--drm", "/dev/dri/card1"]).is_some());
    assert!(parse(&["--touch", "auto", "--sim", "s"]).is_some(), "touches may drive the simulator");

    let refused: [&[&str]; 9] = [
        &["--drm"],
        &["--touch"],
        &["--drm", "auto", "--drm", "auto"],
        &["--touch", "auto", "--touch", "/dev/input/event1"],
        &["--drm", "auto", "--sim", "s"],
        &["--sim", "s", "--drm", "/dev/dri/card1"],
        &["--drm", "auto", "--sim-window"],
        &["--sim-window", "--touch", "auto"],
        &["--touch", "auto", "--drm", "auto", "--sim-window"],
    ];
    for flags in refused {
        assert_eq!(parse(flags), None, "{flags:?} is a usage error");
    }
}

#[path = "hardware_tests.rs"]
mod hardware;
