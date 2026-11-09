use super::*;

fn parse(args: &[&str]) -> Option<Args> {
    parse_args(args.iter().map(|a| (*a).to_owned()))
}

fn sim(dir: &str, keys: Option<Keys>) -> Args {
    Args { sim_dir: PathBuf::from(dir), keys, window: false }
}

fn windowed(args: Args) -> Args {
    Args { window: true, ..args }
}

#[test]
fn no_arguments_use_the_default_simulator_directory_and_read_no_keyboard() {
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
