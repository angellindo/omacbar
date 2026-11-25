//! Opt-in device selection and ownership for the touchbard entry path.

use std::fs::{self, File};
use std::io::{self, BufReader, PipeReader};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use tb_core::BarSize;
use tb_daemon::{
    CardKms, DrmDisplay, EvdevTouches, Kms, LineTouches, NodeId, SceneRenderer, SimDisplay, StopRead,
    StopSignals, TouchSource, VERIFIED_DIGITIZERS, check_digitizer, check_touch_bar_card,
    confirm_touch_bar_node, find_digitizer, find_touch_bar_card, open_digitizer_with, validate_card_path,
    validate_digitizer_path,
};

use crate::workers::Workers;
use crate::{
    Args, ConfigState, DEVICES, Message, actions, follow_keys_and_theme, forward_touches, run_daemon,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Selection {
    Auto,
    Path(PathBuf),
}

impl From<String> for Selection {
    fn from(value: String) -> Self {
        if value == "auto" { Self::Auto } else { Self::Path(value.into()) }
    }
}

impl Args {
    /// Check both flags before discovery, GPU initialization or any device open.
    pub(super) fn validate_device_paths(&self) -> Result<(), String> {
        if let Some(Selection::Path(path)) = &self.drm {
            validate_card_path(path).map_err(|e| e.to_string())?;
        }
        if let Some(Selection::Path(path)) = &self.touch {
            validate_digitizer_path(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

type OpenCard<K> = Box<dyn FnOnce(&Path) -> io::Result<(K, NodeId)>>;
type OpenTouch<T> = Box<dyn FnOnce(&Path) -> Result<T, String>>;

#[allow(clippy::struct_field_names, reason = "The sealed seam names the input_devices path")]
pub(super) struct Devices<K, T> {
    pub sysfs: PathBuf,
    pub input_devices: PathBuf,
    pub open_card: OpenCard<K>,
    pub open_touch: OpenTouch<T>,
}

impl Devices<CardKms, EvdevTouches<StopRead<File>>> {
    pub(super) fn real(stop: Option<Arc<PipeReader>>) -> Self {
        let sysfs = PathBuf::from("/sys");
        let digitizer_sysfs = sysfs.clone();
        Self {
            sysfs,
            input_devices: DEVICES.into(),
            open_card: Box::new(|path| {
                let card = CardKms::open(path)?;
                let id = card.node_id()?;
                Ok((card, id))
            }),
            open_touch: Box::new(move |path| {
                open_digitizer_with(&digitizer_sysfs, path, VERIFIED_DIGITIZERS, |file| StopRead(file, stop))
                    .map_err(|error| error.to_string())
            }),
        }
    }
}

/// The acceptance seam for an arbitrary injected touch source, including a permanently blocked
/// one. Production supplies a cancellable reader and joins it before releasing the card.
#[cfg(test)]
pub(super) fn run_hardware<R, K, T>(
    args: Args,
    home: Option<&Path>,
    renderer: R,
    devices: Devices<K, T>,
    stop: Option<StopSignals>,
) -> Result<(), String>
where
    R: SceneRenderer,
    K: Kms,
    T: TouchSource + Send + 'static,
{
    run_hardware_with(
        args,
        home,
        || Ok(renderer),
        devices,
        stop,
        HardwareInput {
            stdin: io::stdin(),
            workers: Workers::new().map_err(|e| e.to_string())?,
            join_digitizer: false,
        },
    )
}

pub(super) struct HardwareInput<S> {
    pub stdin: S,
    pub workers: Workers,
    pub join_digitizer: bool,
}

pub(super) fn run_hardware_with<R, K, T>(
    args: Args,
    home: Option<&Path>,
    make_renderer: impl FnOnce() -> Result<R, String>,
    devices: Devices<K, T>,
    stop: Option<StopSignals>,
    input: HardwareInput<impl AsFd + Send + 'static>,
) -> Result<(), String>
where
    R: SceneRenderer,
    K: Kms,
    T: TouchSource + Send + 'static,
{
    args.validate_device_paths()?;
    let HardwareInput { stdin, mut workers, join_digitizer } = input;
    let Devices { sysfs, input_devices, open_card, open_touch } = devices;
    let mut drm = args.drm.as_ref().map(|target| start_display(target, &sysfs, open_card)).transpose()?;
    let run = (|| {
        let (tx, rx) = mpsc::channel();
        let digitizer = args
            .touch
            .as_ref()
            .map(|target| start_digitizer(target, &input_devices, open_touch))
            .transpose()?;
        let renderer = make_renderer()?;
        let action_runner = actions(args.emit, stop.as_ref());
        let _signal = stop
            .map(|stop| {
                let tx = tx.clone();
                stop.watch(move |result| {
                    let _ = tx.send(Message::Stop(result.map_err(|e| format!("cannot wait for stop: {e}"))));
                })
            })
            .transpose()
            .map_err(|e| format!("cannot start stop signal waiter: {e}"))?;
        if let Some(touch) = digitizer {
            start_touches(touch, &tx, &mut workers, join_digitizer)?;
        } else {
            let lines = LineTouches::new(BufReader::new(StopRead(stdin, workers.stop_reader())));
            start_touches(lines, &tx, &mut workers, true)?;
        }
        let theme = follow_keys_and_theme(args.keys, home, &tx, &mut workers);
        let config = ConfigState::start(home, theme, &tx, &mut workers);
        drop(tx);
        if let Some(display) = &mut drm {
            run_daemon(renderer, display, config, action_runner, rx)
        } else {
            let display = SimDisplay::new(BarSize::MACBOOK_PRO, args.sim_dir).map_err(|e| e.to_string())?;
            run_daemon(renderer, display, config, action_runner, rx)
        }
    })();
    drop(workers);
    let restored = drm.as_mut().map(DrmDisplay::restore).transpose().map_err(|e| e.to_string());
    match (run, restored) {
        (Err(run), Err(restore)) => Err(format!("{run}. {restore}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(()), Ok(_)) => Ok(()),
    }
}

fn start_display<K: Kms>(
    target: &Selection,
    sysfs: &Path,
    open: OpenCard<K>,
) -> Result<DrmDisplay<K>, String> {
    let path = match target {
        Selection::Auto => find_touch_bar_card(sysfs),
        Selection::Path(path) => check_touch_bar_card(sysfs, path),
    }
    .map_err(|e| e.to_string())?;
    let (card, node) = open(&path).map_err(|e| format!("cannot open card {}: {e}", path.display()))?;
    confirm_touch_bar_node(sysfs, &path, node).map_err(|e| e.to_string())?;
    DrmDisplay::start(card, BarSize::MACBOOK_PRO).map_err(|e| e.to_string())
}

fn start_digitizer<T>(target: &Selection, input_devices: &Path, open: OpenTouch<T>) -> Result<T, String> {
    let input = fs::read_to_string(input_devices)
        .map_err(|e| format!("cannot read {}: {e}", input_devices.display()))?;
    let path = match target {
        Selection::Auto => find_digitizer(&input),
        Selection::Path(path) => check_digitizer(&input, path),
    }
    .map_err(|e| e.to_string())?;
    open(&path)
}

fn start_touches(
    mut touches: impl TouchSource + Send + 'static,
    tx: &mpsc::Sender<Message>,
    workers: &mut Workers,
    join: bool,
) -> Result<(), String> {
    let tx = tx.clone();
    workers
        .spawn("touch reader", join, move || {
            let result = forward_touches(&mut touches, &tx).map_err(|e| e.to_string());
            let _ = tx.send(Message::Stop(result));
        })
        .map_err(|e| format!("cannot start touch reader: {e}"))
}
