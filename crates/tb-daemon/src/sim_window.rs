//! The simulator window (ADR-0010): a desktop window that shows the simulator's screen live at the
//! bar's proportions, and turns a primary press on it into touches. The only module of
//! `tb-daemon` that names Rune's window types.

use std::sync::mpsc::{self, Receiver, Sender};

use rune::{App, AppSpec, Color, Cx, Drag, El, Phase, Pixels, Tx, col, el};
use tb_core::BarSize;
use tb_render::Frame;

use crate::touch::{Touch, TouchError, TouchSource};

/// The bar at half scale, in window points (1:1 device px on a 2x output).
pub const SIM_WINDOW: (f32, f32) = (1085.0, 30.0);
/// The smallest the window resizes to: a quarter of the bar, keeping its shape (Rune's default
/// minimum, 420 x 400, would not).
const SIM_WINDOW_MIN: (f32, f32) = (542.5, 15.0);
const BAR: BarSize = BarSize::MACBOOK_PRO;

/// The simulator window's state: the screen it shows and where its touches go.
#[derive(Debug)]
pub struct SimWindow {
    screen: Pixels,
    touches: Sender<Touch>,
}

/// Touches from the simulator window; closes (`Ok(None)`) when the window's state is dropped.
#[derive(Debug)]
pub struct WindowTouches {
    touches: Receiver<Touch>,
}

impl TouchSource for WindowTouches {
    fn next_touch(&mut self) -> Result<Option<Touch>, TouchError> {
        Ok(self.touches.recv().ok())
    }
}

impl SimWindow {
    /// A window showing nothing yet (its screen is 0 x 0) and its touch source.
    pub fn new() -> (SimWindow, WindowTouches) {
        let (tx, rx) = mpsc::channel();
        (SimWindow { screen: Pixels::new(), touches: tx }, WindowTouches { touches: rx })
    }

    /// What the window shows: Rune pixels, 0 x 0 until the first frame.
    pub fn screen(&self) -> &Pixels {
        &self.screen
    }

    /// The function to give `SimDisplay::on_present`: shows each screen in this window.
    pub fn show_screen(&self) -> impl FnMut(&Frame) + Send + 'static {
        let screen = self.screen.clone();
        move |frame: &Frame| screen.set(frame.width, frame.height, frame.rgba.clone())
    }

    /// A press is a touch at the bar x under it, its release (or cancel) lifts it; moves land
    /// nothing. The daemon decides what the x hits, as for stdin touches.
    fn on_drag(&mut self, drag: &mut Drag, _tx: &mut Tx) {
        let touch = match drag.phase {
            Phase::Down => Touch::Down { x: bar_x(drag.x, drag.w) },
            Phase::Up => Touch::Up,
            Phase::Move => return,
        };
        // A closed source means the daemon stopped: nobody is left to see the touch.
        let _ = self.touches.send(touch);
    }
}

/// The bar column (device px) under `x` points into the screen element `width` points wide.
fn bar_x(x: f32, width: f32) -> u16 {
    let last = f32::from(BAR.width.saturating_sub(1));
    let x = if width > 0.0 { (x / width * f32::from(BAR.width)).floor() } else { 0.0 };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "floored and clamped to the bar's columns at the Rune boundary"
    )]
    let x = x.clamp(0.0, last) as u16;
    x
}

/// The window's view: element `sim-screen` at the bar's aspect, full window width, centred on a
/// black window, drawing the screen; a primary press on it is a touch.
pub fn sim_window_view(w: &SimWindow, _cx: &mut Cx<SimWindow>) -> El {
    let aspect = f32::from(BAR.width) / f32::from(BAR.height);
    col().w_full().h_full().fill(Color::BLACK).justify_center().child(
        el().key("sim-screen")
            .w_full()
            .aspect_ratio(aspect)
            .pixels(&w.screen)
            .on_drag(|s: &mut SimWindow, d: &mut Drag, tx: &mut Tx| s.on_drag(d, tx)),
    )
}

/// The window app on `window`: the bar at half scale, floated by tiling window managers, and
/// reading none of `touchbard`'s flags.
pub fn sim_window_app(window: SimWindow) -> App<SimWindow> {
    let spec = AppSpec {
        min_size: Some(SIM_WINDOW_MIN),
        ..AppSpec::new("Touch Bar simulator", SIM_WINDOW, unused_init, sim_window_view)
    };
    App::new(spec).init(move || window).args(Vec::<String>::new()).float_on_open()
}

/// `AppSpec` needs a plain function; `App::init` replaces it with the given window.
fn unused_init() -> SimWindow {
    SimWindow::new().0
}
