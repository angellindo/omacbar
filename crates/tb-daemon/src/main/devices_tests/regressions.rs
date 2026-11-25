use std::io::{Seek, SeekFrom};
use std::os::fd::BorrowedFd;
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rustix::fs::{CWD, Mode, fcntl_getfl, mkfifoat};
use tb_daemon::fake::KmsLog;

use super::*;

const CHILD: &str = "TB_DEVICE_REGRESSION_CHILD";
const LIMIT: Duration = Duration::from_secs(10);

#[test]
fn a_stop_after_stdin_readiness_is_lost_restores_once_without_changing_the_callers_flags_or_mask() {
    run_in_child("stdin", &[""]);
}

#[test]
fn a_recording_fifo_without_a_reader_does_not_block_stopping_the_display() {
    run_in_child("emit", &["record.fifo", "absent/record.bin"]);
}

#[test]
fn auto_refuses_indeterminate_touch_bar_metadata_before_opening_any_card() {
    let root = fake_card("uncertain");
    let sysfs = root.join("sys");
    let usb = sysfs.join("devices/usb1/1-4");
    fs::create_dir_all(usb.join("1-4:1.2/drm/card2")).unwrap();
    symlink("drivers/appletbdrm", usb.join("1-4:1.2/driver")).unwrap();
    symlink("../..", usb.join("1-4:1.2/drm/card2/device")).unwrap();
    symlink(usb.join("1-4:1.2/drm/card2"), sysfs.join("class/drm/card2")).unwrap();
    fs::write(usb.join("idVendor"), "05ac\n").unwrap();
    let product = usb.join("idProduct");
    for bad in [None, Some(&b"\xff"[..]), Some(&b"garbage"[..])] {
        if let Some(bad) = bad {
            fs::write(&product, bad).unwrap();
        }
        let (input, _writer) = io::pipe().unwrap();
        let args = parse_args(["--drm", "auto"].map(str::to_owned).into_iter()).unwrap();
        let error = run_with_devices(&root, args, None, input, None).unwrap_err();
        assert!(error.contains("idProduct") && error.contains("card2"), "{error}");
    }
    assert_eq!(
        tb_daemon::check_touch_bar_card(&sysfs, Path::new("/dev/dri/card1")).unwrap(),
        Path::new("/dev/dri/card1")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn drm_reads_stdin_from_the_inherited_file_offset_or_a_socket() {
    let root = fake_card("stdin-file-and-socket");
    let path = root.join("touches");
    let touches = b"down 906\nup\n";
    fs::write(&path, [b"skip\n", &touches[..]].concat()).unwrap();
    let mut file = fs::File::open(path).unwrap();
    file.seek(SeekFrom::Start(5)).unwrap();
    let (stream, mut writer) = UnixStream::pair().unwrap();
    writer.write_all(touches).unwrap();
    drop(writer);

    for input in [Box::new(file) as Box<dyn AsFd + Send>, Box::new(stream)] {
        let kms = RecordingKms::touch_bar();
        let log = kms.log();
        let args = parse_args(["--drm", "auto"].map(str::to_owned).into_iter()).unwrap();
        assert_eq!(run_with_devices(&root, args, Some(kms), input, None), Ok(()));
        assert_eq!(log.calls().iter().filter(|call| matches!(call, KmsCall::Dirty { .. })).count(), 2);
    }
    fs::remove_dir_all(root).unwrap();
}

fn run_in_child(case: &str, destinations: &[&str]) {
    let root = fake_card(case);
    if case == "emit" {
        mkfifoat(CWD, root.join("record.fifo"), Mode::RUSR | Mode::WUSR).unwrap();
    }
    for destination in destinations {
        let output = Command::new("timeout")
            .args(["--signal=KILL", "10s", "env", "--block-signal=HUP,INT,TERM,USR1"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "devices_tests::regressions::device_run_child",
                "--exact",
                "--include-ignored",
                "--nocapture",
            ])
            .env(CHILD, &root)
            .env("TB_DEVICE_REGRESSION_CASE", case)
            .env("TB_DEVICE_RECORDING_DESTINATION", destination)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{}\n{stderr}", String::from_utf8_lossy(&output.stdout));
        if case == "emit" {
            assert!(stderr.contains(destination) && stderr.contains("key actions do nothing"), "{stderr}");
            assert!(!stderr.contains("/dev/uinput"), "{stderr}");
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "run only in an owned subprocess with stop signals blocked"]
fn device_run_child() {
    let Some(root) = std::env::var_os(CHILD).map(PathBuf::from) else { return };
    let mask = caller_mask();
    let stop = block_stop_signals().unwrap();
    let (input, mut writer) = io::pipe().unwrap();
    let witness = rustix::io::dup(&input).unwrap();
    let flags = fcntl_getfl(&witness).unwrap();
    let drained = Arc::new(AtomicBool::new(false));
    let kms = RecordingKms::touch_bar();
    let log = kms.log();
    let stdin_case = std::env::var_os("TB_DEVICE_REGRESSION_CASE").unwrap() == "stdin";
    let mut args = parse_args(["--drm", "auto"].map(str::to_owned).into_iter()).unwrap();
    let sent = if stdin_case {
        writer.write_all(b"x").unwrap();
        signal_after_present(log.clone(), Some(Arc::clone(&drained)))
    } else {
        let destination = std::env::var_os("TB_DEVICE_RECORDING_DESTINATION").unwrap();
        args.emit = Some(Emit::Path(root.join(destination)));
        writer.write_all(b"down 906\nup\n").unwrap();
        signal_after_present(log.clone(), None)
    };
    let input = DrainAfterPoll { input, drained, worker_calls: AtomicBool::new(false), enabled: stdin_case };
    let result = run_with_devices(&root, args, Some(kms), input, Some(stop));
    sent.join().unwrap();
    assert_eq!(result, Ok(()));
    assert_eq!(caller_mask(), mask);
    assert_eq!(fcntl_getfl(&witness).unwrap(), flags);
    let calls = log.calls();
    assert_eq!(calls.iter().filter(|call| matches!(call, KmsCall::SetCrtc { fb: 0, .. })).count(), 1);
    assert_eq!(calls.iter().filter(|call| matches!(call, KmsCall::DropMaster)).count(), 1);
    assert_eq!(calls.last(), Some(&KmsCall::Close));
    drop(writer);
}

fn caller_mask() -> String {
    fs::read_to_string("/proc/thread-self/status")
        .unwrap()
        .lines()
        .find(|line| line.starts_with("SigBlk:"))
        .unwrap()
        .to_owned()
}

fn signal_after_present(log: KmsLog, drained: Option<Arc<AtomicBool>>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let deadline = Instant::now() + LIMIT;
        let mut have_master = false;
        loop {
            let calls = log.calls();
            if !have_master && calls.contains(&KmsCall::SetMaster) {
                have_master = true;
                crate::log::info("simulated DRM master acquired");
            }
            let ready = match &drained {
                Some(drained) => drained.load(Ordering::Acquire),
                None => calls.iter().filter(|call| matches!(call, KmsCall::Dirty { .. })).count() == 2,
            };
            if ready && calls.iter().any(|call| matches!(call, KmsCall::SetCrtc { fb, .. } if *fb != 0)) {
                crate::log::info("stop requested after the first present");
                assert!(
                    Command::new("kill")
                        .args(["-TERM", &std::process::id().to_string()])
                        .status()
                        .unwrap()
                        .success()
                );
                return;
            }
            assert!(Instant::now() < deadline, "the run never reached the stop point");
            thread::sleep(Duration::from_millis(1));
        }
    })
}

// The worker first borrows the fd for poll, then borrows it to perform I/O. Drain the one
// byte before the second borrow returns, while leaving the writer open to keep EOF away.
struct DrainAfterPoll {
    input: PipeReader,
    drained: Arc<AtomicBool>,
    worker_calls: AtomicBool,
    enabled: bool,
}

impl AsFd for DrainAfterPoll {
    fn as_fd(&self) -> BorrowedFd<'_> {
        let fd = self.input.as_fd();
        if self.enabled
            && thread::current().name() == Some("touch reader")
            && self.worker_calls.swap(true, Ordering::Relaxed)
            && !self.drained.load(Ordering::Relaxed)
        {
            let mut byte = [0];
            assert_eq!(rustix::io::read(fd, &mut byte).unwrap(), 1);
            assert_eq!(&byte, b"x");
            self.drained.store(true, Ordering::Release);
        }
        fd
    }
}
