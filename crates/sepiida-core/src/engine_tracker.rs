//! Keeping track of Wallpaper Engine from the background agent.
//!
//! Windows has no event for "a process started" without admin rights, so
//! the agent does not poll for Wallpaper Engine. Instead:
//!
//! - whenever the agent checks the wallpaper and finds Wallpaper Engine
//!   running, it remembers the install folder (on disk, across restarts)
//!   and waits on that process so it hears when it exits;
//! - it watches `config.json` in the remembered folder, which Wallpaper
//!   Engine writes when its wallpaper or playlist changes, and also when
//!   it starts, so a start is noticed without polling.
//!
//! Until Wallpaper Engine has been seen once, only an apply for another
//! reason (a Windows wallpaper change, a settings change, "apply now")
//! notices it.

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::fsio;

/// Wallpaper Engine's settings file, next to its executable.
pub const CONFIG_FILE: &str = "config.json";
const STATE_VERSION: u32 = 1;
const MAX_STATE_BYTES: u64 = 64 * 1024;
/// Longest path Windows accepts, in UTF-16 units.
const MAX_PATH_CHARS: usize = 32_767;

/// A running Wallpaper Engine, as found in the process list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub process_id: u32,
    /// The folder holding the executable.
    pub folder: PathBuf,
}

/// What the agent should start doing after an observation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Actions {
    /// Watch `config.json` in this folder instead of the previous one, and
    /// remember the folder on disk.
    pub watch_folder: Option<PathBuf>,
    /// Wait for this process to exit.
    pub wait_for_exit: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Tracker {
    folder: Option<PathBuf>,
    waiting_for: Option<u32>,
}

impl Tracker {
    /// Starts from the folder remembered last time, if any. The caller
    /// watches it straight away.
    pub fn new(remembered: Option<PathBuf>) -> Self {
        Self {
            folder: remembered,
            waiting_for: None,
        }
    }

    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_deref()
    }

    /// Records what the process list showed. `None` (not running) changes
    /// nothing: the folder stays watched so a restart is noticed.
    pub fn observe(&mut self, seen: Option<Observed>) -> Actions {
        let Some(seen) = seen else {
            return Actions::default();
        };
        let mut actions = Actions::default();
        if self.folder.as_ref() != Some(&seen.folder) {
            self.folder = Some(seen.folder.clone());
            actions.watch_folder = Some(seen.folder);
        }
        if self.waiting_for != Some(seen.process_id) {
            self.waiting_for = Some(seen.process_id);
            actions.wait_for_exit = Some(seen.process_id);
        }
        actions
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct StateFile {
    version: u32,
    wallpaper_engine_folder: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("could not read the remembered Wallpaper Engine folder")]
    Read(#[source] io::Error),
    #[error("the remembered Wallpaper Engine folder is not valid")]
    Parse(#[source] serde_json::Error),
    #[error("could not save the Wallpaper Engine folder")]
    Write(#[source] io::Error),
}

/// Reads the remembered folder. A missing, damaged or implausible entry is
/// `None`: the only cost is waiting until Wallpaper Engine is seen again.
/// The caller still checks that the folder exists.
pub fn load_folder(path: &Path) -> Result<Option<PathBuf>, StateError> {
    let bytes = match fsio::read_capped(path, MAX_STATE_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(StateError::Read(error)),
    };
    let file: StateFile = serde_json::from_slice(&bytes).map_err(StateError::Parse)?;
    if file.version != STATE_VERSION {
        return Ok(None);
    }
    Ok(file
        .wallpaper_engine_folder
        .filter(|folder| is_plausible_path(folder))
        .map(PathBuf::from))
}

pub fn save_folder(path: &Path, folder: &Path) -> Result<(), StateError> {
    let file = StateFile {
        version: STATE_VERSION,
        wallpaper_engine_folder: folder.to_str().map(str::to_owned),
    };
    let bytes = serde_json::to_vec_pretty(&file).map_err(StateError::Parse)?;
    fsio::write_atomic(path, &bytes).map_err(StateError::Write)
}

fn is_plausible_path(text: &str) -> bool {
    !text.is_empty()
        && text.encode_utf16().count() <= MAX_PATH_CHARS
        && !text.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(process_id: u32, folder: &str) -> Option<Observed> {
        Some(Observed {
            process_id,
            folder: PathBuf::from(folder),
        })
    }

    #[test]
    fn first_sighting_watches_the_folder_and_waits_for_exit() {
        let mut tracker = Tracker::new(None);
        assert_eq!(
            tracker.observe(seen(7, "/we")),
            Actions {
                watch_folder: Some(PathBuf::from("/we")),
                wait_for_exit: Some(7),
            }
        );
        assert_eq!(tracker.folder(), Some(Path::new("/we")));
        assert_eq!(tracker.observe(seen(7, "/we")), Actions::default());
    }

    #[test]
    fn not_running_keeps_the_folder_watched() {
        let mut tracker = Tracker::new(Some(PathBuf::from("/we")));
        assert_eq!(tracker.observe(None), Actions::default());
        assert_eq!(tracker.folder(), Some(Path::new("/we")));
    }

    #[test]
    fn a_restart_waits_on_the_new_process_only() {
        let mut tracker = Tracker::new(Some(PathBuf::from("/we")));
        assert_eq!(
            tracker.observe(seen(7, "/we")),
            Actions {
                watch_folder: None,
                wait_for_exit: Some(7),
            }
        );
        tracker.observe(None);
        assert_eq!(
            tracker.observe(seen(9, "/we")),
            Actions {
                watch_folder: None,
                wait_for_exit: Some(9),
            }
        );
    }

    #[test]
    fn a_moved_install_switches_the_watched_folder() {
        let mut tracker = Tracker::new(Some(PathBuf::from("/old")));
        let actions = tracker.observe(seen(7, "/new"));
        assert_eq!(actions.watch_folder, Some(PathBuf::from("/new")));
    }

    #[test]
    fn the_folder_survives_a_restart_of_the_agent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("state").join("state.json");
        assert_eq!(load_folder(&path).expect("missing is fine"), None);
        save_folder(&path, Path::new("/games/wallpaper_engine")).expect("save");
        assert_eq!(
            load_folder(&path).expect("load"),
            Some(PathBuf::from("/games/wallpaper_engine"))
        );
    }

    #[test]
    fn bad_state_files_give_no_folder() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("state.json");
        let load = |text: &str| {
            std::fs::write(&path, text).expect("write");
            load_folder(&path)
        };
        assert!(matches!(load("{nope"), Err(StateError::Parse(_))));
        assert_eq!(
            load(r#"{"version":2,"wallpaper-engine-folder":"/we"}"#).expect("load"),
            None
        );
        assert_eq!(
            load(r#"{"version":1,"wallpaper-engine-folder":""}"#).expect("load"),
            None
        );
        assert_eq!(
            load(r#"{"version":1,"wallpaper-engine-folder":"/we\nx"}"#).expect("load"),
            None
        );
        assert_eq!(
            load(r#"{"version":1,"wallpaper-engine-folder":null}"#).expect("load"),
            None
        );
    }
}
