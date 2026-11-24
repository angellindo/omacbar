//! Recovery in the configurator (ADR-0015): where the generations of the config are kept, the
//! newest five of this file, and restoring one of them. A restore is one more guarded write: it
//! replaces only the version the editor last saw, keeps what it replaces as a `recover`
//! generation and writes the kept bytes with the kept permission bits; the editor then shows the
//! file as startup shows it. It is offered only from an editor with nothing pending, so no draft
//! is ever lost. The physical return to the key row is a separate, human procedure: the editor
//! only points to it.

use rune::{El, col, el, text};
use tb_config::{ConfigFileError, Generation, GenerationId, GenerationKind, Health, Reason};
use tb_render::view::color;

use crate::app::{State, body};

/// How many generations the editor lists; `touchbar-recover list` shows them all.
const SHOWN: usize = 5;

/// What the store holds for this file, newest first, or why it could not be listed.
pub(crate) type History = Result<Vec<Generation>, String>;

/// What showing a file does when it is missing.
#[derive(Clone, Copy)]
pub(crate) enum Missing {
    /// Startup: write the default (keeping an absence generation first).
    WriteDefault,
    /// After a restore: say so, write nothing.
    Report,
}

impl State {
    /// Lists the generations of the editor's file again. Reads only.
    pub(crate) fn refresh_history(&mut self) {
        let Some(file) = &self.file else {
            self.history = Ok(Vec::new());
            return;
        };
        self.history = file
            .backups()
            .map_or_else(|| Ok(Vec::new()), |backups| backups.list(file.path()).map_err(|e| e.to_string()));
    }

    /// Nothing differs from what the editor last opened or saved, and no reset is pending.
    fn untouched(&self) -> bool {
        let reference = self.saving.baseline.as_ref().unwrap_or(&self.opened);
        !self.reset_pending() && reference == &self.config && self.invalid_widths().is_empty()
    }

    /// Restores generation `id` over the file the editor last saw, then shows the result.
    pub(crate) fn restore(&mut self, id: GenerationId) {
        if !self.untouched() {
            self.status = format!("not restored: {PENDING}");
            return;
        }
        let Some(file) = &mut self.file else { return };
        match file.recover(&id) {
            Ok(()) => {
                let name = file.path().display().to_string();
                let kept = file.last_generation_path().map_or_else(String::new, |g| {
                    format!("; the file it replaced is kept in {}", g.display())
                });
                let fresh = file.reopened();
                let shown = self.show_file(fresh, Missing::Report);
                self.status = format!("restored {name} from generation {id}{kept}; {shown}");
            }
            Err(ConfigFileError::Changed(now)) if now.is_absent() => {
                self.status = format!(
                    "not restored: {} was removed since it was opened; nothing was written or kept",
                    self.file_name()
                );
                self.refresh_history();
            }
            Err(ConfigFileError::Changed(_)) => {
                self.status = match self.adopt() {
                    Ok(_) => "not restored: config changed on disk; showing the newer version \
                              (nothing written or kept)"
                        .to_owned(),
                    Err(why) => format!("not restored: config changed on disk: {why}; nothing written"),
                };
            }
            Err(e) => {
                self.status = format!("not restored: {e}");
                self.refresh_history();
            }
        }
    }
}

const PENDING: &str = "to restore a generation, save or cancel your pending changes first";

/// Plain text that wraps inside a long path too.
fn line(s: &State, key: &str, message: &str) -> El {
    let view = text(message).font(body()).w_full().wrap_anywhere().color(color(s.theme.button_text));
    if key.is_empty() { view } else { view.key(key) }
}

/// The recovery region: location, newest generations, the rest and the key-row pointer.
pub(crate) fn recovery(s: &State) -> El {
    let mut region =
        col().key("recovery").w_full().child(line(s, "recovery-title", "RESTORE A SAVED CONFIG"));
    let Some(backups) = s.file.as_ref().and_then(|f| f.backups()) else {
        return region
            .child(line(s, "backups-location", "no generation store: nothing can be restored here").mt(4.0))
            .child(line(s, "key-row-return", KEY_ROW).mt(12.0));
    };
    region = region.child(
        line(
            s,
            "backups-location",
            &format!(
                "generations of this file are kept in {}; `touchbar-recover list` shows them all",
                backups.dir().display()
            ),
        )
        .mt(4.0),
    );
    let clean = s.untouched();
    if !clean {
        region = region.child(line(s, "recovery-pending", PENDING).mt(4.0));
    }
    match &s.history {
        Ok(all) if all.is_empty() => region = region.child(line(s, "", "no generation kept yet").mt(4.0)),
        Ok(all) => {
            region = region.children(all.iter().take(SHOWN).map(|g| row(s, g, clean).mt(4.0)));
            if all.len() > SHOWN {
                let more =
                    format!("{} more generation(s) of this file: `touchbar-recover list`", all.len() - SHOWN);
                region = region.child(line(s, "recovery-more", &more).mt(4.0));
            }
        }
        Err(why) => region = region.child(line(s, "recovery-error", &format!("not listed: {why}")).mt(4.0)),
    }
    region.child(line(s, "key-row-return", KEY_ROW).mt(12.0))
}

const KEY_ROW: &str = "back to the key row: a hardware step this editor never does; follow \
     docs/hardware-runbook.md, section Physical return to key mode. Your layout and its backups \
     stay on disk.";

/// One generation: what it is, and a press that restores it when it can be restored here.
fn row(s: &State, g: &Generation, clean: bool) -> El {
    let label = describe(g);
    let restorable = clean && g.kind == GenerationKind::File && g.health == Health::Verified;
    let view = el()
        .key(format!("recover-{}", g.id))
        .role("button")
        .label(label.as_str())
        .w(168.0)
        .padding(8.0)
        .fill(color(s.theme.button_fill))
        .border(1.0, color(s.theme.button_border))
        .child(line(s, "", &label));
    if restorable {
        let id = g.id;
        view.on_press(move |s: &mut State| s.restore(id))
    } else {
        view.disabled(true).opacity(0.6)
    }
}

/// The row text: the id, which write kept it, and what it holds or why it cannot be restored.
fn describe(g: &Generation) -> String {
    let when = match g.reason {
        Reason::FirstWrite => "before the first write",
        Reason::Save => "before a save",
        Reason::Overwrite => "before an overwrite",
        Reason::Recover => "before a restore",
        Reason::FactoryReset => "before factory reset",
    };
    match (&g.health, g.kind) {
        (Health::Damaged(why), _) => format!("#{} {when} · damaged, never restored: {why}", g.id),
        (Health::Verified, GenerationKind::Absent { .. }) => {
            format!("#{} {when} · no file then; only `touchbar-recover` can return to that", g.id)
        }
        (Health::Verified, GenerationKind::File) => {
            format!("restore #{} · {when} · {} bytes · mode {:04o}", g.id, g.len, g.mode)
        }
    }
}
