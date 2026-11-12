//! The configurator binary owns the production clock; library callers inject their own.

use std::time::{SystemTime, UNIX_EPOCH};

#[allow(
    clippy::disallowed_methods,
    reason = "wall clock is read only at this binary edge and injected into State"
)]
fn save_time() -> Option<u64> {
    SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs())
}

fn main() {
    let mut spec = tb_app::spec();
    spec.setup = Some(|state, _| state.set_save_clock(save_time));
    rune::run(spec);
}
