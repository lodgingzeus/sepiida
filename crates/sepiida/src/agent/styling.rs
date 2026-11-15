//! Shell styling driven by the agent: attaches the hook DLL to Explorer
//! (the taskbar, and File Explorer's folder windows: `folders`) and the
//! sandboxed hosts (`hosts`) while the setting is on, sends them
//! the palette's colours and turns the look on after every apply, and takes the styling down
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
        framework::Framework,
        hosts::Host,
        protocol::Command,
        rules::{Material, Role},
    },
};
use tracing::{error, info, warn};

use super::{folders::FolderStyling, hosts::HostStyling};
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
    folders: FolderStyling,
}

impl ShellStyling {
    pub fn new(config_path: PathBuf, events: Sender<Event>) -> Self {
        Self {
            config_path,
            gate: AttachGate::default(),
            hosts: HostStyling::new(events.clone()),
            folders: FolderStyling::new(events),
        }
    }

    pub fn enabled(&self) -> bool {
        self.gate.enabled()
    }

    /// Called after every apply with the setting and the palette in effect.
    /// Never fails: problems are logged and the next apply tries again.
    pub fn sync(&mut self, enabled: bool, palette: &Palette, material: Material) {
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
            self.sync_taskbar(&colors, material, system_dark);
            self.folders.sync(&colors, material, &mut self.gate);
        }
        self.hosts.sync(&colors, material);
    }

    /// A window of a sandboxed host, or a folder window of explorer.exe,
    /// appeared (process `pid`).
    pub fn window_shown(&mut self, host: Host, pid: u32) {
        if !self.gate.enabled() {
            return;
        }
        if host == Host::Explorer {
            self.folders.window_shown(pid, &mut self.gate);
        } else {
            self.hosts.window_shown(host, pid);
        }
    }

    /// An attach to File Explorer's XAML in `pid` ended.
    pub fn explorer_attach_done(&mut self, pid: u32, attached: bool) {
        self.folders.attach_done(pid, attached);
    }

    /// A sandboxed host exited with `exit_code` at `at`. Turns styling off
    /// after repeated crashes right after attaching.
    pub fn host_exited(&mut self, host: Host, pid: u32, exit_code: Option<u32>, at: Instant) {
        if let Some(crashes) = self.hosts.host_exited(host, pid, exit_code, at) {
            turn_off_after_crashes(&self.config_path, crashes, host.exe());
        }
    }

    fn sync_taskbar(&mut self, colors: &[(Role, Color)], material: Material, system_dark: bool) {
        let taskbar = match shell_hook::taskbar() {
            Ok(taskbar) => taskbar,
            Err(error) => {
                warn!("shell styling: {error}");
                return;
            }
        };
        if !shell_hook::is_attached(taskbar.pid, Framework::System) && !self.attach(&taskbar) {
            return;
        }
        for &(role, color) in colors {
            if let Err(error) = shell_hook::send(
                taskbar.pid,
                Framework::System,
                Command::SetColor(role, color),
            ) {
                warn!("could not send the shell colours: {error}");
                return;
            }
            info!(?role, %color, system_dark, "shell colour sent");
        }
        // After the colours: the look's glass is made in their colour;
        // the material before the look, so the other one's rules never show.
        let sent = shell_hook::send(
            taskbar.pid,
            Framework::System,
            Command::SetMaterial(material),
        )
        .and_then(|_| shell_hook::send(taskbar.pid, Framework::System, Command::SetLook(true)));
        if let Err(error) = sent {
            warn!("could not turn the shell look on: {error}");
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
                if !shell_hook::wait_until_attached(
                    taskbar.pid,
                    Framework::System,
                    CONTROL_WINDOW_TIMEOUT,
                ) {
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

    /// Restores every copy of the DLL: the sandboxed hosts, the taskbar
    /// and File Explorer's folder windows, if any. All copies stay
    /// attached: attaching again in the same explorer.exe soon after a
    /// detach has ended it, and a second attachment in a sandboxed host
    /// cannot write values. The next sync finds each copy attached and
    /// only sends the colours and the look.
    pub fn stop(&mut self) {
        self.hosts.stop();
        self.folders.stop();
        let Ok(taskbar) = shell_hook::taskbar() else {
            return;
        };
        match shell_hook::send(taskbar.pid, Framework::System, Command::Restore) {
            Ok(windows) => info!(windows, "taskbar styling removed"),
            Err(shell_hook::ShellHookError::NotAttached) => {}
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
