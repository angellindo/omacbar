#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use rustix::fs::{CWD, Mode, OFlags, mkfifoat, open};
use rustix::io::Errno;
use tb_daemon::{NodeId, confirm_digitizer_node, confirm_touch_bar_node};

#[test]
fn drm_confirmation_cannot_take_usb_identity_and_device_number_from_different_cards() {
    let sysfs = Sysfs::new("drm");
    sysfs.write("old/interface/card/dev", "226:2\n");
    sysfs.link("old/interface/card/device", "..");
    sysfs.link("old/interface/driver", "drivers/appletbdrm");
    sysfs.write("old/idProduct", "8600\n");
    sysfs.write("new/interface/card/dev", "226:1\n");
    sysfs.link("new/interface/card/device", "..");
    sysfs.link("new/interface/driver", "drivers/appletbdrm");
    sysfs.write("new/idVendor", "05ac\n");
    sysfs.write("new/idProduct", "8302\n");
    sysfs.link("class/drm/card1", "../../old/interface/card");

    let result = replace_during_read(
        &sysfs.0.join("old/idVendor"),
        b"05ac\n",
        || {
            fs::remove_file(sysfs.0.join("class/drm/card1")).unwrap();
            sysfs.link("class/drm/card1", "../../new/interface/card");
        },
        || {
            let node = NodeId { major: 226, minor: 1, char_device: true };
            confirm_touch_bar_node(&sysfs.0, Path::new("/dev/dri/card1"), node)
        },
    );
    assert!(result.is_err(), "old T1 identity and new device number must not confirm: {result:?}");
}

#[test]
fn digitizer_confirmation_cannot_take_device_number_and_input_identity_from_different_events() {
    let sysfs = Sysfs::new("input");
    sysfs.write("active/event/dev", "13:80\n");
    sysfs.link("active/event/device", "..");
    sysfs.write("active/id/vendor", "05ac\n");
    sysfs.write("active/id/product", "8302\n");
    sysfs.write("replacement/event/dev", "13:81\n");
    sysfs.link("replacement/event/device", "..");
    sysfs.write("replacement/id/bustype", "0003\n");
    sysfs.write("replacement/id/vendor", "05ac\n");
    sysfs.write("replacement/id/product", "8600\n");
    sysfs.link("class/input/event16", "../../active/event");

    let result = replace_during_read(
        &sysfs.0.join("active/id/bustype"),
        b"0003\n",
        || {
            fs::rename(sysfs.0.join("active"), sysfs.0.join("retired")).unwrap();
            fs::rename(sysfs.0.join("replacement"), sysfs.0.join("active")).unwrap();
        },
        || {
            let node = NodeId { major: 13, minor: 80, char_device: true };
            confirm_digitizer_node(&sysfs.0, Path::new("/dev/input/event16"), node)
        },
    );
    assert!(result.is_err(), "old device number and new T1 identity must not confirm: {result:?}");
}

// A FIFO pauses confirmation between attributes without relying on scheduling or real devices.
fn replace_during_read<T: Send>(
    fifo: &Path,
    attribute: &[u8],
    replace: impl FnOnce(),
    confirm: impl FnOnce() -> T + Send,
) -> T {
    mkfifoat(CWD, fifo, Mode::RUSR | Mode::WUSR).unwrap();
    thread::scope(|scope| {
        let reader = scope.spawn(confirm);
        let deadline = Instant::now() + Duration::from_secs(10);
        let writer = loop {
            match open(fifo, OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC, Mode::empty()) {
                Ok(writer) => break writer,
                Err(Errno::NXIO) if reader.is_finished() => return reader.join().unwrap(),
                Err(Errno::NXIO) if Instant::now() < deadline => thread::sleep(Duration::from_millis(1)),
                error => panic!("confirmation did not reach {}: {error:?}", fifo.display()),
            }
        };
        // On unwind, close the writer before waiting for the reader.
        let mut writer = File::from(writer);
        replace();
        writer.write_all(attribute).unwrap();
        drop(writer);
        reader.join().unwrap()
    })
}

struct Sysfs(PathBuf);

impl Sysfs {
    fn new(case: &str) -> Self {
        let root =
            Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sysfs-race-{case}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        Self(root)
    }

    fn write(&self, path: &str, value: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }

    fn link(&self, path: &str, target: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(target, path).unwrap();
    }
}

impl Drop for Sysfs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
