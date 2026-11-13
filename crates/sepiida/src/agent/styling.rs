//! Shell styling driven by the agent: attaches the hook DLL to Explorer
//! while the setting is on, sends it the palette's colours after every
//! apply, and takes the styling down when the setting goes off or the agent
//! quits. A crash guard turns the setting off if Explorer keeps dying soon
//! after an attach.

use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use sepiida_core::{
    Palette,
    accent::ThemeMode,
    config::Config,
    shell::{
        colors::role_colors,
        crash_guard::{CrashGuard, Verdict},
        protocol::Command,
    },
};
use tracing::{error, info, warn};

use crate::win::{
    message_box,
    shell_hook::{self, Method, Taskbar},
    theme,
};

/// After an Explorer restart, no attach before this long: the taskbar
/// window exists before its XAML does.
const SETTLE_AFTER_RESTART: Duration = Duration::from_secs(3);
/// When the agent looks again after an Explorer restart, counted from the
/// restart. One-off timers after an event, like the tray's re-adds.
pub const RETRIES_AFTER_RESTART: [Duration; 2] = [Duration::from_secs(4), Duration::from_secs(15)];

pub struct ShellStyling {
    config_path: PathBuf,
    guard: CrashGuard,
    /// The setting as of the last apply.
    enabled: bool,
    /// No attach before this time (set after an Explorer restart).
    not_before: Option<Instant>,
    /// Set by the crash guard; cleared once the setting is seen off, so
    /// turning it back on works.
    tripped: bool,
}

impl ShellStyling {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            guard: CrashGuard::default(),
            enabled: false,
            not_before: None,
            tripped: false,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Called after every apply with the setting and the palette in effect.
    /// Never fails: problems are logged and the next apply tries again.
    pub fn sync(&mut self, enabled: bool, palette: &Palette) {
        if !enabled {
            self.tripped = false;
        }
        self.enabled = enabled && !self.tripped;
        if !self.enabled {
            self.stop();
            return;
        }
        if let Some(due) = self.not_before {
            if Instant::now() < due {
                info!("Explorer restarted moments ago; attaching on the next look");
                return;
            }
            self.not_before = None;
        }
        let taskbar = match shell_hook::taskbar() {
            Ok(taskbar) => taskbar,
            Err(error) => {
                warn!("shell styling: {error}");
                return;
            }
        };
        if !shell_hook::is_attached(taskbar.pid) && !self.attach(&taskbar) {
            return;
        }
        let system_dark = theme::system_mode() == ThemeMode::Dark;
        for (role, color) in role_colors(palette, system_dark) {
            if let Err(error) = shell_hook::send(taskbar.pid, Command::SetColor(role, color)) {
                warn!("could not send the shell colours: {error}");
                return;
            }
            info!(?role, %color, system_dark, "shell colour sent");
        }
    }

    fn attach(&mut self, taskbar: &Taskbar) -> bool {
        let dll = match shell_hook::hook_dll_path() {
            Ok(dll) => dll,
            Err(error) => {
                warn!("shell styling: {error}");
                return false;
            }
        };
        info!(pid = taskbar.pid, dll = %dll.display(), "attaching the styling DLL to Explorer");
        // Counted before the attempt: a DLL that crashes Explorer may do so
        // before it reports back.
        self.guard.attached(Instant::now());
        match shell_hook::attach(taskbar, &dll, 1, Method::InProcess) {
            Ok(attached) => {
                if attached.replaced > 0 {
                    info!(
                        windows = attached.replaced,
                        "asked an earlier copy to detach"
                    );
                }
                if let Some(error) = attached.unhook_error {
                    warn!("could not remove the attach hook: {error}");
                }
                if attached.ready {
                    info!("styling DLL attached");
                } else {
                    warn!("the styling DLL did not report back; see logs\\hook.log");
                }
                attached.ready
            }
            Err(error) => {
                warn!("could not attach the styling DLL: {error}");
                false
            }
        }
    }

    /// Explorer restarted at `now`; the styling went with the old process.
    /// Turns styling off after repeated crashes right after attaching.
    pub fn explorer_restarted(&mut self, now: Instant) {
        if !self.enabled {
            return;
        }
        self.not_before = Some(now + SETTLE_AFTER_RESTART);
        if self.guard.explorer_restarted(now) == Verdict::Disable {
            self.enabled = false;
            self.tripped = true;
            turn_off_after_crashes(&self.config_path, self.guard.crashes());
        }
    }

    /// Restores and detaches every copy of the DLL in Explorer, if any.
    pub fn stop(&mut self) {
        let Ok(taskbar) = shell_hook::taskbar() else {
            return;
        };
        match shell_hook::detach_all(taskbar.pid) {
            Ok(0) => {}
            Ok(windows) => info!(windows, "shell styling removed"),
            Err(error) => warn!("could not remove the shell styling: {error}"),
        }
    }
}

/// Saves the setting as off (the agent's settings watcher then applies the
/// change) and tells the user.
fn turn_off_after_crashes(config_path: &Path, crashes: u32) {
    error!(
        crashes,
        "Explorer restarted right after the styling DLL attached, again; turning shell styling off"
    );
    match Config::load_or_default(config_path) {
        // A broken file is left for the user to fix, never overwritten
        // with defaults; styling stays off for this run either way.
        (_, Some(error)) => warn!(
            "could not save the setting: {:#}",
            anyhow::Error::new(error)
        ),
        (mut config, None) => {
            config.shell_styling = false;
            if let Err(error) = config.save(config_path) {
                warn!(
                    "could not save the setting: {:#}",
                    anyhow::Error::new(error)
                );
            }
        }
    }
    // A message box blocks its thread until dismissed; keep it off the
    // agent's.
    let spawned = thread::Builder::new()
        .name("crash-notice".to_owned())
        .spawn(|| {
            message_box::error(
                "Explorer stopped twice right after sepiida restyled the taskbar, so \
                 taskbar styling has been turned off.\n\nYou can turn it back on in \
                 sepiida's settings. The details are in \
                 %LOCALAPPDATA%\\sepiida\\logs\\hook.log.",
            );
        });
    if let Err(error) = spawned {
        warn!("could not show the notice: {error}");
    }
}
