//! Protected write regressions at the Linux topology and directory syscall boundaries.

use std::cell::RefCell;
use std::ffi::OsStr;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::rc::Rc;

use super::*;
use rustix::io::Errno;

type BeforePin = Box<dyn FnOnce(&Path, &Path)>;

thread_local! {
    pub(crate) static MOUNTS: RefCell<Option<MountFixture>> = const { RefCell::new(None) };
    static BEFORE_PIN: RefCell<Option<BeforePin>> = const { RefCell::new(None) };
    static MKDIR_FAULT: RefCell<Option<(&'static OsStr, bool)>> = const { RefCell::new(None) };
}

pub(crate) struct MountFixture {
    pub(crate) mountinfo: Vec<u8>,
    ids: Vec<(PathBuf, u64)>,
}

impl MountFixture {
    fn new(root: &Path) -> Self {
        let mut fixture = Self { mountinfo: Vec::new(), ids: Vec::new() };
        fixture.add(root, 1, "8:1", Path::new("/"));
        fixture
    }

    fn add(&mut self, point: &Path, id: u64, device: &str, root: &Path) {
        let point = fs::canonicalize(point).unwrap();
        self.ids.push((point.clone(), id));
        self.mountinfo.extend(format!("{id} 1 {device} ").bytes());
        mountinfo_path(&mut self.mountinfo, root);
        self.mountinfo.push(b' ');
        mountinfo_path(&mut self.mountinfo, &point);
        self.mountinfo.extend(b" rw - ext4 /dev/root rw\n");
    }

    pub(crate) fn mount_id(&self, path: &Path) -> Option<u64> {
        self.ids
            .iter()
            .filter(|(point, _)| path.starts_with(point))
            .max_by_key(|(point, _)| point.components().count())
            .map(|(_, id)| *id)
    }
}

fn mountinfo_path(line: &mut Vec<u8>, path: &Path) {
    for &b in path.as_os_str().as_bytes() {
        match b {
            b' ' => line.extend(b"\\040"),
            b'\t' => line.extend(b"\\011"),
            b'\n' => line.extend(b"\\012"),
            b'\\' => line.extend(b"\\134"),
            _ => line.push(b),
        }
    }
}

pub(crate) fn config_directory_fd(
    parent: impl AsFd,
    name: &OsStr,
    stage: &OsStr,
    cloning: bool,
) -> rustix::io::Result<()> {
    if !cloning && let Some(edit) = BEFORE_PIN.take() {
        let parent = fs::read_link(format!("/proc/self/fd/{}", parent.as_fd().as_raw_fd())).unwrap();
        edit(&parent.join(name), &parent.join(stage));
    }
    if MKDIR_FAULT.with_borrow(|fault| *fault == Some((name, cloning))) {
        MKDIR_FAULT.take();
        return Err(Errno::MFILE);
    }
    Ok(())
}

#[test]
fn first_write_rolls_back_its_parents_when_intermediate_open_runs_out_of_descriptors() {
    config_directory_exhaustion(false, false);
}

#[test]
fn first_write_rolls_back_its_parents_when_intermediate_clone_runs_out_of_descriptors() {
    config_directory_exhaustion(false, true);
}

#[test]
fn first_write_reuses_absence_when_opening_its_leaf_runs_out_of_descriptors() {
    config_directory_exhaustion(true, false);
}

#[test]
fn first_write_reuses_absence_when_cloning_its_leaf_runs_out_of_descriptors() {
    config_directory_exhaustion(true, true);
}

fn config_directory_exhaustion(leaf: bool, cloning: bool) {
    let theme = BarTheme::omarchy_fallback();
    let profile = Profile::new();
    let folder = profile.config.parent().unwrap();
    let failed = if leaf {
        fs::create_dir_all(folder.parent().unwrap()).unwrap();
        OsStr::new("touchbar-customizer")
    } else {
        OsStr::new(".config")
    };
    MKDIR_FAULT.set(Some((failed, cloning)));
    let mut file = profile.open();
    let error = file.load(&theme).unwrap_err();
    assert!(error.to_string().contains(&io::Error::from(Errno::MFILE).to_string()), "{error}");
    let ConfigFileError::RefusedAfterKeep { generation, .. } = error else {
        panic!("first write must report the retained absence");
    };
    let backups = file.backups().unwrap();
    assert_eq!(backups.verify(&generation).unwrap().kind, GenerationKind::Absent { dir_existed: false });
    if leaf {
        assert!(!folder.exists(), "a failed first write leaked the leaf");
        assert_eq!(fs::read_dir(folder.parent().unwrap()).unwrap().count(), 0);
    } else {
        assert!(!profile.root.join("home").exists(), "a failed first write leaked a parent");
    }
    let mut fresh = profile.open();
    assert_eq!(fresh.load(&theme).unwrap(), Loaded::WroteDefault);
    assert_eq!(fresh.last_generation(), Some(&generation));
    assert_eq!(backups.list(&profile.config).unwrap().len(), 1);
}

#[test]
fn failed_open_preserves_a_replacement_with_a_different_staging_inode() {
    let theme = BarTheme::omarchy_fallback();
    for with_content in [false, true] {
        let profile = Profile::new();
        let folder = profile.config.parent().unwrap();
        fs::create_dir_all(folder.parent().unwrap()).unwrap();
        let replacement = profile.root.join("replacement");
        fs::create_dir(&replacement).unwrap();
        let inode = fs::metadata(&replacement).unwrap().ino();
        if with_content {
            fs::write(replacement.join("other"), b"concurrent content").unwrap();
        }
        let displaced = profile.root.join("displaced");
        let observed = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&observed);
        BEFORE_PIN.set(Some(Box::new(move |_, stage| {
            fs::rename(stage, displaced).unwrap();
            fs::rename(replacement, stage).unwrap();
            slot.replace(Some(stage.to_path_buf()));
        })));
        MKDIR_FAULT.set(Some((OsStr::new("touchbar-customizer"), false)));
        let error = profile.open().load(&theme).unwrap_err();
        assert!(error.to_string().contains(&io::Error::from(Errno::MFILE).to_string()), "{error}");
        let actual = observed.take().unwrap();
        assert_eq!(fs::metadata(&actual).unwrap().ino(), inode);
        if with_content {
            assert_eq!(fs::read(actual.join("other")).unwrap(), b"concurrent content");
        } else {
            assert_eq!(fs::read_dir(actual).unwrap().count(), 0);
        }
    }
}

#[test]
fn first_write_does_not_pin_or_remove_an_empty_directory_installed_before_open() {
    let theme = BarTheme::omarchy_fallback();
    let profile = Profile::new();
    let parent = profile.config.parent().unwrap();
    fs::create_dir_all(parent.parent().unwrap()).unwrap();
    let replacement = profile.root.join("replacement");
    fs::create_dir(&replacement).unwrap();
    let replacement_inode = fs::metadata(&replacement).unwrap().ino();
    let moved = replacement.clone();
    let displaced = profile.root.join("displaced");
    BEFORE_PIN.set(Some(Box::new(move |target, _| {
        if target.is_dir() {
            fs::rename(target, displaced).unwrap();
        }
        fs::rename(moved, target).unwrap();
    })));
    // On the baseline the write incorrectly adopts the replacement. Force a later failure
    // so its rollback attempts to remove it too.
    TEMP_FAULT.set(Some(Box::new(|| {})));
    assert!(profile.open().load(&theme).is_err());
    TEMP_FAULT.take();
    assert_eq!(fs::metadata(parent).unwrap().ino(), replacement_inode);
    assert_eq!(fs::read_dir(parent).unwrap().count(), 0);
    assert_eq!(fs::read_dir(parent.parent().unwrap()).unwrap().count(), 1);

    let mut fresh = profile.open();
    assert_eq!(fresh.load(&theme).unwrap(), Loaded::WroteDefault);
    let kept = fresh.backups().unwrap().verify(fresh.last_generation().unwrap()).unwrap();
    assert_eq!(kept.kind, GenerationKind::Absent { dir_existed: true });
}

fn with_generation_mount(test: impl FnOnce(&Profile, ConfigFile, &Path, &BarTheme)) {
    let profile = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let (mut file, original) = profile.seed(&theme);
    file.save(edited(&theme), &theme).unwrap();
    let alias = profile.root.join("generation alias\\041");
    fs::create_dir(&alias).unwrap();
    fs::write(alias.join("config.toml"), &original).unwrap();
    let mut mounts = MountFixture::new(&profile.root);
    mounts.add(&alias, 2, "8:1", Path::new("/state/touchbar-customizer/backups/1"));
    MOUNTS.set(Some(mounts));
    test(&profile, file, &alias, &theme);
    MOUNTS.take();
    assert_eq!(fs::read(alias.join("config.toml")).unwrap(), original);
    assert_eq!(fs::read(profile.store.join("1/config.toml")).unwrap(), original);
    assert_eq!(fs::read_dir(&profile.store).unwrap().count(), 1);
}

#[test]
fn save_refuses_a_mount_root_inside_a_generation() {
    with_generation_mount(|profile, _, alias, theme| {
        let mut config = ConfigFile::protected(alias.join("config.toml"), Backups::at(&profile.store));
        config.look().unwrap();
        assert!(matches!(config.save(edited(theme), theme), Err(ConfigFileError::Backup(_))));
    });
}

#[test]
fn first_open_refuses_missing_config_parents_through_a_generation_mount() {
    with_generation_mount(|profile, _, alias, theme| {
        let mut config =
            ConfigFile::protected(alias.join("missing/deeper/config.toml"), Backups::at(&profile.store));
        assert!(matches!(config.load(theme), Err(ConfigFileError::Backup(_))));
        assert!(!alias.join("missing").exists());
    });
}

#[test]
fn restore_refuses_a_mount_root_inside_the_store_before_generation_lookup() {
    with_generation_mount(|profile, _, alias, _| {
        let mut config = ConfigFile::protected(alias.join("config.toml"), Backups::at(&profile.store));
        config.look().unwrap();
        let id = crate::GenerationId::parse("1").unwrap();
        assert!(matches!(
            config.recover(&id),
            Err(ConfigFileError::Backup(crate::BackupError::Unsafe { .. }))
        ));
        assert!(matches!(
            config.recover_removing(&id),
            Err(ConfigFileError::Backup(crate::BackupError::Unsafe { .. }))
        ));
    });
}

#[test]
fn receipt_refuses_a_fresh_name_through_a_generation_mount() {
    with_generation_mount(|_, file, alias, _| {
        let receipt = alias.join("receipt.json");
        assert_eq!(file.write_save_receipt(&receipt).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert!(!receipt.exists());
    });
}

#[test]
fn receipt_refuses_a_fresh_name_through_an_absence_generation_mount() {
    let profile = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let mut file = profile.open();
    assert_eq!(file.load(&theme).unwrap(), Loaded::WroteDefault);
    let id = *file.last_generation().unwrap();
    file.save(edited(&theme), &theme).unwrap();
    let generation = profile.store.join(id.to_string());
    let manifest = fs::read(generation.join("manifest.toml")).unwrap();
    let alias = profile.root.join("absence");
    fs::create_dir(&alias).unwrap();
    let mut mounts = MountFixture::new(&profile.root);
    mounts.add(&alias, 2, "8:1", Path::new("/state/touchbar-customizer/backups/1"));
    MOUNTS.set(Some(mounts));

    let receipt = alias.join("receipt.json");
    assert_eq!(file.write_save_receipt(&receipt).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    assert_eq!(fs::read_dir(&alias).unwrap().count(), 0);
    assert_eq!(fs::read(generation.join("manifest.toml")).unwrap(), manifest);
    let backups = file.backups().unwrap();
    assert_eq!(backups.verify(&id).unwrap().kind, GenerationKind::Absent { dir_existed: false });
}

#[test]
fn first_open_refuses_aliases_of_overlapping_missing_destination_trees() {
    let profile = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let alias = profile.root.join("alias");
    fs::create_dir(&alias).unwrap();
    let mut mounts = MountFixture::new(&profile.root);
    mounts.add(&alias, 2, "8:1", Path::new("/home"));
    MOUNTS.set(Some(mounts));

    let store = alias.join(".config/touchbar-customizer/inside/backups");
    let mut file = ConfigFile::protected(&profile.config, Backups::at(&store));
    assert!(matches!(file.load(&theme), Err(ConfigFileError::Backup(crate::BackupError::Unsafe { .. }))));
    assert_eq!(fs::read_dir(&alias).unwrap().count(), 0);
    assert!(!profile.root.join("home").exists());
}

#[test]
fn a_receipt_cannot_reach_a_nested_store_mount_through_its_second_view() {
    with_generation_mount(|profile, file, alias, _| {
        let nested = profile.store.join("1");
        MOUNTS.with_borrow_mut(|fixture| {
            let mounts = fixture.as_mut().unwrap();
            let point = fs::canonicalize(alias).unwrap();
            mounts.ids.retain(|(p, _)| *p != point);
            mounts.add(&nested, 3, "8:2", Path::new("/"));
            mounts.add(alias, 4, "8:2", Path::new("/"));
        });
        let receipt = alias.join("receipt.json");
        assert_eq!(file.write_save_receipt(&receipt).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert!(!receipt.exists());
    });
}

#[test]
fn a_receipt_fails_closed_when_its_descriptor_mount_is_missing_from_the_snapshot() {
    with_generation_mount(|_, file, alias, _| {
        MOUNTS.with_borrow_mut(|fixture| fixture.as_mut().unwrap().ids.last_mut().unwrap().1 = 42);
        let receipt = alias.join("receipt.json");
        assert!(file.write_save_receipt(&receipt).unwrap_err().to_string().contains("mount identity"));
        assert!(!receipt.exists());
    });
}

#[test]
fn ordinary_home_and_xdg_paths_work_on_sibling_mounts_and_subvolumes() {
    let theme = BarTheme::omarchy_fallback();
    for (device, home_root, state_root) in [("8:2", "/", "/"), ("8:1", "/@home", "/@state")] {
        let profile = Profile::new();
        let home = profile.root.join("home");
        let state = profile.root.join("state");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&state).unwrap();
        let mut mounts = MountFixture::new(&profile.root);
        mounts.add(&home, 2, "8:1", Path::new(home_root));
        mounts.add(&state, 3, device, Path::new(state_root));
        MOUNTS.set(Some(mounts));

        let mut file = profile.open();
        assert_eq!(file.load(&theme).unwrap(), Loaded::WroteDefault);
        file.save(edited(&theme), &theme).unwrap();
        let original = *file.last_generation().unwrap();
        file.recover(&original).unwrap();
        let receipt = home.join("receipt.json");
        file.write_save_receipt(&receipt).unwrap();
        assert!(receipt.is_file());
        assert!(file.backups().unwrap().verify(&original).is_ok());
        MOUNTS.take();
    }
}

#[test]
fn a_config_and_store_in_separate_locations_can_save_and_restore() {
    let theme = BarTheme::omarchy_fallback();
    let config = Profile::new();
    let state = Profile::under(Path::new("/tmp"));
    let mut file = ConfigFile::protected(&config.config, Backups::at(state.store.clone()));
    assert_eq!(file.load(&theme).unwrap(), Loaded::WroteDefault);
    file.save(edited(&theme), &theme).unwrap();
    let id = *file.last_generation().unwrap();
    file.recover(&id).unwrap();
    assert!(file.backups().unwrap().verify(&id).is_ok());
}

#[test]
fn a_receipt_cannot_occupy_a_displaced_stores_directory_name_or_its_missing_parent() {
    let profile = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let (mut file, _) = profile.seed(&theme);
    file.save(edited(&theme), &theme).unwrap();

    for path in [&profile.store, profile.store.parent().unwrap()] {
        let displaced =
            profile.root.join(format!("displaced-{}", path.file_name().unwrap().to_string_lossy()));
        fs::rename(path, &displaced).unwrap();
        assert_eq!(file.write_save_receipt(path).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert!(!path.exists());
        assert!(displaced.is_dir());
    }
}
