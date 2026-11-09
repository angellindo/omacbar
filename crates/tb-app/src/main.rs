//! The configurator. `cargo run -p tb-app` opens the window on `~/.config/touchbar-customizer/config.toml`
//! (written with the default config when missing); `cargo run -p tb-app -- --snap out/app.png
//! [--scene <name>]` renders it headless. Run headless with a temp `HOME` so the real config is
//! never written. Scenes: `fn`, `fn-ctrl` (other layers), `selected`, `dragging`, `reordered`,
//! `added`, `issues` (editing states).

fn main() {
    rune::run(tb_app::spec());
}
