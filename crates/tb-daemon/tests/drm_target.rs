//! Choosing the Touch Bar card from sysfs, before any device node is opened, and confirming that
//! the node actually opened is still that card. Every sysfs tree here is built under cargo's temp
//! dir with the shape the kernel gives a USB DRM device (`class/drm/cardN` → the device's `drm/`
//! dir, `device` → the USB interface, its `driver` link, the USB device's `idVendor`/`idProduct`
//! above it, `dev` = `major:minor`). No test reads `/sys` or opens `/dev`.
//!
//! IDs: T1 iBridge display `05ac:8600`, T2 `05ac:8302` (research 0001, 0010; appletbdrm.c
//! L817–820). The driver name `appletbdrm` is the upstream one; that the patched T1 driver keeps
//! it is hardware-unverified (TB-10), and a mismatch is refused, never guessed around.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tb_daemon::find_touch_bar_card;

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tb_daemon::{CardKms, NodeId, TargetError, check_touch_bar_card, confirm_touch_bar_node};

const DRM_MAJOR: u32 = 226;

/// A fake sysfs root under cargo's temp dir.
struct Sysfs {
    root: PathBuf,
}

impl Sysfs {
    fn new(name: &str) -> Self {
        static RUNS: AtomicUsize = AtomicUsize::new(0);
        let n = RUNS.fetch_add(1, Ordering::Relaxed);
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("drm_target")
            .join(format!("{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("class/drm")).unwrap();
        Sysfs { root }
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn link(&self, rel: &str, target: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let _ = std::fs::remove_file(&p);
        symlink(target, p).unwrap();
    }

    /// A DRM card under a USB interface: `devices/usb1/<usb>/<usb>:1.2/drm/<card>`.
    fn usb_card(&self, card: &str, usb: &str, driver: Option<&str>, vendor: &str, product: &str) {
        let iface = format!("devices/usb1/{usb}/{usb}:1.2");
        self.write(&format!("devices/usb1/{usb}/idVendor"), &format!("{vendor}\n"));
        self.write(&format!("devices/usb1/{usb}/idProduct"), &format!("{product}\n"));
        if let Some(driver) = driver {
            std::fs::create_dir_all(self.root.join(format!("bus/usb/drivers/{driver}"))).unwrap();
            self.link(&format!("{iface}/driver"), &format!("../../../../bus/usb/drivers/{driver}"));
        }
        self.card_dir(card, &iface, &format!("../../../{usb}:1.2"));
    }

    /// A DRM card under a PCI GPU: `devices/pci0000:00/<slot>/drm/<card>`.
    fn pci_card(&self, card: &str, slot: &str, driver: &str) {
        let dev = self.pci_device(slot, driver);
        self.card_dir(card, &dev, &format!("../../../{slot}"));
        // Its connector and render node sit next to it in class/drm.
        let connector = format!("{dev}/drm/{card}/{card}-eDP-1");
        self.write(&format!("{connector}/status"), "connected\n");
        self.link(&format!("class/drm/{card}-eDP-1"), &format!("../../{connector}"));
        self.write(&format!("{dev}/drm/renderD128/dev"), "226:128\n");
        self.link("class/drm/renderD128", &format!("../../{dev}/drm/renderD128"));
    }

    /// A PCI GPU bound to `driver`, without a card yet; returns its sysfs dir.
    fn pci_device(&self, slot: &str, driver: &str) -> String {
        let dev = format!("devices/pci0000:00/{slot}");
        self.write(&format!("{dev}/vendor"), "0x8086\n");
        std::fs::create_dir_all(self.root.join(format!("bus/pci/drivers/{driver}"))).unwrap();
        self.link(&format!("{dev}/driver"), &format!("../../../bus/pci/drivers/{driver}"));
        dev
    }

    fn card_dir(&self, card: &str, parent: &str, device_link: &str) {
        let minor: u32 = card.trim_start_matches("card").parse().unwrap();
        let dir = format!("{parent}/drm/{card}");
        self.write(&format!("{dir}/dev"), &format!("{DRM_MAJOR}:{minor}\n"));
        self.link(&format!("{dir}/device"), device_link);
        self.link(&format!("class/drm/{card}"), &format!("../../{dir}"));
    }

    /// The common laptop: an Intel GPU as card0 and the T1 bar as card1.
    fn laptop(name: &str) -> Self {
        let s = Sysfs::new(name);
        s.pci_card("card0", "0000:00:02.0", "i915");
        s.usb_card("card1", "1-3", Some("appletbdrm"), "05ac", "8600");
        s
    }
}

fn dev(card: &str) -> PathBuf {
    Path::new("/dev/dri").join(card)
}

fn char_node(minor: u32) -> NodeId {
    NodeId { major: DRM_MAJOR, minor, char_device: true }
}

// ---------- auto ----------

#[test]
fn auto_finds_the_one_touch_bar_card_among_desktop_gpus_connectors_and_render_nodes() {
    let s = Sysfs::laptop("one");
    s.pci_card("card2", "0000:01:00.0", "amdgpu");
    assert_eq!(find_touch_bar_card(&s.root).unwrap(), dev("card1"));
}

#[test]
fn auto_with_two_touch_bar_cards_is_ambiguous_and_lists_both() {
    let s = Sysfs::laptop("two");
    s.usb_card("card2", "1-4", Some("appletbdrm"), "05ac", "8600");
    match find_touch_bar_card(&s.root) {
        Err(TargetError::Ambiguous(mut cards)) => {
            cards.sort();
            assert_eq!(cards, [dev("card1"), dev("card2")]);
        }
        other => panic!("two bars must be ambiguous, got {other:?}"),
    }
}

#[test]
fn auto_never_falls_back_to_a_desktop_gpu_a_t2_bar_or_an_unbound_card() {
    let s = Sysfs::new("none");
    s.pci_card("card0", "0000:00:02.0", "i915");
    s.pci_card("card2", "0000:01:00.0", "amdgpu");
    s.usb_card("card1", "1-3", Some("appletbdrm"), "05ac", "8302");
    s.usb_card("card3", "1-4", None, "05ac", "8600");
    let got = find_touch_bar_card(&s.root);
    assert!(matches!(got, Err(TargetError::NotFound)), "{got:?}");
}

#[test]
fn auto_with_no_drm_class_finds_nothing() {
    let s = Sysfs::new("empty");
    std::fs::remove_dir_all(s.root.join("class/drm")).unwrap();
    assert!(find_touch_bar_card(&s.root).is_err());
}

// ---------- explicit path ----------

#[test]
fn an_explicit_touch_bar_card_path_is_accepted() {
    let s = Sysfs::laptop("explicit");
    assert_eq!(check_touch_bar_card(&s.root, &dev("card1")).unwrap(), dev("card1"));
}

#[test]
fn an_explicit_desktop_gpu_is_refused_as_the_wrong_driver() {
    let s = Sysfs::laptop("desktop");
    s.pci_card("card2", "0000:01:00.0", "amdgpu");
    for (card, want) in [("card0", "i915"), ("card2", "amdgpu")] {
        match check_touch_bar_card(&s.root, &dev(card)) {
            Err(TargetError::WrongDriver { driver }) => assert_eq!(driver, want),
            other => panic!("{card} must be refused as {want}, got {other:?}"),
        }
    }
}

#[test]
fn an_explicit_t2_bar_is_refused_as_the_wrong_device() {
    let s = Sysfs::new("t2");
    s.usb_card("card1", "1-3", Some("appletbdrm"), "05ac", "8302");
    match check_touch_bar_card(&s.root, &dev("card1")) {
        Err(TargetError::WrongDevice { vendor, product }) => {
            assert_eq!((vendor.as_str(), product.as_str()), ("05ac", "8302"));
        }
        other => panic!("a T2 bar is not the T1 target, got {other:?}"),
    }
}

#[test]
fn an_explicit_card_that_is_unbound_or_absent_from_sysfs_is_refused() {
    let s = Sysfs::laptop("unbound");
    s.usb_card("card2", "1-4", None, "05ac", "8600");
    for card in ["card2", "card7"] {
        let got = check_touch_bar_card(&s.root, &dev(card));
        assert!(got.is_err(), "{card}: {got:?}");
    }
}

/// Lexical check first: the sysfs root does not even exist, so any answer other than
/// `NotACardPath` means sysfs was consulted (or a non-card path was accepted).
#[test]
fn paths_that_are_not_a_dev_dri_card_node_are_refused_before_any_sysfs_read() {
    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("drm_target/no-such-sysfs");
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("drm_target/card1");
    let paths = [
        tmp.as_path(),
        Path::new("card1"),
        Path::new("/dev/dri/renderD128"),
        Path::new("/dev/dri/card1-eDP-1"),
        Path::new("/dev/dri/by-path/pci-0000:00:02.0-card"),
        Path::new("/dev/dri/../dri/card1"),
        Path::new("/dev/dri/card"),
        Path::new("/dev/dri/cardX"),
        Path::new("/dev/fb0"),
        Path::new("/dev/dfr0"),
    ];
    for p in paths {
        let got = check_touch_bar_card(&missing, p);
        assert!(matches!(got, Err(TargetError::NotACardPath { .. })), "{}: {got:?}", p.display());
    }
}

// ---------- confirming the opened node ----------

#[test]
fn the_opened_node_must_be_the_character_device_sysfs_names_for_the_card() {
    let s = Sysfs::laptop("confirm");
    let card = dev("card1");
    assert_eq!(confirm_touch_bar_node(&s.root, &card, char_node(1)).unwrap(), card);
    for (what, node) in [
        ("another minor", char_node(0)),
        ("a render node", char_node(128)),
        ("another major", NodeId { major: 13, minor: 1, char_device: true }),
        ("not a character device", NodeId { major: DRM_MAJOR, minor: 1, char_device: false }),
    ] {
        let got = confirm_touch_bar_node(&s.root, &card, node);
        assert!(matches!(got, Err(TargetError::Changed { .. })), "{what}: {got:?}");
    }
}

/// Between discovery and open the card may be renumbered: card1 now names the desktop GPU, or a
/// T2 bar, or nothing. Confirmation re-reads sysfs and refuses; it never keeps the stale answer.
#[test]
fn a_card_renumbered_after_discovery_is_refused_at_confirmation() {
    let card = dev("card1");

    let s = Sysfs::laptop("renumbered-gpu");
    assert_eq!(find_touch_bar_card(&s.root).unwrap(), card);
    // card1 (same 226:1) now belongs to a desktop GPU.
    let gpu = s.pci_device("0000:01:00.0", "amdgpu");
    s.card_dir("card1", &gpu, "../../../0000:01:00.0");
    let got = confirm_touch_bar_node(&s.root, &card, char_node(1));
    assert!(matches!(got, Err(TargetError::WrongDriver { .. })), "{got:?}");

    let s = Sysfs::laptop("renumbered-t2");
    assert_eq!(find_touch_bar_card(&s.root).unwrap(), card);
    s.write("devices/usb1/1-3/idProduct", "8302\n");
    let got = confirm_touch_bar_node(&s.root, &card, char_node(1));
    assert!(matches!(got, Err(TargetError::WrongDevice { .. })), "{got:?}");

    let s = Sysfs::laptop("renumbered-gone");
    assert_eq!(find_touch_bar_card(&s.root).unwrap(), card);
    std::fs::remove_file(s.root.join("class/drm/card1")).unwrap();
    assert!(confirm_touch_bar_node(&s.root, &card, char_node(1)).is_err());
}

#[test]
fn a_second_touch_bar_appearing_after_discovery_does_not_change_the_confirmed_card() {
    // Confirmation is about the chosen node only: card1 stays valid even if card2 appears.
    let s = Sysfs::laptop("second-appears");
    let card = find_touch_bar_card(&s.root).unwrap();
    s.usb_card("card2", "1-4", Some("appletbdrm"), "05ac", "8600");
    assert_eq!(confirm_touch_bar_node(&s.root, &card, char_node(1)).unwrap(), card);
    assert!(matches!(find_touch_bar_card(&s.root), Err(TargetError::Ambiguous(_))));
}

/// The production adapter's own identity read: an ordinary file opened through `CardKms` is not
/// a character device, so it can never confirm as the card, whatever sysfs says.
#[test]
fn an_ordinary_file_opened_by_the_real_adapter_never_confirms_as_the_card() {
    let s = Sysfs::laptop("real-adapter");
    let file = s.root.join("fake-node");
    std::fs::write(&file, b"").unwrap();
    let kms = CardKms::open(&file).unwrap();
    let node = kms.node_id().unwrap();
    assert!(!node.char_device, "{node:?}");
    let got = confirm_touch_bar_node(&s.root, &dev("card1"), node);
    assert!(matches!(got, Err(TargetError::Changed { .. })), "{got:?}");
}
