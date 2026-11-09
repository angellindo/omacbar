//! Runs the `touchbar-customizer` binary headless (`--snap`, `--tree`, `--script`, `--scene`)
//! with `HOME` set to a temp home under cargo's temp dir, never the real one, and reads the
//! element tree it dumps. Needs a GPU adapter (lavapipe is fine).
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// An empty directory for one test, under cargo's temp dir.
pub fn fresh_dir(suite: &str, test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(suite).join(test);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// The flow script that drags F3 before F1 and saves.
pub fn drag_f3_before_f1_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/drag-f3-before-f1.json")
}

/// One run of the configurator with `HOME` = `home` and `args`; it must exit 0.
pub fn run(home: &Path, args: &[&std::ffi::OsStr]) {
    let out = Command::new(env!("CARGO_BIN_EXE_touchbar-customizer"))
        .args(args)
        .env("HOME", home)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "touchbar-customizer {args:?} exit {:?}, stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `--snap <dir>/<name>.png --tree <dir>/<name>.json` plus `extra`; returns the parsed tree.
/// The PNG must be written and not empty; with `TB_WRITE_SNAPSHOTS=1` it is copied to
/// `out/app-<name>.png` for a human to look at.
pub fn snap(home: &Path, dir: &Path, name: &str, extra: &[&std::ffi::OsStr]) -> Vec<Element> {
    let (png, tree) = (dir.join(format!("{name}.png")), dir.join(format!("{name}.json")));
    let mut args: Vec<&std::ffi::OsStr> =
        vec!["--snap".as_ref(), png.as_os_str(), "--tree".as_ref(), tree.as_os_str()];
    args.extend_from_slice(extra);
    run(home, &args);
    assert!(fs::metadata(&png).unwrap().len() > 0, "{} is written", png.display());
    if std::env::var_os("TB_WRITE_SNAPSHOTS").is_some() {
        let dest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out");
        fs::create_dir_all(&dest).unwrap();
        fs::copy(&png, dest.join(format!("app-{name}.png"))).unwrap();
    }
    elements(&fs::read_to_string(&tree).unwrap())
}

/// One element of a `--tree` dump.
#[derive(Debug, Clone)]
pub struct Element {
    pub key: Option<String>,
    pub path: String,
    pub rect: [f32; 4],
    pub text: Option<String>,
}

/// The `elements` of a `--tree` dump (flat objects: no nested objects inside one element).
pub fn elements(tree: &str) -> Vec<Element> {
    let start = tree.find("\"elements\"").expect("the tree lists its elements");
    let rest = &tree[start..];
    let open = rest.find('[').unwrap();
    let bytes = rest.as_bytes();
    let (mut out, mut depth, mut in_str, mut esc, mut obj_start) = (Vec::new(), 0usize, false, false, 0usize);
    for (i, &b) in bytes.iter().enumerate().skip(open + 1) {
        if in_str {
            match (esc, b) {
                (true, _) => esc = false,
                (false, b'\\') => esc = true,
                (false, b'"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    obj_start = i;
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    out.push(element(&rest[obj_start..=i]));
                }
            }
            b']' if depth == 0 => break,
            _ => {}
        }
    }
    out
}

fn element(obj: &str) -> Element {
    Element {
        key: string_field(obj, "key"),
        path: string_field(obj, "path").unwrap_or_default(),
        rect: rect_field(obj),
        text: string_field(obj, "text"),
    }
}

fn string_field(obj: &str, name: &str) -> Option<String> {
    let tag = format!("\"{name}\": \"");
    let at = obj.find(&tag)? + tag.len();
    let mut out = String::new();
    let mut chars = obj[at..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn rect_field(obj: &str) -> [f32; 4] {
    let tag = "\"rect\": [";
    let at = obj.find(tag).expect("every element has a rect") + tag.len();
    let end = at + obj[at..].find(']').unwrap();
    let nums: Vec<f32> = obj[at..end].split(',').map(|n| n.trim().parse().unwrap()).collect();
    [nums[0], nums[1], nums[2], nums[3]]
}

/// The element keyed `key`.
pub fn keyed<'a>(tree: &'a [Element], key: &str) -> Option<&'a Element> {
    tree.iter().find(|e| e.key.as_deref() == Some(key))
}

/// What the element keyed `key` shows: its own text, else the texts inside it joined by spaces.
pub fn text_of(tree: &[Element], key: &str) -> Option<String> {
    let e = keyed(tree, key)?;
    if let Some(t) = &e.text {
        return Some(t.clone());
    }
    let inside = format!("{}/", e.path);
    let texts: Vec<&str> =
        tree.iter().filter(|d| d.path.starts_with(&inside)).filter_map(|d| d.text.as_deref()).collect();
    (!texts.is_empty()).then(|| texts.join(" "))
}

/// The x of the element keyed `key`.
pub fn x_of(tree: &[Element], key: &str) -> f32 {
    keyed(tree, key).unwrap_or_else(|| panic!("no element keyed {key} in the tree")).rect[0]
}
