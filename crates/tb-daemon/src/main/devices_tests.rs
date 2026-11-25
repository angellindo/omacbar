#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::io::{self, PipeReader, Write};
use std::os::unix::fs::{DirBuilderExt, symlink};
use std::time::Duration;

use super::*;
use tb_daemon::NodeId;
use tb_daemon::fake::{FlatRenderer, KmsCall, KmsStep, RecordingKms};

#[path = "devices_tests/regressions.rs"]
mod regressions;

#[test]
fn drm_without_a_digitizer_presents_and_restores_at_stdin_eof() {
    let root = fake_card("stdin-eof");
    let kms = RecordingKms::touch_bar();
    let log = kms.log();
    let (input, mut writer) = io::pipe().unwrap();
    writer.write_all(b"down 906\nup\n").unwrap();
    drop(writer);
    let result = run_drm_with_stdin(&root, kms, input);
    fs::remove_dir_all(&root).unwrap();

    assert_eq!(result, Ok(()));
    let mut display_changes = log
        .calls()
        .into_iter()
        .filter(|call| matches!(call, KmsCall::SetCrtc { .. } | KmsCall::Dirty { .. } | KmsCall::Close));
    assert!(matches!(display_changes.next(), Some(KmsCall::SetCrtc { fb, .. }) if fb != 0));
    assert!(matches!(display_changes.next(), Some(KmsCall::Dirty { .. })));
    assert!(matches!(display_changes.next(), Some(KmsCall::Dirty { .. })));
    assert!(matches!(display_changes.next(), Some(KmsCall::SetCrtc { fb: 0, .. })));
    assert!(matches!(display_changes.next(), Some(KmsCall::Close)));
    assert!(display_changes.next().is_none());
}

#[test]
fn a_failed_present_cancels_stdin_even_while_the_pipe_is_open() {
    let root = fake_card("stdin-blocked");
    let kms = RecordingKms::touch_bar().fail_at(KmsStep::Dirty, rustix::io::Errno::IO.raw_os_error());
    let log = kms.log();
    let (input, mut writer) = io::pipe().unwrap();
    writer.write_all(b"down 906\n").unwrap();
    let (finished, result) = mpsc::channel();
    let sysfs_root = root.clone();
    let run = thread::spawn(move || {
        let _ = finished.send(run_drm_with_stdin(&sysfs_root, kms, input));
    });
    let ending = result.recv_timeout(Duration::from_secs(10));
    drop(writer);
    if ending.is_ok() {
        run.join().unwrap();
    }
    fs::remove_dir_all(root).unwrap();

    assert!(ending.unwrap().unwrap_err().contains("display device error"));
    let calls = log.calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, KmsCall::SetCrtc { fb: 0, .. })).count(), 1);
    assert_eq!(calls.last(), Some(&KmsCall::Close));
}

fn fake_card(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("touchbard-drm-{label}-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    fs::create_dir_all(root.join("home/.local/state/omarchy/current")).unwrap();
    let sysfs = root.join("sys");
    let usb = sysfs.join("devices/usb1/1-3");
    let interface = usb.join("1-3:1.2");
    let card = usb.join("drm/card1");
    fs::create_dir_all(&interface).unwrap();
    fs::create_dir_all(&card).unwrap();
    fs::create_dir_all(sysfs.join("class/drm")).unwrap();
    fs::write(usb.join("idVendor"), "05ac\n").unwrap();
    fs::write(usb.join("idProduct"), "8600\n").unwrap();
    fs::write(card.join("dev"), "226:1\n").unwrap();
    symlink("../../../drivers/appletbdrm", interface.join("driver")).unwrap();
    symlink("../../1-3:1.2", card.join("device")).unwrap();
    symlink(&card, sysfs.join("class/drm/card1")).unwrap();
    root
}

fn run_drm_with_stdin(root: &Path, kms: RecordingKms, input: PipeReader) -> Result<(), String> {
    let args = parse_args(["--drm", "auto"].map(str::to_owned).into_iter()).unwrap();
    run_with_devices(root, args, Some(kms), input, None)
}

fn run_with_devices(
    root: &Path,
    args: Args,
    kms: Option<RecordingKms>,
    input: impl AsFd + Send + 'static,
    stop: Option<StopSignals>,
) -> Result<(), String> {
    let devices = Devices::<_, LineTouches<io::Cursor<Vec<u8>>>> {
        sysfs: root.join("sys"),
        input_devices: root.join("unused"),
        open_card: Box::new(move |path| {
            assert_eq!(path, Path::new("/dev/dri/card1"));
            Ok((
                kms.expect("startup must not open a card"),
                NodeId { major: 226, minor: 1, char_device: true },
            ))
        }),
        open_touch: Box::new(|_| panic!("--drm without --touch must read stdin")),
    };
    run_hardware_with(
        args,
        Some(&root.join("home")),
        || Ok(FlatRenderer),
        devices,
        stop,
        HardwareInput { stdin: input, workers: Workers::new().unwrap(), join_digitizer: false },
    )
}
