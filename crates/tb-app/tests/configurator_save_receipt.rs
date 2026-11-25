//! The real headless configurator emits a receipt for a confirmed explicit save.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/bin.rs"]
mod bin;
mod support {
    pub mod configurator;
    pub mod ui;
}

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;

use serde_json::json;
use sha2::{Digest, Sha256};
use support::configurator::{edit, open_config, operate, words};
use tb_core::config::{Config, ThemeSource, to_toml};

fn run(home: &Path, args: &[&str]) -> String {
    let tree = home.join("tree.json");
    let output = Command::new(env!("CARGO_BIN_EXE_touchbar-customizer"))
        .env("HOME", home)
        .env_remove("XDG_STATE_HOME")
        .env("TB_SAVE_RECEIPT", home.join("receipt.json"))
        .arg("--snap")
        .arg(home.join("app.png"))
        .arg("--tree")
        .arg(&tree)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    fs::read_to_string(tree).unwrap()
}

const FLOW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/rehearsal/edit-label-save.json");

fn assert_receipt(home: &Path, config: &Path) {
    let bytes = fs::read(config).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("label = \"Rehearsal\""));
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(home.join("receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt, json!({"config": config, "sha256": format!("{:x}", Sha256::digest(&bytes))}));
}

#[test]
fn only_the_explicit_save_receipts_the_bytes_the_configurator_wrote() {
    let home = bin::fresh_dir("save-receipt", "explicit");
    let config = tb_config::default_path(&home);
    run(&home, &[]);
    assert!(config.is_file(), "opening created the default");
    assert!(!home.join("receipt.json").exists(), "opening never emits a save receipt");

    run(&home, &["--script", FLOW, "--at", "mark:end"]);
    assert_receipt(&home, &config);
}

#[test]
fn a_save_refused_over_a_missing_selected_theme_emits_no_receipt() {
    let home = bin::fresh_dir("save-receipt", "refused");
    let config = tb_config::default_path(&home);
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let original = to_toml(&Config {
        theme: ThemeSource::File { path: home.join("missing.toml").to_str().unwrap().to_owned() },
        ..Config::default()
    })
    .unwrap();
    fs::write(&config, &original).unwrap();
    let script = home.join("save.json");
    fs::write(
        &script,
        br#"[
            {"t":0.3,"type":"key","name":"s","ctrl":true},
            {"t":0.7,"type":"mark","name":"end"}
        ]"#,
    )
    .unwrap();

    let tree = run(&home, &["--script", script.to_str().unwrap(), "--at", "mark:end"]);
    assert!(bin::text_of(&bin::elements(&tree), "status").unwrap().starts_with("not saved:"));
    assert_eq!(fs::read_to_string(&config).unwrap(), original);
    assert!(!home.join("receipt.json").exists());
}

#[test]
fn an_explicit_save_replaces_the_unreadable_utf8_version_opened_by_the_configurator() {
    let home = bin::fresh_dir("save-receipt", "invalid");
    let config = tb_config::default_path(&home);
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, b"not = [ valid toml \xff\n").unwrap();

    run(&home, &["--script", FLOW, "--at", "mark:end"]);
    assert_receipt(&home, &config);
}

#[test]
fn a_conflict_emits_no_receipt_until_the_configurator_overwrites_with_a_backup() {
    let (mut rt, config) =
        open_config("save_receipt_conflict", &support::configurator::fixture(), tb_app::WINDOW);
    let home = config.parent().unwrap();
    let receipt = home.join("receipt.json");
    rt.state.set_save_receipt(Some(receipt.clone()));
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "Rehearsal");
    let newer = format!("# external edit\n{}", fs::read_to_string(&config).unwrap());
    fs::write(&config, &newer).unwrap();

    operate(&mut rt, "save");
    assert!(words(&rt, "status").contains("changed on disk"));
    assert_eq!(fs::read_to_string(&config).unwrap(), newer);
    assert!(!receipt.exists());

    operate(&mut rt, "conflict-overwrite");
    assert_receipt(home, &config);
    assert_eq!(fs::read_to_string(config.with_extension("toml.bak")).unwrap(), newer);
}

#[test]
fn a_protected_save_receipts_the_new_bytes_and_keeps_the_exact_previous_layout() {
    let home = bin::fresh_dir("save-receipt", "generation");
    let config = tb_config::default_path(&home);
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let original = format!("# keep this comment\n{}", to_toml(&Config::default()).unwrap());
    fs::write(&config, &original).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o640)).unwrap();

    run(&home, &["--script", FLOW, "--at", "mark:end"]);
    assert_receipt(&home, &config);
    assert_eq!(fs::metadata(&config).unwrap().permissions().mode() & 0o777, 0o640);

    let store = home.join(".local/state/touchbar-customizer/backups");
    let [generation]: [_; 1] = tb_config::Backups::at(&store).list(&config).unwrap().try_into().unwrap();
    assert_eq!(generation.reason, tb_config::Reason::Save);
    assert_eq!(generation.health, tb_config::Health::Verified);
    let kept = store.join(generation.id.to_string()).join("config.toml");
    assert_eq!(fs::read_to_string(&kept).unwrap(), original);

    // An app with a receipt path that already exists cannot overwrite someone else's file.
    let receipt = home.join("receipt.json");
    let first = fs::read(&receipt).unwrap();
    let tree = run(&home, &["--script", FLOW, "--at", "mark:end"]);
    assert!(
        bin::text_of(&bin::elements(&tree), "status").unwrap().contains("Could not write the save receipt")
    );
    assert_eq!(fs::read(receipt).unwrap(), first);
    assert_eq!(fs::read_to_string(kept).unwrap(), original);
}

#[test]
fn a_protected_save_refuses_a_symlink_without_a_receipt_or_generation() {
    let home = bin::fresh_dir("save-receipt", "symlink");
    let config = tb_config::default_path(&home);
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let target = home.join("linked.toml");
    let original = to_toml(&Config::default()).unwrap();
    fs::write(&target, &original).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&target, &config).unwrap();

    let tree = run(&home, &["--script", FLOW, "--at", "mark:end"]);
    let status = bin::text_of(&bin::elements(&tree), "status").unwrap();
    assert!(status.starts_with("not saved") && status.contains("symbolic link"), "{status}");
    assert_eq!(fs::read_link(config).unwrap(), target);
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o640);
    assert!(!home.join("receipt.json").exists());
    assert!(!home.join(".local/state/touchbar-customizer/backups").exists());
}

#[test]
fn the_configurator_reports_a_committed_save_even_when_its_receipt_is_refused() {
    use support::configurator::{fixture, runtime, theme};
    use tb_config::{Backups, ConfigFile, GenerationKind, Loaded};

    let profile = bin::fresh_dir("save-receipt", "store-alias");
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o700)).unwrap();
    let path = tb_config::default_path(&profile);
    let store = profile.join("state/backups");
    let backups = Backups::at(&store);
    let mut file = ConfigFile::protected(&path, backups.clone());
    assert_eq!(file.load(&theme()).unwrap(), Loaded::WroteDefault);
    let id = *file.last_generation().unwrap();
    let generation = backups.verify(&id).unwrap();
    assert_eq!(generation.kind, GenerationKind::Absent { dir_existed: false });

    fs::write(&path, to_toml(&fixture()).unwrap()).unwrap();
    let alias = profile.join("receipt-alias");
    symlink(store.join(id.to_string()), &alias).unwrap();
    let receipt = alias.join("config.toml");
    let mut state = tb_app::State::open_protected(Some(path.clone()), Some(backups.clone()), theme());
    state.set_save_receipt(Some(receipt.clone()));
    let mut rt = runtime(state, tb_app::WINDOW);
    operate(&mut rt, "row-a");
    edit(&mut rt, "field-label", "Rehearsal");
    operate(&mut rt, "save");

    let status = words(&rt, "status");
    assert!(status.starts_with("saved config "), "{status}");
    assert!(status.contains("Could not write the save receipt"), "{status}");
    assert!(status.contains("generation store"), "{status}");
    assert!(fs::read_to_string(&path).unwrap().contains("label = \"Rehearsal\""));
    assert!(!receipt.exists());
    assert_eq!(backups.verify(&id).unwrap(), generation);
}
