//! The configurator. `cargo run -p tb-app` opens the window;
//! `cargo run -p tb-app -- --snap out/app.png [--scene fn-ctrl]` renders it headless.

mod app;

fn main() {
    rune::run(app::spec());
}
