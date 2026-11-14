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
    Color, Palette,
    cache::PaletteCache,
    config::Config,
    schedule::{Event, Flow, Scheduler},
};
use tracing::{error, info, warn};

use crate::{
    apply::{self, Origin},
    cli::ApplyOptions,
    gui::tray,
    logging,
    paths::Paths,
    win::{desktop, desktop_events, flyout_events, mode_watch, single_instance},
};

use super::{
    engine_watch::EngineWatch,
    styling::{RETRIES_AFTER_RESTART, ShellStyling},
    watch::{FileWatcher, MissingFolder},
};

/// Quiet time after the last notification before applying.
const SETTLE: Duration = Duration::from_millis(500);
/// Longest wait after the first notification of a burst.
const MAX_DELAY: Duration = Duration::from_secs(3);
/// At sign-in Wallpaper Engine usually starts a little after the agent and
/// does not touch its `config.json` when it does, so the agent looks once
/// more after this long. The one timer in the agent, approved as an
/// exception to the event-only rule.
const ENGINE_RECHECK: Duration = Duration::from_secs(45);

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
    let restart_events = events.clone();
    desktop_events::spawn(events.clone(), move || {
        tray::recreate_tray();
        explorer_restarted(&restart_events);
    })?;

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
    let _wallpaper_watcher = watch_transcoded_wallpaper(events.clone());
    let mut engine = EngineWatch::start(paths.agent_state.clone(), events.clone());
    recheck_for_engine_later(events.clone());
    watch_system_mode(events.clone());
    watch_flyouts(events.clone());
    // Theme from the current wallpaper straight away. `receiver` is alive,
    // so the send cannot fail.
    let _ = events.send(Event::ApplyNow);

    let styling = ShellStyling::new(paths.config.clone(), events);
    let mut agent = Agent {
        paths,
        cache: None,
        styling,
    };
    let mut scheduler = Scheduler::new(SETTLE, MAX_DELAY);
    let result = run_loop(&mut agent, &mut scheduler, receiver, &mut engine);
    // Shell styling does not outlive the agent.
    if agent.styling.enabled() {
        agent.styling.stop();
    }
    result
}

fn run_loop(
    agent: &mut Agent,
    scheduler: &mut Scheduler,
    receiver: &Receiver<Event>,
    engine: &mut EngineWatch,
) -> Result<()> {
    loop {
        let event = match scheduler.deadline() {
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(due) => receiver.recv_timeout(due.saturating_duration_since(Instant::now())),
        };
        match event {
            Ok(event) => {
                match event {
                    Event::ExplorerRestarted(at) => agent.styling.explorer_restarted(at),
                    Event::ShellHostExited(host, pid, exit_code, at) => {
                        agent.styling.host_exited(host, pid, exit_code, at);
                    }
                    Event::ShellWindowShown(host, pid) => agent.styling.window_shown(host, pid),
                    _ => {}
                }
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
        info!(trigger = "settings file", "change noticed");
        let _ = events.send(Event::Changed);
    })?)
}

/// Windows rewrites this copy on every wallpaper change, which also covers
/// changes that arrive without a broadcast. Optional: the folder is
/// Windows' own, so it is never created, and the broadcast still works
/// without it.
fn watch_transcoded_wallpaper(events: Sender<Event>) -> Option<FileWatcher> {
    let file = desktop::transcoded_wallpaper()?;
    let watched = FileWatcher::new(vec![file], MissingFolder::Fail, move || {
        info!(trigger = "wallpaper file", "change noticed");
        let _ = events.send(Event::Changed);
    });
    match watched {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            warn!("{:#}", anyhow::Error::new(error));
            None
        }
    }
}

/// Runs on the desktop event thread after `TaskbarCreated`. Explorer's
/// XAML comes up a little after its taskbar window, so the agent also looks
/// again a few seconds later to attach the styling DLL.
fn explorer_restarted(events: &Sender<Event>) {
    let _ = events.send(Event::ExplorerRestarted(Instant::now()));
    for delay in RETRIES_AFTER_RESTART {
        let events = events.clone();
        let spawned = thread::Builder::new()
            .name("explorer-recheck".to_owned())
            .spawn(move || {
                thread::sleep(delay);
                info!(trigger = "Explorer restart recheck", "change noticed");
                let _ = events.send(Event::Changed);
            });
        if let Err(error) = spawned {
            warn!("could not schedule a look after the Explorer restart: {error}");
        }
    }
}

/// Shell host windows appearing: the notification centre has XAML only
/// while shown, and any host may have a new brush since it was last
/// shown. Without the hook those stay unstyled until the next apply;
/// nothing else depends on it.
fn watch_flyouts(events: Sender<Event>) {
    let watched = flyout_events::spawn(move |host, pid| {
        let _ = events.send(Event::ShellWindowShown(host, pid));
    });
    if let Err(error) = watched {
        warn!("{:#}", anyhow::Error::new(error));
    }
}

/// Shell colours are made for the system light/dark mode, so a change of
/// mode means new colours.
fn watch_system_mode(events: Sender<Event>) {
    let watched = mode_watch::spawn(move |mode| {
        info!(trigger = "system mode", ?mode, "change noticed");
        let _ = events.send(Event::Changed);
    });
    if let Err(error) = watched {
        warn!("{:#}", anyhow::Error::new(error));
    }
}

fn recheck_for_engine_later(events: Sender<Event>) {
    let spawned = thread::Builder::new()
        .name("engine-recheck".to_owned())
        .spawn(move || {
            thread::sleep(ENGINE_RECHECK);
            info!(trigger = "Wallpaper Engine recheck", "change noticed");
            let _ = events.send(Event::Changed);
        });
    if let Err(error) = spawned {
        warn!("could not schedule the Wallpaper Engine recheck: {error}");
    }
}

struct Agent {
    paths: Paths,
    /// Loaded on first use and kept for the life of the agent.
    cache: Option<PaletteCache>,
    styling: ShellStyling,
}

/// What one apply put in effect.
struct Applied {
    accent: Option<Color>,
    palette: Palette,
    shell_styling: bool,
}

impl Agent {
    /// Never fails: every problem is logged and the agent carries on, so a
    /// bad wallpaper or a locked registry key does not stop later applies.
    /// Returns the accent colour now in effect, if there is one.
    fn apply_once(&mut self) -> Option<Color> {
        match self.try_apply() {
            Ok(applied) => {
                // Also when the registry needed no change: the shell may
                // have lost its colours (Explorer restart) or need them for
                // another mode.
                self.styling.sync(applied.shell_styling, &applied.palette);
                applied.accent
            }
            Err(error) => {
                error!("apply failed: {error:#}");
                None
            }
        }
    }

    fn try_apply(&mut self) -> Result<Applied> {
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
        let (origin, wallpaper) = match &prepared.origin {
            Origin::Locked => ("locked colour".to_owned(), String::new()),
            Origin::Wallpaper(resolved) => {
                (resolved.kind.to_string(), resolved.wallpaper.to_string())
            }
        };
        let applied = Applied {
            accent: prepared.accent.map(|accent| accent.accent()),
            palette: prepared.palette,
            shell_styling: config.shell_styling,
        };
        if prepared.pending.is_empty() {
            info!(%origin, %wallpaper, source = %prepared.palette.source, "nothing to change");
            return Ok(applied);
        }
        apply::commit(&prepared, &self.paths.saved_theme)?;
        info!(
            %origin,
            %wallpaper,
            source = %prepared.palette.source,
            values = prepared.pending.len(),
            "applied"
        );
        Ok(applied)
    }
}
