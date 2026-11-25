//! A damaged store member must leave other generations available in the configurator.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support {
    pub mod configurator;
    pub mod ui;
}

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;

use support::configurator::*;
use tb_app::State;
use tb_config::{Backups, ConfigFile};
use tb_core::config::to_toml;

#[test]
fn an_untrusted_manifest_has_no_restore_control_and_other_generations_can_be_restored() {
    let dir = fresh_dir("configurator_trust", "manifest");
    fs::set_permissions(&dir, Permissions::from_mode(0o700)).unwrap();
    let path = dir.join("home/config.toml");
    fs::create_dir(path.parent().unwrap()).unwrap();
    let backups = Backups::at(dir.join("state/backups"));
    let theme = theme();
    let config = fixture();
    let text = to_toml(&config).unwrap();
    let original = format!("# original\n{text}");
    fs::write(&path, &original).unwrap();
    let mut file = ConfigFile::protected(&path, backups.clone());
    file.load(&theme).unwrap();
    file.save(config.clone(), &theme).unwrap();
    fs::write(&path, format!("# another version\n{text}")).unwrap();
    file.look().unwrap();
    file.save(config, &theme).unwrap();
    let bad = backups.dir().join("2/manifest.toml");
    fs::set_permissions(&bad, Permissions::from_mode(0o644)).unwrap();
    let before = fs::read(&bad).unwrap();

    let size = (1200.0, 1200.0);
    let mut rt = runtime(State::open_protected(Some(path.clone()), Some(backups), theme), size);
    let diagnostic = all_words(&rt);
    assert!(diagnostic.contains("unreadable generation 2"), "{diagnostic}");
    assert!(diagnostic.contains("config unknown") && diagnostic.contains("other users"));
    assert!(rt.find("recover-2").is_none());
    reveal(&mut rt, "recover-1", size);
    tap(&mut rt, "recover-1");
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert_eq!(fs::read(bad).unwrap(), before);
}
