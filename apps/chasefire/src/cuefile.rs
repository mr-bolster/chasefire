//! Where cue files live, and the rules for not losing one.
//!
//! There is no native file dialog here, on purpose: a modal window that steals
//! focus from a program whose whole job is to sit above everything else is a
//! bad trade, and it would be one more thing to go wrong on a machine that has
//! to boot into a show. The path box *is* the dialog. Typing a different name
//! and pressing save is "save as"; that is the whole of it.
//!
//! What replaces the dialog is the part a dialog actually earns its keep for:
//! **never quietly destroying work.** Two things can do that — overwriting
//! somebody else's file, and starting a new list on top of one that was never
//! saved — and both of them ask first.

use std::path::{Path, PathBuf};

/// Where a cue file should go when nobody has said otherwise.
///
/// Deliberately **not** next to the program. The examples folder ships inside
/// the installation, and a first-time user who presses save is then writing
/// into the program's own directory — which on Windows may not even be
/// writable, and which an update will wipe.
pub fn default_directory() -> PathBuf {
    documents()
        .map(|documents| documents.join("Chasefire"))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn documents() -> Option<PathBuf> {
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    }?;
    let home = PathBuf::from(home);
    let documents = home.join("Documents");
    // Not everybody has one, and a localised Linux desktop may call it
    // something else entirely. Falling back to the home directory is better
    // than inventing a folder somebody will never find again.
    Some(if documents.is_dir() { documents } else { home })
}

/// A name that is not in use yet, so "new" never lands on top of anything.
pub fn untitled(directory: &Path, stem: &str) -> PathBuf {
    let first = directory.join(format!("{stem}.json"));
    if !first.exists() {
        return first;
    }
    // Bounded rather than a bare loop: a directory that somehow answers "yes,
    // that exists" to everything must not hang the program.
    for number in 2..1000 {
        let candidate = directory.join(format!("{stem}-{number}.json"));
        if !candidate.exists() {
            return candidate;
        }
    }
    first
}

/// Write the cues out, making the folder if it is not there yet.
pub fn save(path: &str, cues: &[cue::Cue], no_name: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err(no_name.to_string());
    }
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    let text = serde_json::to_string_pretty(cues).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

/// Is there already a file there? Asked before saving, so that overwriting one
/// is always a second, deliberate click.
pub fn exists(path: &str) -> bool {
    !path.trim().is_empty() && Path::new(path).exists()
}

/// The short name, for saying which file is meant without a line of path.
pub fn name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_place_is_not_inside_the_program() {
        // The bug this exists to stop: shipping with a cue path pointing at the
        // examples folder, so the first save writes into the installation.
        let directory = default_directory();
        let executable = std::env::current_exe().expect("there is an executable");
        let installed = executable.parent().expect("it lives somewhere");
        assert!(
            !directory.starts_with(installed),
            "cues would be saved into the program's own folder: {}",
            directory.display()
        );
    }

    #[test]
    fn a_new_file_never_lands_on_an_old_one() {
        let directory = std::env::temp_dir().join("chasefire-cuefile-test");
        std::fs::create_dir_all(&directory).unwrap();
        let first = untitled(&directory, "prueba");
        std::fs::write(&first, "[]").unwrap();
        let second = untitled(&directory, "prueba");
        assert_ne!(first, second, "handed back a name already taken");
        assert!(!second.exists());
        std::fs::remove_file(&first).ok();
    }

    #[test]
    fn saving_makes_the_folder_it_needs() {
        // Somebody typing a path by hand into a folder that does not exist yet
        // should get the folder, not an error four seconds before doors.
        let directory = std::env::temp_dir().join("chasefire-cuefile-test/hondo/mas-hondo");
        std::fs::remove_dir_all(directory.parent().unwrap()).ok();
        let path = directory.join("show.json");
        save(&path.to_string_lossy(), &[], "sin nombre").expect("should have written it");
        assert!(path.exists());
        std::fs::remove_dir_all(directory.parent().unwrap()).ok();
    }

    #[test]
    fn an_empty_path_is_refused_rather_than_guessed() {
        assert!(save("   ", &[], "sin nombre").is_err());
    }
}
