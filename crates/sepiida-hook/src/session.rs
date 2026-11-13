//! The one active attachment: which diagnostics service we are watching and
//! with which callback object. Attaching again replaces it, detaching ends
//! it. Each attachment gets a new generation number, so callbacks that
//! arrive late for an old one are recognised and ignored.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use windows::{
    Win32::{
        Foundation::CloseHandle,
        System::Threading::{EVENT_MODIFY_STATE, GetCurrentProcessId, OpenEventW, SetEvent},
    },
    core::{BSTR, HSTRING, IUnknown, Interface, Result},
};

use sepiida_core::shell::{hosts::AttachMode, protocol};

use crate::{
    dump,
    log::log,
    watcher::Watcher,
    xamlom::{IVisualTreeService3, IVisualTreeServiceCallback2, IXamlDiagnostics},
};

/// Moves a COM pointer to another thread.
///
/// SAFETY: XAML Diagnostics' service objects are called from threads other
/// than the one they were handed to on: both TranslucentTB and Windhawk
/// call `AdviseVisualTreeChange`/`UnadviseVisualTreeChange` from a helper
/// thread, because calling it on the UI thread can hang. Our callback
/// object only touches thread-local state.
struct SendCom<T>(T);
unsafe impl<T> Send for SendCom<T> {}

struct Active {
    service: SendCom<IVisualTreeService3>,
    watcher: SendCom<IVisualTreeServiceCallback2>,
}

static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn active() -> std::sync::MutexGuard<'static, Option<Active>> {
    ACTIVE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether callbacks of this generation still belong to the active
/// attachment.
pub fn is_current(generation: u64) -> bool {
    GENERATION.load(Ordering::Acquire) == generation
}

/// Starts watching the visual tree behind `site` (XAML's
/// `IXamlDiagnostics`), replacing any earlier attachment.
pub fn attach(site: &IUnknown) -> Result<()> {
    let diagnostics: IXamlDiagnostics = site.cast()?;
    let service: IVisualTreeService3 = site.cast()?;
    let mode = attach_mode(&diagnostics);
    // Styling has one attachment; dump attachments (one per XAML core)
    // run side by side and are stopped together (see `dump`).
    match mode {
        AttachMode::Style => detach(),
        AttachMode::Dump => dump::started(),
    }

    let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    log!("attaching (generation {generation}, {mode:?} mode) in {exe}");
    let watcher: IVisualTreeServiceCallback2 =
        Watcher::new(generation, mode, diagnostics, service.clone()).into();
    if mode == AttachMode::Style {
        *active() = Some(Active {
            service: SendCom(service.clone()),
            watcher: SendCom(watcher.clone()),
        });
    }

    let (service, watcher) = (SendCom(service), SendCom(watcher));
    let spawned = thread::Builder::new()
        .name("sepiida-advise".into())
        .spawn(move || {
            crate::guard("advise thread", (), || {
                let (service, watcher) = (service, watcher);
                // SAFETY: both pointers are live; the callback stays alive
                // for as long as XAML holds the reference it takes here.
                let hr = unsafe { service.0.AdviseVisualTreeChange(watcher.0.as_raw()) };
                if hr.is_ok() {
                    log!("watching the visual tree");
                    signal_ready();
                } else {
                    log!("AdviseVisualTreeChange failed: {hr}");
                    signal_failed();
                }
            });
        });
    if let Err(error) = spawned {
        log!("could not start the advise thread: {error}");
    }
    Ok(())
}

/// The mode sepiida.exe asked for, passed as the initialisation string.
fn attach_mode(diagnostics: &IXamlDiagnostics) -> AttachMode {
    let mut data = BSTR::new();
    // SAFETY: `data` receives a string we own and free on drop.
    let hr = unsafe { diagnostics.GetInitializationData(&mut data) };
    if hr.is_err() {
        log!("GetInitializationData failed: {hr}");
        return AttachMode::default();
    }
    AttachMode::from_init_data(&data.to_string())
}

/// Stops the active attachment, if any. Brushes are put back by the UI
/// thread before it calls this.
pub fn detach() {
    let Some(old) = active().take() else {
        return;
    };
    GENERATION.fetch_add(1, Ordering::AcqRel);
    log!("detaching");
    let spawned = thread::Builder::new()
        .name("sepiida-unadvise".into())
        .spawn(move || {
            crate::guard("unadvise thread", (), || {
                let old = old;
                // SAFETY: both pointers are live until `old` drops below.
                let hr = unsafe {
                    old.service
                        .0
                        .UnadviseVisualTreeChange(old.watcher.0.as_raw())
                };
                if hr.is_err() {
                    log!("UnadviseVisualTreeChange failed: {hr}");
                }
            });
        });
    if let Err(error) = spawned {
        log!("could not start the unadvise thread: {error}");
    }
}

/// Tells sepiida.exe, if it is waiting, that we are attached.
fn signal_ready() {
    signal("ready", protocol::ready_event_name);
}

/// Tells sepiida.exe, if it is waiting, that this attach gave up.
pub fn signal_failed() {
    signal("failed", protocol::failed_event_name);
}

fn signal(what: &str, event_name: fn(u32) -> String) {
    // SAFETY: plain Win32 calls with a valid name; the handle is closed.
    unsafe {
        let name = HSTRING::from(event_name(GetCurrentProcessId()));
        match OpenEventW(EVENT_MODIFY_STATE, false, &name) {
            Ok(event) => {
                if let Err(error) = SetEvent(event) {
                    log!("could not signal {what}: {error}");
                }
                let _ = CloseHandle(event);
            }
            // Attached by something other than sepiida.exe.
            Err(_) => log!("nobody is waiting for the {what} signal"),
        }
    }
}
