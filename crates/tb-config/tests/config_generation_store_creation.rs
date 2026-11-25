#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod generations;
}

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

use support::generations::{Profile, theme};
use tb_config::{Backups, ConfigFile};

#[test]
fn a_private_store_is_created_under_an_owned_ancestor_and_reused() {
    let profile = Profile::new("config_generation_store_creation", "new_ancestors");
    let backups = profile.backups();
    let mut file = ConfigFile::protected(&profile.config, backups.clone());
    let parent = profile.store.parent().unwrap();
    assert!(!parent.exists());

    file.load(&theme()).unwrap();

    let uid = fs::metadata(&profile.dir).unwrap().uid();
    assert_eq!(fs::metadata(parent).unwrap().uid(), uid);
    let store = fs::metadata(&profile.store).unwrap();
    assert_eq!(store.uid(), uid);
    assert_eq!(store.permissions().mode() & 0o777, 0o700);

    let mut reopened = ConfigFile::protected(&profile.config, backups);
    reopened.load(&theme()).unwrap();
    reopened.save(reopened.config().clone(), &theme()).unwrap();
    let generations = profile.generations();
    assert_eq!(generations.len(), 2);
    for generation in generations {
        profile.backups().verify(&generation.id).unwrap();
    }
}

#[test]
fn an_existing_state_path_can_resolve_to_a_directory_owned_by_the_user() {
    let profile = Profile::new("config_generation_store_creation", "linked_ancestor");
    let state = profile.dir.join("state");
    let actual = profile.dir.join("actual-state");
    fs::create_dir(&actual).unwrap();
    symlink(&actual, &state).unwrap();
    let mut file = ConfigFile::protected(&profile.config, Backups::at(&profile.store));

    file.load(&theme()).unwrap();

    assert!(fs::symlink_metadata(state).unwrap().is_symlink());
    let store = actual.join("touchbar-customizer/backups");
    assert_eq!(fs::metadata(&store).unwrap().permissions().mode() & 0o777, 0o700);
    let [generation] = profile.generations().try_into().unwrap();
    Backups::at(store).verify(&generation.id).unwrap();
}
