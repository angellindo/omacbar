//! The real headless configurator emits a receipt for a confirmed explicit save.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/bin.rs"]
mod bin;
mod support {
    pub mod configurator;
    pub mod ui;
}

use std::fs;
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
