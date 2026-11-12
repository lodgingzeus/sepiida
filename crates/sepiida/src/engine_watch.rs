//! The agent's view of Wallpaper Engine: watches its `config.json` and
//! hears when it exits. See `sepiida_core::engine_tracker` for why there is
//! no polling.

use std::{
    fs::File,
    hash::{DefaultHasher, Hasher},
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, mpsc::Sender},
};

use sepiida_core::{
    engine_tracker::{self, CONFIG_FILE, Tracker},
    schedule::Event,
};
use tracing::{info, warn};

use crate::{
    watch::{FileWatcher, MissingFolder},
    win::wallpaper_engine,
};

/// Wallpaper Engine's settings are well under this; a larger file is
/// treated as changed every time rather than read whole.
const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

pub struct EngineWatch {
    tracker: Tracker,
    watcher: Option<FileWatcher>,
    state_path: PathBuf,
    events: Sender<Event>,
}

impl EngineWatch {
    /// Starts watching the folder remembered from an earlier run, if it
    /// still exists.
    pub fn start(state_path: PathBuf, events: Sender<Event>) -> Self {
        let remembered = match engine_tracker::load_folder(&state_path) {
            Ok(folder) => folder.filter(|folder| folder.is_dir()),
            Err(error) => {
                warn!("{:#}", anyhow::Error::new(error));
                None
            }
        };
        let mut this = Self {
            tracker: Tracker::new(remembered.clone()),
            watcher: None,
            state_path,
            events,
        };
        if let Some(folder) = remembered {
            this.watch(folder);
        }
        this
    }

    /// Looks for a running Wallpaper Engine and starts watching it. Called
    /// around every apply, which is the only time the agent looks.
    pub fn refresh(&mut self) {
        let seen = match wallpaper_engine::running() {
            Ok(seen) => seen,
            Err(error) => {
                warn!("could not look for Wallpaper Engine: {error}");
                return;
            }
        };
        let actions = self.tracker.observe(seen);
        if let Some(folder) = actions.watch_folder {
            info!(folder = %folder.display(), "found Wallpaper Engine");
            if let Err(error) = engine_tracker::save_folder(&self.state_path, &folder) {
                warn!("{:#}", anyhow::Error::new(error));
            }
            self.watch(folder);
        }
        if let Some(process_id) = actions.wait_for_exit {
            let events = self.events.clone();
            wallpaper_engine::wait_for_exit(process_id, move || {
                // In Auto mode the Windows wallpaper takes over.
                let _ = events.send(Event::Changed);
            });
        }
    }

    fn watch(&mut self, folder: PathBuf) {
        let events = self.events.clone();
        let file = folder.join(CONFIG_FILE);
        // Only a change in content counts: Wallpaper Engine may rewrite the
        // file without changing it, which must not cost an apply.
        let last = Mutex::new(content_hash(&file));
        let watched = file.clone();
        match FileWatcher::new(vec![file], MissingFolder::Fail, move || {
            let current = content_hash(&watched);
            let changed = match last.lock() {
                Ok(mut last) => std::mem::replace(&mut *last, current) != current,
                // A panic elsewhere poisoned it; err on the side of applying.
                Err(_) => true,
            };
            if changed || current.is_none() {
                let _ = events.send(Event::Changed);
            }
        }) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(error) => {
                self.watcher = None;
                warn!("{:#}", anyhow::Error::new(error));
            }
        }
    }
}

/// `None` when the file is missing, unreadable or too large.
fn content_hash(path: &Path) -> Option<u64> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).ok()? > MAX_CONFIG_BYTES {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    hasher.write(&bytes);
    Some(hasher.finish())
}
