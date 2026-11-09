//! Functional, end to end: the flow script `scripts/drag-f3-before-f1.json` drags F3 onto F1's
//! left half in the preview and presses save, in the real `touchbar-customizer` binary with a
//! fresh temp `HOME`. The saved config file then reads back as the default config with F3 first
//! in the F-key group, and a second run on the same `HOME` shows that order: the configurator
//! reloads what it saved. Needs a GPU adapter (lavapipe is fine).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support {
    pub mod bin;
}

use support::bin::*;
use tb_config::{ConfigFile, Loaded, default_path};
use tb_core::config::Config;
use tb_core::{BarTheme, Chord};

/// The default config with the default layer's F-key group in this order, built by hand.
fn default_with_f_keys(order: &[&str]) -> Config {
    let mut config = Config::default();
    let layer = config.layout.layer_mut(Chord::Default).unwrap();
    let f_keys = layer.groups[1].buttons.clone();
    layer.groups[1].buttons =
        order.iter().map(|id| f_keys.iter().find(|b| b.id.as_str() == *id).cloned().unwrap()).collect();
    config
}

#[test]
fn the_flow_script_drags_f3_before_f1_saves_and_a_reload_reads_the_same_config() {
    let dir = fresh_dir("save_flow_script", "drag_save_reload");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let script = drag_f3_before_f1_script();

    let saved = snap(
        &home,
        &dir,
        "flow",
        &["--script".as_ref(), script.as_os_str(), "--at".as_ref(), "mark:end".as_ref()],
    );
    let status = text_of(&saved, "status").unwrap_or_default();
    assert!(status.starts_with("saved"), "the status says the config was saved: {status:?}");

    let mut file = ConfigFile::new(default_path(&home));
    assert_eq!(file.load(&BarTheme::omarchy_fallback()).unwrap(), Loaded::Read);
    let expected =
        default_with_f_keys(&["f3", "f1", "f2", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"]);
    assert_eq!(file.config(), &expected, "the file holds the default config with F3 before F1");

    let reloaded = snap(&home, &dir, "reload", &[]);
    assert!(
        x_of(&reloaded, "preview-f3") < x_of(&reloaded, "preview-f1"),
        "a fresh run shows the saved order: f3 x {} vs f1 x {}",
        x_of(&reloaded, "preview-f3"),
        x_of(&reloaded, "preview-f1")
    );
    let mut again = ConfigFile::new(default_path(&home));
    assert_eq!(again.load(&BarTheme::omarchy_fallback()).unwrap(), Loaded::Read);
    assert_eq!(again.config(), &expected, "opening the configurator leaves the saved file as it was");
}
