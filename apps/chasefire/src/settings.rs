//! What Chasefire remembers between runs.
//!
//! One JSON file. Readable, editable in any text editor, and easy to delete
//! when somebody wants to start again — which matters more than being compact,
//! because the person deleting it will be doing so under pressure.
//!
//! Two places, in this order:
//!
//! 1. `chasefire.json` **beside the executable**, if it exists. That is the
//!    portable case: a stick that goes from venue to venue carrying its own
//!    settings, which is how a fair number of people in this trade work.
//! 2. Otherwise the usual place for the platform — `~/.config/chasefire/` or
//!    `%APPDATA%\chasefire\`.
//!
//! Nothing here is required. A missing or corrupt file means defaults and a
//! note in the log, never a refusal to start: settings are a convenience and
//! must never be the reason a show cannot begin.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub device: Option<String>,
    pub channel: usize,
    /// `None` means work the frame rate out from the signal.
    pub frame_rate: Option<f64>,
    pub osc_target: Option<String>,
    pub cue_file: Option<String>,
    pub offset_frames: i32,
    pub freewheel_frames: u32,
    /// True for the little guitarist, false for the transport marks.
    pub pablo: bool,
    pub always_on_top: bool,
    /// How many times the reminder has been closed. Kept because it is honest,
    /// and because it is funnier than a number nobody can see.
    pub reminders_dismissed: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device: None,
            channel: 1,
            frame_rate: None,
            osc_target: None,
            cue_file: None,
            offset_frames: 0,
            freewheel_frames: 8,
            pablo: true,
            always_on_top: true,
            reminders_dismissed: 0,
        }
    }
}

impl Settings {
    /// Read them, or return defaults. Never fails.
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = path() else {
            return (Self::default(), None);
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(settings) => (settings, None),
                // A corrupt file is worth mentioning but not worth stopping
                // for. Starting with defaults beats not starting.
                Err(error) => (
                    Self::default(),
                    Some(format!("settings unreadable ({error}) — using defaults")),
                ),
            },
            Err(_) => (Self::default(), None),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = path().ok_or("nowhere to save settings")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(&path, text).map_err(|error| error.to_string())
    }

    /// Where the file is, for showing in Options. People ask.
    pub fn location() -> String {
        path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "nowhere".into())
    }
}

/// Beside the executable if a file is already there, otherwise the platform's
/// usual place.
fn path() -> Option<PathBuf> {
    if let Ok(executable) = std::env::current_exe() {
        let portable = executable.with_file_name("chasefire.json");
        if portable.exists() {
            return Some(portable);
        }
    }
    config_directory().map(|directory| directory.join("settings.json"))
}

fn config_directory() -> Option<PathBuf> {
    // Done by hand rather than with a crate: it is six lines, and a dependency
    // that reads environment variables is a dependency all the same.
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|base| PathBuf::from(base).join("chasefire"))
    } else if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(PathBuf::from(base).join("chasefire"))
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("chasefire"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_safe_ones() {
        let settings = Settings::default();
        // Not armed, and freewheeling for the length the trade expects. Nothing
        // in here should surprise somebody who has never opened the file.
        assert_eq!(settings.freewheel_frames, 8);
        assert_eq!(settings.channel, 1);
        assert_eq!(settings.offset_frames, 0);
        assert!(settings.frame_rate.is_none(), "work it out unless told");
    }

    #[test]
    fn a_file_from_an_older_version_still_loads() {
        // `serde(default)` means a settings file written by a version that had
        // fewer fields still opens, filling in the rest. Somebody upgrading
        // mid-tour must not lose their setup to a schema change.
        let old = r#"{"channel": 3, "osc_target": "10.0.0.5:7000"}"#;
        let settings: Settings = serde_json::from_str(old).expect("should load");
        assert_eq!(settings.channel, 3);
        assert_eq!(settings.osc_target.as_deref(), Some("10.0.0.5:7000"));
        assert_eq!(settings.freewheel_frames, 8, "missing fields take defaults");
    }

    #[test]
    fn it_survives_a_round_trip() {
        let settings = Settings {
            channel: 7,
            reminders_dismissed: 47,
            pablo: false,
            ..Default::default()
        };

        let text = serde_json::to_string(&settings).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();

        assert_eq!(back.channel, 7);
        assert_eq!(back.reminders_dismissed, 47);
        assert!(!back.pablo);
    }
}
