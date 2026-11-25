//! The binary's only output (R11): results on stdout, refusals on stderr.

use std::fmt::Display;
use std::path::{Path, PathBuf};

use tb_config::{Generation, GenerationId, GenerationKind};

use super::describe;

#[expect(clippy::print_stderr, reason = "the tool's single error sink")]
pub fn error(message: impl Display) {
    eprintln!("touchbar-recover: {message}");
}

#[expect(clippy::print_stdout, reason = "the tool's single result sink")]
fn say(message: impl Display) {
    println!("{message}");
}

/// The store, the config, one line per generation (newest first), then incomplete temps.
pub fn listing(store: &Path, config: &Path, generations: &[Generation], incomplete: &[String]) {
    say(format_args!("store: {}", store.display()));
    say(format_args!("config: {}", config.display()));
    if generations.is_empty() {
        say("no generations of this config yet");
    }
    for generation in generations {
        say(line(generation));
    }
    for temp in incomplete {
        say(format_args!("incomplete: {temp} (an interrupted write left it; not a generation, kept)"));
    }
}

/// `<id>  <reason>  <kind>  <mode>  <size>  <health>`.
fn line(generation: &Generation) -> String {
    let (reason, kind, health) = describe(generation);
    let what = match generation.kind {
        GenerationKind::File => format!("mode {:o}  {} bytes", generation.mode, generation.len),
        GenerationKind::Absent { dir_existed: true } => "no config file then".to_owned(),
        GenerationKind::Absent { dir_existed: false } => "no config file nor directory then".to_owned(),
    };
    format!("{}  {reason}  {kind}  {what}  {health}", generation.id)
}

pub fn verified(generation: &Generation, config: &Path) {
    say(line(generation));
    let how = match generation.kind {
        GenerationKind::File => format!("touchbar-recover restore {}", generation.id),
        GenerationKind::Absent { .. } => format!("touchbar-recover restore {} --remove-file", generation.id),
    };
    say(format_args!(
        "generation {} is verified and belongs to {}; `{how}` restores it",
        generation.id,
        config.display()
    ));
}

pub fn restored(generation: &Generation, config: &Path, kept: Option<&GenerationId>) {
    match generation.kind {
        GenerationKind::File => say(format_args!(
            "restored generation {} to {} (mode {:o}, {} bytes)",
            generation.id,
            config.display(),
            generation.mode,
            generation.len
        )),
        GenerationKind::Absent { .. } => {
            say(format_args!("removed {} as generation {} records", config.display(), generation.id));
            say("touchbard and touchbar-customizer write the default config when they start without one");
        }
    }
    if let Some(kept) = kept {
        say(format_args!("what it replaced is kept as generation {kept}"));
    }
}

/// What the check of a restored file found, read as `touchbard` would read it.
pub enum Verdict {
    /// Valid with the Omarchy source; the active theme's colours file it was checked against.
    ValidOmarchy(PathBuf),
    /// Valid with the selected theme file at this path.
    ValidThemeFile(PathBuf),
    /// Not accepted: the library's diagnostics, one per issue.
    Refused(Vec<String>),
}

pub fn checked(verdict: &Verdict) {
    match verdict {
        Verdict::ValidOmarchy(colors) => say(format_args!(
            "the restored config is valid with the Omarchy theme source (active theme {}); \
             touchbard draws it once it reloads the file",
            colors.display()
        )),
        Verdict::ValidThemeFile(path) => say(format_args!(
            "the restored config is valid with the theme file {}; touchbard draws it once it reloads the file",
            path.display()
        )),
        Verdict::Refused(why) => {
            say("the restored config is kept as restored, but touchbard would not accept it:");
            for line in why {
                say(format_args!("  {line}"));
            }
            say("touchbard keeps the last valid design on the bar until the file is fixed");
        }
    }
}
