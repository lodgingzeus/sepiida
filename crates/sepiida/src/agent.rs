//! `sepiida run`: the background agent. Applies once at startup, then again
//! whenever the wallpaper (Windows or Wallpaper Engine) or the settings
//! change.
//!
//! Event-driven only: the main thread sleeps on a channel and wakes for a
//! notification or for the end of a debounce wait, never on a timer.

use std::{
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError, Sender},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use sepiida_core::{
    cache::PaletteCache,
    config::Config,
    schedule::{Event, Flow, Scheduler},
};
use tracing::{error, info, warn};

use crate::{
    apply::{self, Origin},
    cli::ApplyOptions,
    engine_watch::EngineWatch,
    logging,
    paths::Paths,
    watch::{FileWatcher, MissingFolder},
    win::{desktop_events, single_instance},
};

/// Quiet time after the last notification before applying.
const SETTLE: Duration = Duration::from_millis(500);
/// Longest wait after the first notification of a burst.
const MAX_DELAY: Duration = Duration::from_secs(3);

pub fn run() -> Result<()> {
    let paths = Paths::locate()?;
    // Before logging starts, so a second copy cannot rotate the log file
    // the running agent writes to.
    let Some(_instance) =
        single_instance::acquire().context("could not check for another agent")?
    else {
        println!("sepiida is already running");
        return Ok(());
    };
    logging::init(&paths.log_dir)?;
    info!(version = env!("CARGO_PKG_VERSION"), "agent started");

    let (events, receiver) = mpsc::channel();
    let _settings_watcher = watch_files(vec![paths.config.clone()], events.clone())?;
    desktop_events::spawn(events.clone())?;
    let mut engine = EngineWatch::start(paths.agent_state.clone(), events.clone());
    // Theme from the current wallpaper straight away. The channel was just
    // created and `receiver` is alive, so the send cannot fail.
    let _ = events.send(Event::ApplyNow);

    let mut agent = Agent { paths, cache: None };
    let mut scheduler = Scheduler::new(SETTLE, MAX_DELAY);
    loop {
        let event = match scheduler.deadline() {
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(due) => receiver.recv_timeout(due.saturating_duration_since(Instant::now())),
        };
        match event {
            Ok(event) => {
                if scheduler.handle(event, Instant::now()) == Flow::Quit {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // Unreachable while `events` is alive; quit rather than spin.
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if scheduler.take_due(Instant::now()) {
            agent.apply_once();
            engine.refresh();
        }
    }
    info!("agent stopped");
    Ok(())
}

fn watch_files(files: Vec<PathBuf>, events: Sender<Event>) -> Result<FileWatcher> {
    Ok(FileWatcher::new(files, MissingFolder::Create, move || {
        let _ = events.send(Event::Changed);
    })?)
}

struct Agent {
    paths: Paths,
    /// Loaded on first use and kept for the life of the agent.
    cache: Option<PaletteCache>,
}

impl Agent {
    /// Never fails: every problem is logged and the agent carries on, so a
    /// bad wallpaper or a locked registry key does not stop later applies.
    fn apply_once(&mut self) {
        if let Err(error) = self.try_apply() {
            error!("apply failed: {error:#}");
        }
    }

    fn try_apply(&mut self) -> Result<()> {
        // Re-read every time: the file is tiny and this picks up edits
        // without a separate reload path.
        let (config, config_error) = Config::load_or_default(&self.paths.config);
        if let Some(error) = config_error {
            warn!(
                "{:#}; using the default settings",
                anyhow::Error::new(error)
            );
        }
        let cache = self.cache.get_or_insert_with(|| {
            let (cache, error) = PaletteCache::load_or_empty(&self.paths.palette_cache);
            if let Some(error) = error {
                warn!(
                    "{:#}; starting with an empty cache",
                    anyhow::Error::new(error)
                );
            }
            cache
        });

        let prepared = apply::prepare(&ApplyOptions::default().settings(&config), cache)?;
        if let Err(error) = cache.save(&self.paths.palette_cache) {
            warn!("{:#}", anyhow::Error::new(error));
        }
        if let Some(error) = prepared.wallpaper_engine_error() {
            warn!("Wallpaper Engine could not be read, using the Windows wallpaper: {error}");
        }
        let origin = match &prepared.origin {
            Origin::Locked => "locked colour".to_owned(),
            Origin::Wallpaper(resolved) => resolved.kind.to_string(),
        };
        if prepared.pending.is_empty() {
            info!(%origin, source = %prepared.palette.source, "nothing to change");
            return Ok(());
        }
        apply::commit(&prepared, &self.paths.saved_theme)?;
        info!(
            %origin,
            source = %prepared.palette.source,
            values = prepared.pending.len(),
            "applied"
        );
        Ok(())
    }
}
