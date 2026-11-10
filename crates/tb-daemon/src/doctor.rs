//! JSON-only, read-only native doctor boundary. No event devices or config actions are opened.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use tb_config::{ActiveTheme, ConfigFile, ConfigFileError, default_path};
use tb_core::config::ThemeSource;
use tb_core::{BarSize, BarTheme, SpacingRules};

#[derive(Serialize)]
struct Check {
    id: &'static str,
    status: &'static str,
    required: bool,
    message: String,
    evidence: Vec<String>,
    next: Vec<&'static str>,
}

impl Check {
    fn new(id: &'static str, status: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            status,
            required: id.starts_with("runtime."),
            message: message.into(),
            evidence: Vec::new(),
            next: Vec::new(),
        }
    }
}

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    checks: Vec<Check>,
}

fn main() -> ExitCode {
    let Some((kind, home)) = arguments(&std::env::args().skip(1).collect::<Vec<_>>()) else {
        return ExitCode::from(2);
    };
    let checks = match kind.as_str() {
        "config" => home.map_or_else(Vec::new, |home| config_checks(&home)),
        "gpu" => vec![gpu_check()],
        "input" => input_checks(),
        _ => return ExitCode::from(2),
    };
    let report = Report { schema_version: 1, checks };
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if serde_json::to_writer(&mut output, &report).is_err() || output.write_all(b"\n").is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn arguments(args: &[String]) -> Option<(String, Option<PathBuf>)> {
    match args {
        [flag, kind] if flag == "--check" && matches!(kind.as_str(), "gpu" | "input") => {
            Some((kind.clone(), None))
        }
        [flag, kind, home_flag, home]
            if flag == "--check"
                && kind == "config"
                && home_flag == "--home"
                && Path::new(home).is_absolute() =>
        {
            Some((kind.clone(), Some(PathBuf::from(home))))
        }
        _ => None,
    }
}

fn config_checks(home: &Path) -> Vec<Check> {
    let active = ActiveTheme::under_home(home).load();
    let mut file = ConfigFile::new(default_path(home));
    let missing = match std::fs::symlink_metadata(file.path()) {
        Ok(_) => false,
        Err(e) => e.kind() == io::ErrorKind::NotFound,
    };
    let result = if missing {
        file.check_active_theme(&active.theme).map(|()| active.theme.clone())
    } else {
        file.reload_resolved(&active.theme)
    };
    let mut theme = Check::new("runtime.theme", "ok", "Selected theme resolved");
    let config = match result {
        Ok(_) => {
            if matches!(file.config().theme, ThemeSource::Omarchy) && active.fallback.is_some() {
                theme.status = "warning";
                theme.message = "Active theme unavailable or malformed; using bundled Tokyo Night".into();
            }
            Check::new(
                "runtime.config",
                if missing { "warning" } else { "ok" },
                if missing {
                    "Config missing: in-memory first-run defaults validated; no file created"
                } else {
                    "Config parsed and all domain issues checked"
                },
            )
        }
        Err(error) => rejected_config(&error, &mut theme),
    };
    vec![config, theme]
}

fn rejected_config(error: &ConfigFileError, theme: &mut Check) -> Check {
    let mut config = Check::new("runtime.config", "failed", "Config could not be validated");
    match error {
        ConfigFileError::Invalid(issues) => {
            // Issue text can include command values, unknown keys, paths and syntax excerpts.
            let syntax =
                issues.iter().filter(|i| matches!(i, tb_core::config::ConfigIssue::Syntax(_))).count();
            config
                .evidence
                .push(format!("syntax issues: {syntax}; domain issues: {}", issues.len() - syntax));
            theme.status = "skipped";
            theme.message = "Config rejected; selected theme readiness unverified".into();
        }
        ConfigFileError::Theme(_) => {
            theme.status = "failed";
            theme.message = "Explicitly selected theme unreadable or malformed; no fallback accepted".into();
        }
        _ => {
            config.message = "Existing config unreadable; no first-run default assumed".into();
            theme.status = "skipped";
            theme.message = "Config unreadable; selected theme unverified".into();
        }
    }
    config.next.push("Review config locally in the configurator; keep command values private");
    config
}

fn gpu_check() -> Check {
    let theme = BarTheme::omarchy_fallback();
    let config = tb_core::config::Config::default();
    let scene = tb_render::build_scene(
        &config.layout.layers[0],
        &theme,
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    );
    if let (Ok(scene), Ok(mut renderer)) = (scene, tb_render::Renderer::new()) {
        let frame = renderer.render(&scene);
        if frame.width == 2170 && frame.height == 60 && frame.rgba.len() == 2170 * 60 * 4 {
            return Check::new(
                "runtime.gpu",
                "ok",
                "Rune rendered and read back a 2170x60 RGBA frame in memory",
            );
        }
    }
    let mut check =
        Check::new("runtime.gpu", "failed", "Rune GPU render/readback failed; no usable frame verified");
    check.next.push("Review local Vulkan driver setup; software Vulkan can support headless rendering");
    check
}

fn input_checks() -> Vec<Check> {
    let keyboard = std::fs::read_to_string("/proc/bus/input/devices")
        .ok()
        .and_then(|text| tb_daemon::find_fn_keyboard(&text));
    let readable =
        keyboard.is_some_and(|kb| rustix::fs::access(kb.path, rustix::fs::Access::READ_OK).is_ok());
    let writable = rustix::fs::access("/dev/uinput", rustix::fs::Access::WRITE_OK).is_ok();
    vec![
        Check::new(
            "input.fn",
            if readable { "ok" } else { "warning" },
            if readable {
                "Fn capability and event-node read-access metadata found; manual Fn test remains necessary"
            } else {
                "Fn capability/read-access unavailable; default layer remains usable; manual Fn test required"
            },
        ),
        Check::new(
            "input.uinput",
            if writable { "ok" } else { "warning" },
            if writable {
                "uinput write-access metadata available; no device opened or keys emitted"
            } else {
                "uinput unavailable by metadata; file emission remains usable"
            },
        ),
    ]
}
