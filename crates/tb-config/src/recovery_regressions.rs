//! Exercise protected writes with a local fault at the filesystem boundary.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::fd::AsFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tb_core::config::{Config, to_toml};
use tb_core::{BarTheme, Chord};

use crate::{Backups, ConfigFile, ConfigFileError, GenerationKind, Loaded};

type SyncFault = Box<dyn Fn(&Path) -> io::Result<()>>;
type Edit = Box<dyn FnOnce()>;

thread_local! {
    static SYNC_FAULT: RefCell<Option<SyncFault>> = RefCell::default();
    static TEMP_FAULT: RefCell<Option<Edit>> = RefCell::default();
    static EDIT: RefCell<Option<Edit>> = RefCell::default();
}

pub(crate) fn sync_backup_dir(fd: impl AsFd) -> Result<(), rustix::io::Errno> {
    use std::os::fd::AsRawFd;
    SYNC_FAULT
        .with_borrow(|fault| {
            if let Some(fault) = fault {
                fault(&fs::read_link(format!("/proc/self/fd/{}", fd.as_fd().as_raw_fd()))?)?;
            }
            Ok::<(), io::Error>(())
        })
        .map_err(|_| rustix::io::Errno::IO)?;
    rustix::fs::fsync(fd)
}

pub(crate) fn fail_config_temp() -> io::Result<()> {
    if let Some(edit) = TEMP_FAULT.take() {
        edit();
        return Err(io::Error::other("injected config temporary file failure"));
    }
    Ok(())
}

pub(crate) fn before_config_replace() {
    if let Some(edit) = EDIT.take() {
        edit();
    }
}

struct Profile {
    root: PathBuf,
    config: PathBuf,
    store: PathBuf,
}

impl Profile {
    fn new() -> Self {
        Self::under(&std::env::temp_dir())
    }

    fn under(parent: &Path) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = loop {
            let root = parent.join(format!(
                "tb-config-regression-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match DirBuilder::new().mode(0o700).create(&root) {
                Ok(()) => break root,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => panic!("{e}"),
            }
        };
        let config = root.join("home/.config/touchbar-customizer/config.toml");
        let store = root.join("state/touchbar-customizer/backups");
        Self { root, config, store }
    }

    fn open(&self) -> ConfigFile {
        ConfigFile::protected(&self.config, Backups::at(&self.store))
    }

    fn seed(&self, theme: &BarTheme) -> (ConfigFile, Vec<u8>) {
        let bytes = to_toml(&Config::default_for(theme)).unwrap().into_bytes();
        fs::create_dir_all(self.config.parent().unwrap()).unwrap();
        fs::write(&self.config, &bytes).unwrap();
        fs::set_permissions(&self.config, Permissions::from_mode(0o600)).unwrap();
        let mut file = self.open();
        assert_eq!(file.load(theme).unwrap(), Loaded::Read);
        (file, bytes)
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        SYNC_FAULT.take();
        EDIT.take();
        TEMP_FAULT.take();
        write_identity::MOUNTS.take();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn edited(theme: &BarTheme) -> Config {
    let mut config = Config::default_for(theme);
    let button = &mut config.layout.layer_mut(Chord::Default).unwrap().groups[1].buttons[0];
    "Revised".clone_into(&mut button.label);
    config
}

fn fail_at_directory(dir: PathBuf) {
    SYNC_FAULT.set(Some(Box::new(
        move |at| {
            if at == dir { Err(rustix::io::Errno::IO.into()) } else { Ok(()) }
        },
    )));
}

#[test]
fn a_visible_generation_requires_successful_store_sync_even_on_retry() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let (mut file, original) = p.seed(&theme);
    fail_at_directory(p.store.clone());

    for _ in 0..2 {
        assert!(matches!(file.save(edited(&theme), &theme), Err(ConfigFileError::Backup(_))));
        assert_eq!(fs::read(&p.config).unwrap(), original);
        assert_eq!(fs::read(p.store.join("1/config.toml")).unwrap(), original);
        assert_eq!(fs::read_dir(&p.store).unwrap().count(), 1);
        // Use a fresh ConfigFile to rule out a retry guard that only lives in memory.
        file = p.open();
        file.load(&theme).unwrap();
    }

    SYNC_FAULT.take();
    file.save(edited(&theme), &theme).unwrap();
    assert_eq!(file.last_generation().unwrap().to_string(), "1");
    assert_eq!(file.config(), &edited(&theme));
    assert_eq!(fs::read_dir(&p.store).unwrap().count(), 1);
}

#[test]
fn a_failed_sync_of_a_new_store_ancestor_still_blocks_a_retry() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let (mut file, original) = p.seed(&theme);
    let ancestor = p.root.join("state");
    fail_at_directory(ancestor.clone());

    for _ in 0..2 {
        assert!(matches!(file.save(edited(&theme), &theme), Err(ConfigFileError::Backup(_))));
        assert!(ancestor.join("touchbar-customizer").is_dir());
        assert_eq!(fs::read(&p.config).unwrap(), original);
    }

    SYNC_FAULT.take();
    file.save(edited(&theme), &theme).unwrap();
    assert_eq!(file.config(), &edited(&theme));
}

#[test]
fn a_failed_generation_leaves_the_first_write_directory_absent() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let mut file = p.open();
    SYNC_FAULT.set(Some(Box::new(|at| {
        if at.file_name().is_some_and(|name| name.to_string_lossy().starts_with(".tmp-")) {
            Err(rustix::io::Errno::IO.into())
        } else {
            Ok(())
        }
    })));

    assert!(matches!(file.load(&theme), Err(ConfigFileError::Backup(_))));
    assert!(!p.root.join("home").exists());
    assert_eq!(fs::read_dir(&p.store).unwrap().count(), 0);

    SYNC_FAULT.take();
    assert_eq!(file.load(&theme).unwrap(), Loaded::WroteDefault);
    let id = *file.last_generation().unwrap();
    let kept = file.backups().unwrap().verify(&id).unwrap();
    assert_eq!(kept.kind, GenerationKind::Absent { dir_existed: false });
}

#[test]
fn a_failed_generation_leaves_concurrently_created_content_alone() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let mut file = p.open();
    let folder = p.config.parent().unwrap().to_path_buf();
    let concurrent_folder = folder.clone();
    SYNC_FAULT.set(Some(Box::new(move |at| {
        if at.file_name().is_some_and(|name| name.to_string_lossy().starts_with(".tmp-")) {
            fs::create_dir_all(&concurrent_folder)?;
            fs::write(concurrent_folder.join("other"), b"concurrent content")?;
            return Err(rustix::io::Errno::IO.into());
        }
        Ok(())
    })));

    assert!(matches!(file.load(&theme), Err(ConfigFileError::Backup(_))));
    assert!(!p.config.exists());
    assert_eq!(fs::read(folder.join("other")).unwrap(), b"concurrent content");

    SYNC_FAULT.take();
    assert_eq!(file.load(&theme).unwrap(), Loaded::WroteDefault);
    let id = *file.last_generation().unwrap();
    let kept = file.backups().unwrap().verify(&id).unwrap();
    assert_eq!(kept.kind, GenerationKind::Absent { dir_existed: true });
}

#[test]
fn a_protected_save_preserves_permission_changes_made_after_the_backup() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let (mut file, original) = p.seed(&theme);
    let path = p.config.clone();
    EDIT.set(Some(Box::new(move || {
        fs::set_permissions(path, Permissions::from_mode(0o640)).unwrap();
    })));

    assert!(
        matches!(file.save(edited(&theme), &theme), Err(ConfigFileError::ChangedAfterKeep { generation, .. }) if generation.to_string() == "1")
    );
    assert_eq!(fs::metadata(&p.config).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(fs::read(&p.config).unwrap(), original);
    assert_eq!(file.config(), &Config::default_for(&theme));
    assert!(file.last_generation().is_none());
    let kept = file.backups().unwrap().list(&p.config).unwrap();
    assert_eq!(kept[0].mode, 0o600);

    file.save(edited(&theme), &theme).unwrap();
    let id = *file.last_generation().unwrap();
    assert_eq!(file.backups().unwrap().verify(&id).unwrap().mode, 0o640);
    assert_eq!(fs::metadata(&p.config).unwrap().permissions().mode() & 0o777, 0o640);
}

#[test]
fn a_protected_save_preserves_a_symlink_or_hard_link_added_after_the_backup() {
    for symbolic in [true, false] {
        let p = Profile::new();
        let theme = BarTheme::omarchy_fallback();
        let (mut file, original) = p.seed(&theme);
        let alias = p.root.join("same-bytes");
        let target = alias.clone();
        let path = p.config.clone();
        EDIT.set(Some(Box::new(move || {
            if symbolic {
                fs::rename(&path, &target).unwrap();
                symlink(&target, &path).unwrap();
            } else {
                fs::hard_link(&path, &target).unwrap();
            }
        })));

        let result = file.save(edited(&theme), &theme);
        if symbolic {
            assert!(
                matches!(&result, Err(ConfigFileError::RefusedAfterKeep { generation, source }) if generation.to_string() == "1" && matches!(**source, ConfigFileError::Symlink { .. })),
                "{result:?}"
            );
            assert_eq!(fs::read_link(&p.config).unwrap(), alias);
        } else {
            assert!(
                matches!(&result, Err(ConfigFileError::RefusedAfterKeep { generation, source }) if generation.to_string() == "1" && matches!(**source, ConfigFileError::Unsupported { .. })),
                "{result:?}"
            );
            assert_eq!(fs::metadata(&p.config).unwrap().nlink(), 2);
        }
        assert_eq!(fs::read(&p.config).unwrap(), original);
        assert_eq!(fs::read(alias).unwrap(), original);
        assert!(file.last_generation().is_none());
    }
}

#[test]
fn read_only_store_access_checks_the_owner_of_its_parent() {
    let parent = Path::new("/tmp");
    if fs::metadata(parent).unwrap().uid() == rustix::process::geteuid().as_raw() {
        return;
    }
    let p = Profile::under(parent);
    // The fixture itself is an owned, private store under an existing foreign directory.
    let backups = Backups::at(&p.root);
    let failure = backups.list(&p.config).unwrap_err().to_string();
    assert!(failure.contains("belongs to uid") && failure.contains("XDG_STATE_HOME"), "{failure}");
    assert_eq!(fs::read_dir(&p.root).unwrap().count(), 0);
}

#[test]
fn protected_writes_refuse_config_paths_inside_file_and_absence_generations() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let mut file = p.open();
    file.load(&theme).unwrap();
    file.save(edited(&theme), &theme).unwrap();
    let backups = file.backups().unwrap();
    let history = backups.list(&p.config).unwrap();
    let mut outcomes = Vec::new();
    for generation in &history {
        let dir = p.store.join(generation.id.to_string());
        let alias = p.root.join(format!("alias-{}", generation.id));
        symlink(&dir, &alias).unwrap();
        let path = alias.join("config.toml");
        let mut protected = ConfigFile::protected(&path, backups.clone());
        if generation.kind == GenerationKind::File {
            protected.look().unwrap();
            outcomes.push(protected.save(edited(&theme), &theme));
        } else {
            outcomes.push(protected.load(&theme).map(drop));
        }
        outcomes.push(protected.recover(&generation.id));
        outcomes.push(protected.recover_removing(&generation.id));
        let mut nested = ConfigFile::protected(alias.join("missing/deeper/config.toml"), backups.clone());
        outcomes.push(nested.load(&theme).map(drop));
    }
    for result in outcomes {
        assert!(matches!(&result, Err(ConfigFileError::Backup(_))), "{result:?}");
    }
    assert_eq!(backups.list(&p.config).unwrap(), history);
    assert_eq!(fs::read_dir(&p.store).unwrap().count(), history.len());
    for generation in &history {
        assert_eq!(backups.verify(&generation.id).unwrap(), *generation);
        assert!(!p.store.join(generation.id.to_string()).join("missing").exists());
    }
}

#[test]
fn a_late_first_write_failure_preserves_only_directories_with_other_content() {
    for with_content in [false, true] {
        let p = Profile::new();
        let theme = BarTheme::omarchy_fallback();
        let existing = p.root.join("home");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("keep"), b"existing").unwrap();
        let folder = p.config.parent().unwrap().to_path_buf();
        let concurrent = folder.join("concurrent");
        let content = concurrent.clone();
        let store = p.store.clone();
        TEMP_FAULT.set(Some(Box::new(move || {
            assert!(store.join("1/manifest.toml").is_file());
            if with_content {
                fs::write(content, b"concurrent").unwrap();
            }
        })));
        let mut file = p.open();
        let result = file.load(&theme);
        assert!(
            matches!(&result, Err(ConfigFileError::RefusedAfterKeep { generation, source }) if generation.to_string() == "1" && matches!(**source, ConfigFileError::Io { .. })),
            "{result:?}"
        );
        assert!(!p.config.exists());
        assert_eq!(fs::read(existing.join("keep")).unwrap(), b"existing");
        assert_eq!(existing.join(".config").exists(), with_content);
        if with_content {
            assert_eq!(fs::read(concurrent).unwrap(), b"concurrent");
        }
        let kept = file.backups().unwrap().list(&p.config).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].kind, GenerationKind::Absent { dir_existed: false });
        let mut retry = p.open();
        assert_eq!(retry.load(&theme).unwrap(), Loaded::WroteDefault);
        let history = retry.backups().unwrap().list(&p.config).unwrap();
        assert_eq!(history.len(), 1 + usize::from(with_content));
        assert_eq!(history[0].kind, GenerationKind::Absent { dir_existed: with_content });
        assert_eq!(history.last(), kept.first());
    }
}

#[test]
fn a_conflict_after_publication_reports_its_retained_generation() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let (mut file, original) = p.seed(&theme);
    let path = p.config.clone();
    let store = p.store.clone();
    EDIT.set(Some(Box::new(move || {
        assert!(store.join("1/manifest.toml").is_file());
        fs::set_permissions(path, Permissions::from_mode(0o640)).unwrap();
    })));
    let failure = file.save(edited(&theme), &theme).unwrap_err();
    assert!(failure.to_string().contains("generation 1"), "{failure}");
    assert!(file.last_generation().is_none());
    assert_eq!(fs::read(&p.config).unwrap(), original);
    assert_eq!(fs::metadata(&p.config).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(
        file.backups().unwrap().verify(&crate::GenerationId::parse("1").unwrap()).unwrap().mode,
        0o600
    );

    let backups = file.backups().unwrap().clone();
    let foreign_path = p.root.join("other/config.toml");
    let mut foreign = ConfigFile::protected(&foreign_path, backups.clone());
    foreign.load(&theme).unwrap();
    let foreign_id = *foreign.last_generation().unwrap();
    let foreign_before = backups.verify(&foreign_id).unwrap();
    let original_id = crate::GenerationId::parse("1").unwrap();
    let original_before = backups.verify(&original_id).unwrap();
    fs::set_permissions(&p.config, Permissions::from_mode(0o600)).unwrap();
    let path = p.config.clone();
    EDIT.set(Some(Box::new(move || {
        fs::set_permissions(path, Permissions::from_mode(0o640)).unwrap();
    })));
    let repeated = file.save(edited(&theme), &theme).unwrap_err();
    assert!(repeated.to_string().contains("generation 1"), "{repeated}");
    assert_eq!(backups.verify(&foreign_id).unwrap(), foreign_before);
    assert_eq!(backups.verify(&original_id).unwrap(), original_before);
    assert_eq!(fs::read_dir(&p.store).unwrap().count(), 2);
}

#[test]
fn a_first_write_failure_does_not_remove_a_replacement_directory_or_symlink() {
    for symbolic in [false, true] {
        let p = Profile::new();
        let theme = BarTheme::omarchy_fallback();
        let folder = p.config.parent().unwrap().to_path_buf();
        let moved = p.root.join("moved");
        let redirect = p.root.join("existing");
        fs::create_dir(&redirect).unwrap();
        let after = (folder.clone(), moved.clone(), redirect.clone());
        TEMP_FAULT.set(Some(Box::new(move || {
            fs::rename(&after.0, &after.1).unwrap();
            if symbolic {
                symlink(&after.2, &after.0).unwrap();
            } else {
                fs::create_dir(&after.0).unwrap();
            }
        })));
        let mut file = p.open();
        assert!(file.load(&theme).is_err());
        assert_eq!(fs::read_dir(moved).unwrap().count(), 0);
        assert!(redirect.is_dir());
        assert!(!p.config.exists());
        if symbolic {
            assert_eq!(fs::read_link(&folder).unwrap(), redirect);
        } else {
            assert!(folder.is_dir());
        }
        assert_eq!(file.load(&theme).unwrap(), Loaded::WroteDefault);
        let id = *file.last_generation().unwrap();
        assert_eq!(
            file.backups().unwrap().verify(&id).unwrap().kind,
            GenerationKind::Absent { dir_existed: true }
        );
    }
}

#[test]
fn a_change_while_the_generation_is_staged_keeps_no_generation() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let (mut file, original) = p.seed(&theme);
    let path = p.config.clone();
    SYNC_FAULT.set(Some(Box::new(move |at| {
        if at.file_name().is_some_and(|name| name.to_string_lossy().starts_with(".tmp-")) {
            fs::set_permissions(&path, Permissions::from_mode(0o640))?;
        }
        Ok(())
    })));
    assert!(matches!(file.save(edited(&theme), &theme), Err(ConfigFileError::Changed(_))));
    assert_eq!(fs::read(&p.config).unwrap(), original);
    assert_eq!(fs::metadata(&p.config).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(fs::read_dir(&p.store).unwrap().count(), 0);
    assert_eq!(fs::read_dir(p.config.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn a_conflicting_absence_restore_keeps_the_config_and_reports_the_generation_before_removal() {
    let p = Profile::new();
    let theme = BarTheme::omarchy_fallback();
    let mut file = p.open();
    file.load(&theme).unwrap();
    let absent = *file.last_generation().unwrap();
    let original = fs::read(&p.config).unwrap();
    let path = p.config.clone();
    EDIT.set(Some(Box::new(move || {
        fs::set_permissions(path, Permissions::from_mode(0o640)).unwrap();
    })));

    let Err(ConfigFileError::ChangedAfterKeep { generation, current }) = file.recover_removing(&absent)
    else {
        panic!("the removal must report a conflict after keeping a generation");
    };
    assert!(!current.is_absent());
    assert_eq!(file.last_generation(), Some(&absent));
    assert_eq!(file.backups().unwrap().verify(&generation).unwrap().mode, 0o600);
    assert_eq!(fs::read(p.store.join(generation.to_string()).join("config.toml")).unwrap(), original);
    assert_eq!(fs::read(&p.config).unwrap(), original);
    assert_eq!(fs::metadata(&p.config).unwrap().permissions().mode() & 0o777, 0o640);

    let mut retry = p.open();
    retry.look().unwrap();
    retry.recover_removing(&absent).unwrap();
    let kept = *retry.last_generation().unwrap();
    assert_eq!(retry.backups().unwrap().verify(&kept).unwrap().mode, 0o640);
    assert!(!p.config.exists());
    assert!(p.config.parent().unwrap().is_dir());
    assert_eq!(retry.backups().unwrap().list(&p.config).unwrap().len(), 3);
}

#[path = "write_identity_regressions.rs"]
pub(crate) mod write_identity;
