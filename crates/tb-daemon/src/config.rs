//! One owner of the accepted config and its theme; watchers only send notifications.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use tb_config::{ConfigFile, ConfigFileError, default_path};
use tb_core::config::{Config, ThemeSource};
use tb_core::{BarTheme, Layout};
use tb_daemon::Event;

use crate::{Message, log};

pub(super) struct ConfigState {
    file: Option<ConfigFile>,
    active: BarTheme,
    theme: BarTheme,
}

impl ConfigState {
    /// A failed startup load keeps the default config and active theme. Without HOME the
    /// bundled theme/default layout remain available and no guessed config path is touched.
    pub(super) fn start(home: Option<&Path>, active: BarTheme, events: &Sender<Message>) -> Self {
        let mut state = Self {
            file: home.map(|home| ConfigFile::new(default_path(home))),
            theme: active.clone(),
            active,
        };
        let Some(file) = &mut state.file else {
            log::error(
                "HOME is not set: config loading and changes are not followed; showing the default config",
            );
            return state;
        };
        match file.load_resolved(&state.active) {
            Ok(theme) => state.theme = theme,
            Err(error) => report(file, error),
        }
        match file.watch() {
            Ok(mut watch) => {
                // Watch after creating a missing default, then re-read: saves made between
                // the initial load and registration must reach the initial scene too.
                match file.reload_resolved(&state.active) {
                    Ok(theme) => state.theme = theme,
                    Err(error) => report(file, error),
                }
                let events = events.clone();
                thread::spawn(move || {
                    loop {
                        if let Err(error) = watch.next_change() {
                            log::error(format_args!("{error}; config changes are no longer followed"));
                            return;
                        }
                        if events.send(Message::ConfigChanged).is_err() {
                            return;
                        }
                    }
                });
            }
            Err(error) => log::error(format_args!("{error}; config changes are not followed")),
        }
        state
    }

    pub(super) fn layout(&self) -> Layout {
        self.file.as_ref().map_or_else(|| Config::default().layout, |file| file.config().layout.clone())
    }

    pub(super) fn theme(&self) -> BarTheme {
        self.theme.clone()
    }

    pub(super) fn reload(&mut self) -> Option<Event> {
        let file = self.file.as_mut()?;
        match file.reload_resolved(&self.active) {
            Ok(theme) => self.theme = theme,
            Err(error) => {
                report(file, error);
                return None;
            }
        }
        Some(Event::Config { layout: self.layout(), theme: self.theme() })
    }

    pub(super) fn active_theme(&mut self, active: BarTheme) -> Option<Event> {
        self.active = active;
        if let Some(file) = &self.file {
            if !matches!(file.config().theme, ThemeSource::Omarchy) {
                return None;
            }
            if let Err(error) = file.check_active_theme(&self.active) {
                report(file, error);
                return None;
            }
        }
        self.theme = self.active.clone();
        Some(Event::Config { layout: self.layout(), theme: self.theme() })
    }
}

fn report(file: &ConfigFile, error: ConfigFileError) {
    match error {
        ConfigFileError::Invalid(issues) => {
            for issue in issues {
                log::error(format_args!(
                    "config file `{}`: {issue}; keeping the last valid config",
                    file.path().display()
                ));
            }
        }
        error => log::error(format_args!("{error}; keeping the last valid config")),
    }
}
