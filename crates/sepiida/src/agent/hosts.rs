//! Shell styling of the sandboxed hosts, driven by the agent: Start and
//! quick settings (ShellHost) stay attached while styling is on, and the
//! notification centre, calendar and toasts (ShellExperienceHost) are
//! attached when one of their windows appears, since their XAML only
//! exists while shown.
//!
//! Whenever a host's window appears the colours are sent again, at once
//! and twice more shortly after: Windows gives a surface a new brush on a
//! light/dark switch made while it was hidden, and sets the notification
//! centre's brush only a moment into its opening; colours sent before
//! that land on a brush nobody sees (M4 step 3 round 3).
//!
//! The colours travel with every attach: a host that is hidden runs the
//! DLL's tree callbacks only once it is shown, and has no control window
//! to send colours to until then. A host that already has a control
//! window gets the colours as messages instead.
//!
//! Each host process is watched for its exit (a wait on its handle, no
//! polling); the agent then looks again a little later to attach to the
//! new process. A host that keeps crashing right after an attach trips its
//! crash guard, like Explorer does; an exit whose code shows no crash
//! (Windows ending or restarting the host) is not counted.
//!
//! A hidden host attached with the current colours is not attached again:
//! it picks them up from that attach once shown. Only new colours need a
//! new attach, as it has no control window to send them to yet.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc::Sender,
    thread,
    time::{Duration, Instant},
};

use sepiida_core::{
    Color,
    schedule::Event,
    shell::{
        crash_guard::{CrashGuard, Verdict, is_crash_exit},
        hosts::Host,
        protocol::Command,
        rules::Role,
    },
};
use tracing::{info, warn};

use crate::win::{
    file_access, processes,
    shell_hook::{self, ShellHookError},
};

/// Hosts kept attached whenever styling is on: their XAML is always up.
const KEPT_ATTACHED: [Host; 2] = [Host::StartMenu, Host::ShellHost];
/// When the agent looks again after a host exited, counted from the exit.
/// Windows restarts these hosts within a second or two.
const RETRIES_AFTER_EXIT: [Duration; 2] = [Duration::from_secs(3), Duration::from_secs(15)];
/// A flyout's XAML may come up a moment after its window: attach tries.
const FLYOUT_TRIES: u32 = 3;
const FLYOUT_RETRY: Duration = Duration::from_millis(300);
/// When the colours are sent again after a window appeared, counted from
/// the appearance: during and after its opening animation.
const RESENDS_AFTER_SHOW: [Duration; 2] = [Duration::from_millis(400), Duration::from_millis(1500)];

/// How one attach went.
enum Attach {
    Done,
    /// No XAML diagnostics port answered (yet).
    NoPort,
    Failed,
}

pub struct HostStyling {
    events: Sender<Event>,
    /// The colours to attach with; empty while styling is off.
    colors: Vec<(Role, Color)>,
    /// The DLL was made readable to app packages in this run.
    granted: bool,
    /// Processes whose exit is being waited for.
    watched: HashSet<u32>,
    /// Processes attached in this run, with the colours they were attached
    /// with. One that has no control window yet is hidden, not detached:
    /// it creates the window once shown.
    attached: HashMap<u32, Vec<(Role, Color)>>,
    guards: HashMap<Host, CrashGuard>,
}

impl HostStyling {
    pub fn new(events: Sender<Event>) -> Self {
        Self {
            events,
            colors: Vec::new(),
            granted: false,
            watched: HashSet::new(),
            attached: HashMap::new(),
            guards: HashMap::new(),
        }
    }

    /// Styling is on with these colours: attaches the hosts kept attached
    /// and sends the colours to every attached host.
    pub fn sync(&mut self, colors: &[(Role, Color)]) {
        self.colors = colors.to_vec();
        let Some(dll) = self.dll() else {
            return;
        };
        for host in Host::ALL.into_iter().filter(|h| h.is_sandboxed()) {
            let found = match processes::find_in_session(&[host.exe()]) {
                Ok(found) => found,
                Err(error) => {
                    warn!(?host, "could not look for the process: {error}");
                    continue;
                }
            };
            for process in found {
                if shell_hook::is_attached(process.pid) {
                    self.send_colors(host, process.pid);
                } else if KEPT_ATTACHED.contains(&host) && !self.has_current_colors(process.pid) {
                    self.attach(host, process.pid, &dll);
                }
            }
        }
    }

    /// A window of `host` appeared while styling is on: attaches if
    /// needed, and sends the colours now and again shortly after.
    pub fn window_shown(&mut self, host: Host, pid: u32) {
        if self.colors.is_empty() {
            return;
        }
        if shell_hook::is_attached(pid) {
            self.send_colors(host, pid);
        } else if self.attached.contains_key(&pid) {
            // Attached while hidden; its control window comes with the
            // first tree callbacks, before the re-sends below.
            info!(?host, pid, "attached host shown for the first time");
        } else if !self.attach_shown(host, pid) {
            return;
        }
        self.resend_later(pid);
    }

    /// Attaches to a host whose window just appeared; its XAML may come up
    /// a moment after the window. Returns whether it attached.
    fn attach_shown(&mut self, host: Host, pid: u32) -> bool {
        let Some(dll) = self.dll() else {
            return false;
        };
        for _ in 0..FLYOUT_TRIES {
            match self.attach(host, pid, &dll) {
                Attach::NoPort => thread::sleep(FLYOUT_RETRY),
                Attach::Done => return true,
                Attach::Failed => return false,
            }
        }
        false
    }

    /// Sends the colours to `pid` again after each of `RESENDS_AFTER_SHOW`,
    /// from a short-lived thread (posting needs no agent state).
    fn resend_later(&self, pid: u32) {
        let colors = self.colors.clone();
        let spawned = thread::Builder::new()
            .name("shell-resend".to_owned())
            .spawn(move || {
                let mut waited = Duration::ZERO;
                for at in RESENDS_AFTER_SHOW {
                    thread::sleep(at.saturating_sub(waited));
                    waited = at;
                    for &(role, color) in &colors {
                        // No control window yet (still opening) or gone:
                        // skip this round, the next may find one.
                        if shell_hook::send(pid, Command::SetColor(role, color)).is_err() {
                            break;
                        }
                    }
                }
            });
        if let Err(error) = spawned {
            warn!("could not schedule sending the colours again: {error}");
        }
    }

    /// A watched host exited with `exit_code` at `at`. Returns the crash
    /// count if styling should be turned off because the host keeps
    /// crashing right after an attach.
    pub fn host_exited(
        &mut self,
        host: Host,
        pid: u32,
        exit_code: Option<u32>,
        at: Instant,
    ) -> Option<u32> {
        self.watched.remove(&pid);
        self.attached.remove(&pid);
        let crashed = is_crash_exit(exit_code);
        info!(
            ?host,
            pid,
            exit_code = exit_code.map(|code| format!("{code:#x}")),
            crashed,
            "shell host exited"
        );
        if self.colors.is_empty() {
            return None;
        }
        let guard = self.guards.entry(host).or_default();
        if !crashed {
            guard.exited_cleanly();
            return None;
        }
        (guard.explorer_restarted(at) == Verdict::Disable).then(|| guard.crashes())
    }

    /// Whether `pid` was attached in this run with the colours in use now.
    fn has_current_colors(&self, pid: u32) -> bool {
        self.attached.get(&pid) == Some(&self.colors)
    }

    /// Restores and detaches every copy of the DLL in the sandboxed hosts.
    pub fn stop(&mut self) {
        self.colors.clear();
        self.attached.clear();
        for guard in self.guards.values_mut() {
            guard.reset();
        }
        let hosts: Vec<&str> = Host::ALL
            .into_iter()
            .filter(|h| h.is_sandboxed())
            .map(Host::exe)
            .collect();
        let Ok(found) = processes::find_in_session(&hosts) else {
            return;
        };
        for process in found {
            match shell_hook::detach_all(process.pid) {
                Ok(0) => {}
                Ok(windows) => info!(host = process.name, windows, "shell styling removed"),
                Err(error) => warn!(host = process.name, "could not remove the styling: {error}"),
            }
        }
    }

    fn attach(&mut self, host: Host, pid: u32, dll: &std::path::Path) -> Attach {
        self.watch_exit(host, pid);
        // Counted before the attempt: a DLL that crashes the host may do
        // so before anything reports back.
        self.guards
            .entry(host)
            .or_default()
            .attached(Instant::now());
        match shell_hook::attach_sandboxed(pid, dll, &self.colors, Duration::ZERO) {
            Ok(attached) => {
                self.attached.insert(pid, self.colors.clone());
                info!(
                    ?host,
                    pid,
                    connections = attached.answered.len(),
                    "attached the styling DLL"
                );
                Attach::Done
            }
            Err(ShellHookError::NoConnection) => {
                info!(?host, pid, "no XAML diagnostics port answered yet");
                Attach::NoPort
            }
            Err(error) => {
                warn!(?host, pid, "could not attach the styling DLL: {error}");
                Attach::Failed
            }
        }
    }

    fn send_colors(&self, host: Host, pid: u32) {
        for &(role, color) in &self.colors {
            if let Err(error) = shell_hook::send(pid, Command::SetColor(role, color)) {
                warn!(?host, pid, "could not send the shell colours: {error}");
                return;
            }
        }
        info!(?host, pid, "shell colours sent");
    }

    /// Reports the exit of `pid` to the agent, then asks it to look again
    /// a little later, when Windows has started the host again.
    fn watch_exit(&mut self, host: Host, pid: u32) {
        if !self.watched.insert(pid) {
            return;
        }
        let events = self.events.clone();
        processes::wait_for_exit(pid, "shell-host-exit", move |exit_code| {
            let _ = events.send(Event::ShellHostExited(host, pid, exit_code, Instant::now()));
            for delay in RETRIES_AFTER_EXIT {
                let events = events.clone();
                let spawned = thread::Builder::new()
                    .name("shell-host-recheck".to_owned())
                    .spawn(move || {
                        thread::sleep(delay);
                        info!(trigger = "shell host restart recheck", "change noticed");
                        let _ = events.send(Event::Changed);
                    });
                if let Err(error) = spawned {
                    warn!("could not schedule a look after the host exit: {error}");
                }
            }
        });
    }

    /// The hook DLL, readable to app packages (granted once per run).
    fn dll(&mut self) -> Option<PathBuf> {
        let dll = match shell_hook::hook_dll_path() {
            Ok(dll) => dll,
            Err(error) => {
                warn!("shell styling: {error}");
                return None;
            }
        };
        if !self.granted {
            if let Err(error) = file_access::grant_app_packages_read_dll(&dll) {
                warn!(
                    "could not make the DLL readable to the sandboxed shell: {:#}",
                    anyhow::Error::new(error)
                );
                return None;
            }
            self.granted = true;
        }
        Some(dll)
    }
}
