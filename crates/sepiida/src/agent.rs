//! `sepiida run`: the background agent. Applies once at startup, then again
//! whenever the wallpaper (Windows or Wallpaper Engine) or the settings
//! change.
//!
//! Event-driven only: the main thread sleeps on a channel and wakes for a
//! notification or for the end of a debounce wait, never on a timer.

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use sepiida_core::{
    Color,
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
    tray,
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
    // The tray must exist before the event loop starts, on this thread.
    tray::start(events.clone(), paths.clone())?;
    desktop_events::spawn(events.clone(), tray::recreate_tray)?;

    let worker_events = events.clone();
    let worker = thread::Builder::new()
        .name("agent".to_owned())
        .spawn(move || {
            let result = agent_loop(paths, &receiver, worker_events);
            if let Err(error) = &result {
                error!("agent stopped: {error:#}");
            }
            // However the loop ended, take the tray down with it.
            let _ = slint::quit_event_loop();
            result
        })
        .context("could not start the agent thread")?;

    // The tray keeps the loop alive; it returns after "Quit".
    let ui_result = slint::run_event_loop_until_quit();
    // Drop the tray and any settings window here, on the event-loop thread
    // and while Slint is still set up, not in thread-local destructors at
    // exit.
    tray::shut_down();
    let _ = events.send(Event::Quit);
    let worker_result = worker
        .join()
        .unwrap_or_else(|_| Err(anyhow::anyhow!("the agent thread panicked")));
    info!("agent stopped");
    // A failed agent (for example a watcher that could not start) is
    // reported, so sepiidaw.exe shows it instead of vanishing.
    worker_result?;
    ui_result.context("the tray icon stopped")?;
    Ok(())
}

/// The debounce loop. Runs on its own thread until it receives `Quit`.
fn agent_loop(paths: Paths, receiver: &Receiver<Event>, events: Sender<Event>) -> Result<()> {
    let _settings_watcher = watch_files(vec![paths.config.clone()], events.clone())?;
    let mut engine = EngineWatch::start(paths.agent_state.clone(), events.clone());
    // Theme from the current wallpaper straight away. `receiver` is alive,
    // so the send cannot fail.
    let _ = events.send(Event::ApplyNow);
    drop(events);

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
                    return Ok(());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // Every sender is gone: nothing can wake the loop again.
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        if scheduler.take_due(Instant::now()) {
            if let Some(accent) = agent.apply_once() {
                tray::show_accent(accent);
            }
            engine.refresh();
        }
    }
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
    /// Returns the accent colour now in effect, if there is one.
    fn apply_once(&mut self) -> Option<Color> {
        match self.try_apply() {
            Ok(accent) => accent,
            Err(error) => {
                error!("apply failed: {error:#}");
                None
            }
        }
    }

    fn try_apply(&mut self) -> Result<Option<Color>> {
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
        let accent = prepared.accent.map(|accent| accent.accent());
        if prepared.pending.is_empty() {
            info!(%origin, source = %prepared.palette.source, "nothing to change");
            return Ok(accent);
        }
        apply::commit(&prepared, &self.paths.saved_theme)?;
        info!(
            %origin,
            source = %prepared.palette.source,
            values = prepared.pending.len(),
            "applied"
        );
        Ok(accent)
    }
}
