//! Shell styling driven by the agent: attaches the hook DLL to Explorer
//! and the sandboxed hosts (`hosts`) while the setting is on, sends them
//! the palette's colours after every apply, and takes the styling down
//! when the setting goes off or the agent quits. Crash guards turn the
//! setting off if Explorer or a host keeps dying soon after an attach.

use std::{
    path::{Path, PathBuf},
    sync::mpsc::Sender,
    thread,
    time::{Duration, Instant},
};

use sepiida_core::{
    Color, Palette,
    accent::ThemeMode,
    config::Config,
    schedule::Event,
    shell::{
        attach_gate::{AttachGate, Decision},
        colors::role_colors,
        crash_guard::Verdict,
        hosts::Host,
        protocol::Command,
        rules::Role,
    },
};
use tracing::{error, info, warn};

use super::hosts::HostStyling;
use crate::win::{
    message_box,
    shell_hook::{self, Method, Outcome, Taskbar},
    theme,
};

/// How long a freshly attached DLL gets to create its control window.
const CONTROL_WINDOW_TIMEOUT: Duration = Duration::from_secs(3);
/// When the agent looks again after an Explorer restart, counted from the
/// restart. One-off timers after an event, like the tray's re-adds; a
/// look that finds the DLL attached only resends the colours. Explorer's
/// XAML took over 25 s to come up once on the test VM.
pub const RETRIES_AFTER_RESTART: [Duration; 5] = [
    Duration::from_secs(4),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

pub struct ShellStyling {
    config_path: PathBuf,
    gate: AttachGate,
    hosts: HostStyling,
}

impl ShellStyling {
    pub fn new(config_path: PathBuf, events: Sender<Event>) -> Self {
        Self {
            config_path,
            gate: AttachGate::default(),
            hosts: HostStyling::new(events),
        }
    }

    pub fn enabled(&self) -> bool {
        self.gate.enabled()
    }

    /// Called after every apply with the setting and the palette in effect.
    /// Never fails: problems are logged and the next apply tries again.
    pub fn sync(&mut self, enabled: bool, palette: &Palette) {
        let decision = self.gate.decide(enabled, Instant::now());
        if decision == Decision::Stop {
            self.stop();
            return;
        }
        let system_dark = theme::system_mode() == ThemeMode::Dark;
        let colors = role_colors(palette, system_dark);
        if decision == Decision::Wait {
            info!("Explorer restarted moments ago; attaching to it on the next look");
        } else {
            self.sync_taskbar(&colors, system_dark);
        }
        self.hosts.sync(&colors);
    }

    /// A ShellExperienceHost window appeared (process `pid`).
    pub fn flyout_shown(&mut self, pid: u32) {
        if self.gate.enabled() {
            self.hosts.flyout_shown(pid);
        }
    }

    /// A sandboxed host exited at `at`. Turns styling off after repeated
    /// exits right after attaching.
    pub fn host_exited(&mut self, host: Host, pid: u32, at: Instant) {
        if let Some(crashes) = self.hosts.host_exited(host, pid, at) {
            turn_off_after_crashes(&self.config_path, crashes, host.exe());
        }
    }

    fn sync_taskbar(&mut self, colors: &[(Role, Color)], system_dark: bool) {
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
        for &(role, color) in colors {
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
        self.gate.attaching(Instant::now());
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
                match attached.outcome {
                    Outcome::Ready => {}
                    Outcome::Failed => {
                        info!("Explorer's XAML is not ready yet; trying again on the next look");
                        return false;
                    }
                    Outcome::Exited => {
                        info!("Explorer exited during the attach");
                        return false;
                    }
                    Outcome::TimedOut => {
                        warn!("the styling DLL did not report back; see logs\\hook.log");
                        return false;
                    }
                }
                if !shell_hook::wait_until_attached(taskbar.pid, CONTROL_WINDOW_TIMEOUT) {
                    warn!(
                        "the styling DLL attached but created no control window; see logs\\hook.log"
                    );
                    return false;
                }
                info!("styling DLL attached");
                true
            }
            Err(error) => {
                warn!("could not attach the styling DLL: {error}");
                false
            }
        }
    }

    /// Explorer restarted at `at`; the styling went with the old process.
    /// Turns styling off after repeated crashes right after attaching.
    pub fn explorer_restarted(&mut self, at: Instant) {
        if self.gate.explorer_restarted(at) == Verdict::Disable {
            turn_off_after_crashes(&self.config_path, self.gate.crashes(), Host::Explorer.exe());
        }
    }

    /// Restores and detaches every copy of the DLL in Explorer and the
    /// sandboxed hosts, if any.
    pub fn stop(&mut self) {
        self.hosts.stop();
        let Ok(taskbar) = shell_hook::taskbar() else {
            return;
        };
        match shell_hook::detach_all(taskbar.pid) {
            Ok(0) => {}
            Ok(windows) => info!(windows, "taskbar styling removed"),
            Err(error) => warn!("could not remove the taskbar styling: {error}"),
        }
    }
}

/// Saves the setting as off (the agent's settings watcher then applies the
/// change) and tells the user.
fn turn_off_after_crashes(config_path: &Path, crashes: u32, process: &'static str) {
    error!(
        crashes,
        process,
        "the process restarted right after the styling DLL attached, again; turning shell styling off"
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
        .spawn(move || {
            message_box::error(&format!(
                "{process} stopped twice right after sepiida restyled it, so shell \
                 styling has been turned off.\n\nYou can turn it back on in \
                 sepiida's settings. The details are in \
                 %LOCALAPPDATA%\\sepiida\\logs\\hook.log."
            ));
        });
    if let Err(error) = spawned {
        warn!("could not show the notice: {error}");
    }
}
