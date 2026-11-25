//! The Touch Bar daemon (`touchbard`): renders the shown layer only when something changes and
//! presents each frame with its damage through a display backend (ADR-0002, ADR-0006). A tap runs
//! its button's action: keys on a virtual keyboard, or a command (ADR-0011, ADR-0012).

mod action;
mod daemon;
mod damage;
mod digitizer;
mod display;
mod drm;
mod evdev;
pub mod fake;
mod fn_keyboard;
mod keys;
mod render;
mod sim_window;
mod stop_read;
mod stop_signals;
mod target;
mod touch;
mod uinput;

pub use action::{ActionError, Actions, KeyEmitter, Report, ShSpawner, Spawner};
pub use daemon::{Daemon, DaemonError, Event};
pub use digitizer::{
    AbsAxis, Calibration, DigitizerError, DigitizerMeta, DigitizerNode, DigitizerProfile, EvdevTouches,
    VERIFIED_DIGITIZERS, calibrate, check_digitizer, confirm_digitizer_node, find_digitizer, open_digitizer,
    open_digitizer_with, validate_digitizer_path,
};
pub use display::{Display, DisplayError, SimDisplay};
pub use drm::{
    CardKms, ConnectorInfo, CrtcInfo, DirtyClip, DrmDisplay, DrmError, DumbBuffer, FbSpec, Kms, KmsMetadata,
    ModeInfo, PlaneInfo, StepFailure, check_touch_bar_card, confirm_touch_bar_node, find_touch_bar_card,
    validate_card_path,
};
pub use fn_keyboard::{FnKeyboard, find_fn_keyboard};
pub use keys::{EvdevModifiers, KeyError, ModifierSource, forward_modifiers};
pub use render::{GpuRenderer, SceneRenderer};
pub use sim_window::{SIM_WINDOW, SimWindow, WindowTouches, sim_window_app, sim_window_view};
pub use stop_read::StopRead;
pub use stop_signals::{StopSignals, block_stop_signals};
pub use target::{NodeId, TargetError};
pub use touch::{LineTouches, Touch, TouchError, TouchSource, forward_touches};
pub use uinput::{EmitError, VirtualKeyboard};
