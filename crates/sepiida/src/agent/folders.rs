//! File Explorer's folder windows, styled by the agent (M6): the DLL is
//! attached to the WinUI 3 XAML of each explorer.exe with a folder window
//! (the taskbar's own process, or a separate one), sent the palette and
//! the look after every apply, and attached when a folder window appears
//! in a process that has no copy yet. The look brings the glass and its
//! GDI hooks with it.
//!
//! An attach runs on a worker thread (it waits up to 90 s for the DLL)
//! and reports back with `Event::ExplorerAttachDone`. Turning styling off
//! restores the folder windows but leaves the DLL attached: attaching
//! again in the same process has ended Explorer (`folder_attach`).

use std::{
    sync::mpsc::Sender,
    thread,
    time::{Duration, Instant},
};

use sepiida_core::{
    Color,
    schedule::Event,
    shell::{
        attach_gate::AttachGate,
        folder_attach::{FolderAttaches, Next},
        framework::Framework,
        hosts::{AttachMode, Host},
        protocol::Command,
        rules::{Material, Role},
    },
};
use tracing::{info, warn};

use super::hosts::send_style;
use crate::win::{
    processes,
    shell_hook::{self, Outcome},
};

/// How long a freshly attached DLL gets to create its control window.
const CONTROL_WINDOW_TIMEOUT: Duration = Duration::from_secs(5);
/// Wait after a folder window appeared before attaching: its XAML comes
/// up a moment after the window.
const AFTER_SHOWN: Duration = Duration::from_millis(1500);

pub struct FolderStyling {
    events: Sender<Event>,
    /// The colours to send; empty while styling is off.
    colors: Vec<(Role, Color)>,
    material: Material,
    attaches: FolderAttaches,
}

impl FolderStyling {
    pub fn new(events: Sender<Event>) -> Self {
        Self {
            events,
            colors: Vec::new(),
            material: Material::default(),
            attaches: FolderAttaches::default(),
        }
    }

    /// Styling is on with these colours (after an apply): every process
    /// with a folder window gets them, attaching first where needed.
    pub fn sync(&mut self, colors: &[(Role, Color)], material: Material, gate: &mut AttachGate) {
        self.colors = colors.to_vec();
        self.material = material;
        let pids = shell_hook::explorer_window_pids();
        self.attaches.new_round(&pids);
        for pid in pids {
            self.style(pid, gate, Duration::ZERO);
        }
    }

    /// A folder window of explorer.exe `pid` appeared. An attached copy
    /// styles it by itself (new threads start with what it was told).
    pub fn window_shown(&mut self, pid: u32, gate: &mut AttachGate) {
        if self.colors.is_empty() || shell_hook::is_attached(pid, Framework::WinUi) {
            return;
        }
        // Control Panel's frame is a `CabinetWClass` too, without XAML: it
        // must not use up the retries.
        if shell_hook::explorer_window_pids().contains(&pid) {
            self.style(pid, gate, AFTER_SHOWN);
        }
    }

    fn style(&mut self, pid: u32, gate: &mut AttachGate, delay: Duration) {
        if !may_attach_beside_taskbar(pid) {
            info!(
                pid,
                "File Explorer waits for the taskbar's attach in its process"
            );
            return;
        }
        match self
            .attaches
            .next(pid, shell_hook::is_attached(pid, Framework::WinUi))
        {
            Next::Send => self.send(pid),
            Next::Attach => {
                // Counted before the attempt, for the crash guard: a DLL
                // that ends Explorer may do so before it reports back.
                gate.attaching(Instant::now());
                if !spawn_attach(pid, delay, self.events.clone()) {
                    self.attaches.done(pid, false);
                }
            }
            Next::Wait => {}
        }
    }

    /// The attach to `pid` ended (`Event::ExplorerAttachDone`).
    pub fn attach_done(&mut self, pid: u32, attached: bool) {
        match self.attaches.done(pid, attached) {
            _ if attached && self.colors.is_empty() => {
                // Styling went off while attaching.
                let _ = shell_hook::send(pid, Framework::WinUi, Command::Restore);
            }
            _ if attached => self.send(pid),
            Some(wait) => {
                info!(
                    pid,
                    ?wait,
                    "File Explorer attach failed; trying again later"
                );
                let events = self.events.clone();
                let spawned = thread::Builder::new()
                    .name("explorer-attach-retry".to_owned())
                    .spawn(move || {
                        thread::sleep(wait);
                        let _ = events.send(Event::ShellWindowShown(Host::Explorer, pid));
                    });
                if let Err(error) = spawned {
                    warn!("could not schedule another File Explorer attach: {error}");
                }
            }
            None => warn!(
                pid,
                "File Explorer attach failed; giving up until the next apply"
            ),
        }
    }

    fn send(&self, pid: u32) {
        match send_style(pid, Framework::WinUi, &self.colors, self.material) {
            Ok(()) => info!(pid, "File Explorer colours and look sent"),
            Err(error) => warn!(pid, "could not send the File Explorer colours: {error}"),
        }
    }

    /// Puts File Explorer's own look back in every explorer.exe; the DLL
    /// stays attached (see the module comment).
    pub fn stop(&mut self) {
        self.colors.clear();
        self.attaches.clear();
        let Ok(found) = processes::find_in_session(&[Host::Explorer.exe()]) else {
            return;
        };
        for process in found {
            match shell_hook::send(process.pid, Framework::WinUi, Command::Restore) {
                Ok(windows) => info!(pid = process.pid, windows, "File Explorer styling removed"),
                Err(shell_hook::ShellHookError::NotAttached) => {}
                Err(error) => warn!(
                    pid = process.pid,
                    "could not restore File Explorer: {error}"
                ),
            }
        }
    }
}

/// Whether `pid` may get a File Explorer attach now. The DLL runs one
/// attach at a time per process and the attach signals are named by pid,
/// so in the taskbar's explorer.exe the taskbar's copy comes first.
fn may_attach_beside_taskbar(pid: u32) -> bool {
    match shell_hook::taskbar() {
        Ok(taskbar) if taskbar.pid == pid => shell_hook::is_attached(pid, Framework::System),
        _ => true,
    }
}

/// Attaches to `pid`'s File Explorer XAML on a worker thread, after
/// `delay`, and reports the result to the agent. Returns whether the
/// thread started.
fn spawn_attach(pid: u32, delay: Duration, events: Sender<Event>) -> bool {
    info!(pid, ?delay, "attaching the styling DLL to File Explorer");
    let spawned = thread::Builder::new()
        .name("explorer-attach".to_owned())
        .spawn(move || {
            thread::sleep(delay);
            let attached = attach(pid);
            let _ = events.send(Event::ExplorerAttachDone(pid, attached));
        });
    match spawned {
        Ok(_) => true,
        Err(error) => {
            warn!("could not start the File Explorer attach: {error}");
            false
        }
    }
}

fn attach(pid: u32) -> bool {
    let attempt = || -> Result<Outcome, shell_hook::ShellHookError> {
        let dll = shell_hook::hook_dll_path()?;
        let window = shell_hook::explorer_window_of(pid)?;
        let (_, outcome) = shell_hook::attach_winui(&window, &dll, AttachMode::Style)?;
        Ok(outcome)
    };
    match attempt() {
        Ok(Outcome::Ready) => {
            if shell_hook::wait_until_attached(pid, Framework::WinUi, CONTROL_WINDOW_TIMEOUT) {
                info!(pid, "styling DLL attached to File Explorer");
                true
            } else {
                warn!(
                    pid,
                    "the DLL attached to File Explorer but created no control window"
                );
                false
            }
        }
        Ok(Outcome::Failed) => {
            info!(pid, "File Explorer's XAML is not ready yet");
            false
        }
        Ok(Outcome::Exited) => {
            info!(pid, "Explorer exited during the attach");
            false
        }
        Ok(Outcome::TimedOut) => {
            warn!(
                pid,
                "the DLL did not report back from File Explorer; see logs\\hook.log"
            );
            false
        }
        Err(error) => {
            warn!(pid, "could not attach to File Explorer: {error}");
            false
        }
    }
}
