#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod configurator;
    pub mod ui;
}

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use rune::Runtime;

use support::configurator::{edit, fixture, fresh_dir, operate, runtime, shown, theme, words};
use tb_app::State;
use tb_config::{Backups, Reason};
use tb_core::config::{Config, to_toml};
use tb_core::{ButtonId, Chord};

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

fn profile(test: &str) -> (Runtime<State>, PathBuf, Backups) {
    let root = fresh_dir("reset-conflict", test);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let config = root.join("config/config.toml");
    fs::create_dir(config.parent().unwrap()).unwrap();
    fs::write(&config, to_toml(&fixture()).unwrap()).unwrap();
    let backups = Backups::at(root.join("state/backups"));
    let state = State::open_protected(Some(config.clone()), Some(backups.clone()), theme());
    (runtime(state, tb_app::WINDOW), config, backups)
}

#[test]
fn cancel_disarms_a_preset_conflict_and_the_next_save_uses_the_original_draft() {
    for (name, dirty) in [("clean", false), ("dirty", true)] {
        let (mut rt, path, backups) = profile(name);
        if dirty {
            operate(&mut rt, "row-a");
            edit(&mut rt, "field-label", "Previous draft");
        }
        let before_preset = rt.state.config.clone();
        let mut external = fixture();
        external.layout.layer_mut(Chord::Default).unwrap().groups[0].buttons[0].label =
            "External edit".to_owned();
        let external_bytes = to_toml(&external).unwrap();
        fs::write(&path, &external_bytes).unwrap();

        operate(&mut rt, "reset-factory");
        operate(&mut rt, "save");
        assert!(shown(&rt, "conflict-overwrite"));
        operate(&mut rt, "reset-cancel");

        assert_eq!(rt.state.config, before_preset);
        assert!(!shown(&rt, "conflict-overwrite"));
        assert_eq!(fs::read_to_string(&path).unwrap(), external_bytes);
        assert_eq!(backups.list(&path).unwrap(), []);

        operate(&mut rt, "save");
        assert_eq!(fs::read_to_string(&path).unwrap(), external_bytes);
        if dirty {
            assert!(shown(&rt, "conflict-overwrite"));
            assert_eq!(rt.state.config, before_preset);
            operate(&mut rt, "conflict-overwrite");
            let ids = backups.list(&path).unwrap();
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0].reason, Reason::Overwrite);
            assert_eq!(fs::read_to_string(&path).unwrap(), to_toml(&before_preset).unwrap());
        } else {
            assert!(!shown(&rt, "conflict-overwrite"));
            assert_eq!(rt.state.config, external);
            assert!(words(&rt, "status").contains("newer version"));
            assert_eq!(backups.list(&path).unwrap(), []);
        }
    }
}

#[test]
fn a_conflict_for_the_previous_draft_survives_the_cancel() {
    let (mut rt, path, backups) = profile("preexisting-conflict");
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "Previous draft");
    let original_draft = rt.state.config.clone();
    let external = format!("# external edit\n{}", to_toml(&fixture()).unwrap());
    fs::write(&path, &external).unwrap();
    operate(&mut rt, "save");
    assert!(shown(&rt, "conflict-overwrite"));

    operate(&mut rt, "reset-factory");
    assert!(!shown(&rt, "conflict-overwrite"));
    operate(&mut rt, "reset-cancel");
    assert!(shown(&rt, "conflict-overwrite"));
    assert_eq!(rt.state.config, original_draft);

    operate(&mut rt, "conflict-overwrite");
    assert_eq!(fs::read_to_string(&path).unwrap(), to_toml(&original_draft).unwrap());
    assert_eq!(fs::read_to_string(path.with_extension("toml.bak")).unwrap(), external);
    assert_eq!(backups.list(&path).unwrap()[0].reason, Reason::Overwrite);
}
