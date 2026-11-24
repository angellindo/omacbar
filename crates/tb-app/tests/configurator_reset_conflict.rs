#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod configurator;
    pub mod ui;
}

use std::fs;

use support::configurator::{edit, fixture, fresh_dir, operate, runtime, shown, theme, words};
use tb_app::State;
use tb_config::Backups;
use tb_core::ButtonId;
use tb_core::config::{Config, to_toml};

#[test]
fn a_conflict_keeps_cancel_when_the_pending_factory_layout_matches_the_baseline() {
    let profile = fresh_dir("configurator_reset_conflict", "factory_baseline");
    let config = profile.join("config.toml");
    let store = profile.join("state/backups");
    let factory = Config::default_for(&theme());
    fs::write(&config, to_toml(&factory).unwrap()).unwrap();
    let state = State::open_protected(Some(config.clone()), Some(Backups::at(&store)), theme());
    let mut rt = runtime(state, (1200.0, 760.0));
    assert_eq!(rt.state.config, factory);

    operate(&mut rt, "row-f1");
    edit(&mut rt, "field-width", "bad-width");
    operate(&mut rt, "row-f2");
    edit(&mut rt, "field-label", "pending label");
    let draft = rt.state.config.clone();
    assert_ne!(draft, factory);
    assert!(shown(&rt, "save-blocked-reason"));

    operate(&mut rt, "reset-factory");
    assert_eq!(rt.state.config, factory);
    let external = to_toml(&fixture()).unwrap();
    fs::write(&config, &external).unwrap();
    operate(&mut rt, "save");

    assert!(shown(&rt, "reset-cancel"), "a refused reset save stays cancellable");
    assert_eq!(rt.state.config, factory);
    assert!(shown(&rt, "conflict-reload"));
    assert!(shown(&rt, "conflict-overwrite"));

    operate(&mut rt, "reset-cancel");
    assert_eq!(rt.state.config, draft);
    assert_eq!(rt.state.selected, Some(ButtonId::new("f2")));
    assert_eq!(words(&rt, "field-label"), "pending label");
    operate(&mut rt, "row-f1");
    assert_eq!(words(&rt, "field-width"), "bad-width");
    assert_eq!(fs::read_to_string(&config).unwrap(), external);
    assert!(!store.exists(), "the refused save keeps no generation");
}
