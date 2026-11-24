//! `touchbar-recover`: lists, verifies and restores the generations of the config file from a
//! terminal (ADR-0015). It needs no display, no GPU and no valid config: it never parses a file
//! to decide anything, and restores through the same compare-and-swap as every other write.
//!
//! Exit codes: 0 done, 1 refused or failed, 2 usage, 3 conflict (the file changed; run again).

mod output;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tb_config::{
    ActiveTheme, BackupError, Backups, ConfigFile, ConfigFileError, Generation, GenerationId, GenerationKind,
    Health, default_path,
};
use tb_core::config::ThemeSource;

const USAGE: &str = "usage: touchbar-recover list | verify <id> | restore <id> [--remove-file]";

enum Command {
    List,
    Verify(String),
    Restore { id: String, remove: bool },
}

/// Where the store and the config file are, from the environment read at startup (R12).
struct Profile {
    home: PathBuf,
    store: PathBuf,
    config: PathBuf,
}

/// Why a run stopped: a refusal or failure (1), or a conflict (3).
enum Stop {
    Refused(String),
    Conflict(String),
}

fn main() -> ExitCode {
    let Some(command) = parse_args(std::env::args_os().skip(1)) else {
        output::error(USAGE);
        return ExitCode::from(2);
    };
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
    let state_home = std::env::var_os("XDG_STATE_HOME");
    let result = profile(home.as_deref(), state_home.as_deref()).and_then(|profile| run(&command, &profile));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Stop::Refused(message)) => {
            output::error(message);
            ExitCode::FAILURE
        }
        Err(Stop::Conflict(message)) => {
            output::error(message);
            ExitCode::from(3)
        }
    }
}

/// `list`, `verify <id>`, `restore <id>` with an optional `--remove-file` after `restore`;
/// `None` for anything else. Ids are checked later, so a bad one gets its own diagnostic.
fn parse_args(args: impl Iterator<Item = OsString>) -> Option<Command> {
    let args: Vec<String> = args.map(OsString::into_string).collect::<Result<_, _>>().ok()?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["list"] => Some(Command::List),
        ["verify", id] if !is_flag(id) => Some(Command::Verify((*id).to_owned())),
        ["restore", id] if !is_flag(id) => Some(Command::Restore { id: (*id).to_owned(), remove: false }),
        ["restore", id, "--remove-file"] | ["restore", "--remove-file", id] if !is_flag(id) => {
            Some(Command::Restore { id: (*id).to_owned(), remove: true })
        }
        _ => None,
    }
}

fn is_flag(arg: &str) -> bool {
    arg.starts_with("--")
}

fn profile(home: Option<&Path>, state_home: Option<&OsStr>) -> Result<Profile, Stop> {
    let home = home
        .ok_or_else(|| Stop::Refused("HOME is not set, so there is no config file to restore".to_owned()))?;
    let store = Backups::locate(Some(home), state_home).ok_or_else(|| {
        Stop::Refused("no generation store: set HOME or an absolute XDG_STATE_HOME".to_owned())
    })?;
    Ok(Profile { home: home.to_path_buf(), store, config: default_path(home) })
}

fn run(command: &Command, profile: &Profile) -> Result<(), Stop> {
    let backups = Backups::at(&profile.store);
    match command {
        Command::List => list(&backups, profile),
        Command::Verify(id) => {
            let id = generation_id(id)?;
            let generation = backups.verify_for(&id, &profile.config).map_err(|e| refused(id, &e))?;
            output::verified(&generation, &profile.config);
            Ok(())
        }
        Command::Restore { id, remove } => restore(generation_id(id)?, *remove, profile),
    }
}

fn list(backups: &Backups, profile: &Profile) -> Result<(), Stop> {
    let generations = backups.list(&profile.config).map_err(|e| Stop::Refused(e.to_string()))?;
    let incomplete = backups.incomplete().map_err(|e| Stop::Refused(e.to_string()))?;
    output::listing(&profile.store, &profile.config, &generations, &incomplete);
    Ok(())
}

/// Looks at the file now on disk (whatever it holds), then restores `id` over exactly that.
fn restore(id: GenerationId, remove: bool, profile: &Profile) -> Result<(), Stop> {
    let backups = Backups::at(&profile.store);
    let generation = backups.verify_for(&id, &profile.config).map_err(|e| refused(id, &e))?;
    let mut file = ConfigFile::protected(&profile.config, backups);
    file.look().map_err(|e| not_restored(id, &e))?;
    let done = if remove { file.recover_removing(&id) } else { file.recover(&id) };
    match done {
        Ok(()) => {
            output::restored(&generation, &profile.config, file.last_generation());
            if matches!(generation.kind, GenerationKind::File) {
                output::checked(&check_restored(profile));
            }
            Ok(())
        }
        Err(ConfigFileError::Changed(_)) => Err(Stop::Conflict(format!(
            "`{}` changed while generation {id} was being restored; nothing was written and both \
             versions are kept. Run `touchbar-recover restore {id}` again to replace the newer file.",
            profile.config.display()
        ))),
        Err(e) => Err(not_restored(id, &e)),
    }
}

/// Whether the restored file validates as `touchbard` would load it: its theme source resolved
/// against the active Omarchy theme of this home. Read only: an unprotected `ConfigFile` and a
/// reload never write a default, a theme file or a generation.
fn check_restored(profile: &Profile) -> output::Verdict {
    let active = ActiveTheme::under_home(&profile.home);
    let mut file = ConfigFile::new(&profile.config);
    match file.reload_resolved(&active.load().theme) {
        Ok(_) => match &file.config().theme {
            ThemeSource::Omarchy => output::Verdict::ValidOmarchy(active.colors_path().to_path_buf()),
            ThemeSource::File { path } => output::Verdict::ValidThemeFile(PathBuf::from(path)),
        },
        Err(ConfigFileError::Invalid(issues)) => {
            output::Verdict::Refused(issues.iter().map(ToString::to_string).collect())
        }
        Err(e) => output::Verdict::Refused(vec![e.to_string()]),
    }
}

fn generation_id(raw: &str) -> Result<GenerationId, Stop> {
    GenerationId::parse(raw).ok_or_else(|| {
        Stop::Refused(format!(
            "`{raw}` is not a generation number; use one of the numbers `touchbar-recover list` shows"
        ))
    })
}

fn refused(id: GenerationId, e: &BackupError) -> Stop {
    let advice = match e {
        BackupError::Absence { .. } => "; pass --remove-file to remove the config file",
        _ => "",
    };
    Stop::Refused(format!("generation {id} cannot be restored: {e}{advice}"))
}

fn not_restored(id: GenerationId, e: &ConfigFileError) -> Stop {
    match e {
        ConfigFileError::Backup(inner) => refused(id, inner),
        e => Stop::Refused(format!("generation {id} was not restored, the config file is unchanged: {e}")),
    }
}

/// The words `list` and `verify` use for a generation.
fn describe(generation: &Generation) -> (String, &'static str, String) {
    let reason = match generation.reason {
        tb_config::Reason::FirstWrite => "first-write",
        tb_config::Reason::Save => "save",
        tb_config::Reason::Overwrite => "overwrite",
        tb_config::Reason::Recover => "recover",
        tb_config::Reason::FactoryReset => "factory-reset",
    };
    let kind = match generation.kind {
        GenerationKind::File => "file",
        GenerationKind::Absent { .. } => "absent",
    };
    let health = match &generation.health {
        Health::Verified => "verified".to_owned(),
        Health::Damaged(why) => format!("damaged ({why})"),
    };
    (reason.to_owned(), kind, health)
}
