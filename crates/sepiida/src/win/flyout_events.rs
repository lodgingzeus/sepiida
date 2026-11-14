//! Hears when a sandboxed shell host shows a window: Start, quick
//! settings, the notification centre, the calendar or a toast. The
//! notification centre's XAML exists only while shown, so the agent
//! attaches then; and every host may have been given a new brush while
//! hidden (Windows swaps them on a light/dark switch), so the agent sends
//! the colours again whenever a window appears.
//!
//! Out-of-context WinEvent hooks for `EVENT_OBJECT_SHOW` and
//! `EVENT_OBJECT_UNCLOAKED` (Windows may cloak a flyout's window rather
//! than hide it) on a thread of their own (the events arrive as messages on that thread; nothing is
//! loaded into other processes). Every shown window in the session passes
//! through the callback, so it filters cheaply: whole windows only, one
//! process-name lookup per new process id (cached), and at most one report
//! per second per process.

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use sepiida_core::shell::hosts::Host;
use tracing::info;
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook},
        WindowsAndMessaging::{
            CHILDID_SELF, DispatchMessageW, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED, GetMessageW,
            GetWindowThreadProcessId, MSG, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
            WINEVENT_SKIPOWNPROCESS,
        },
    },
};

use super::processes;

/// Reports for one process closer together than this are dropped: a
/// flyout shows several windows at once.
const QUIET: Duration = Duration::from_secs(1);
/// Process ids remembered; the cache is emptied when it grows past this.
const MAX_KNOWN: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum FlyoutEventsError {
    #[error("could not start the flyout event thread")]
    Thread(#[source] std::io::Error),
    #[error("could not install the window event hook")]
    Hook,
    #[error("the flyout event thread stopped before it was ready")]
    Startup,
}

struct Watch {
    on_shown: Box<dyn Fn(Host, u32)>,
    /// Which sandboxed host each process id seen is, if any.
    known: HashMap<u32, Option<Host>>,
    last: Option<(u32, Instant)>,
}

thread_local! {
    static WATCH: RefCell<Option<Watch>> = const { RefCell::new(None) };
}

/// Starts the hook thread; `on_shown` runs on it with the host and process
/// id each time a sandboxed host shows a window. Returns once the hook is
/// in.
pub fn spawn(on_shown: impl Fn(Host, u32) + Send + 'static) -> Result<(), FlyoutEventsError> {
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::Builder::new()
        .name("flyout-events".to_owned())
        .spawn(move || {
            WATCH.with(|cell| {
                *cell.borrow_mut() = Some(Watch {
                    on_shown: Box::new(on_shown),
                    known: HashMap::new(),
                    last: None,
                });
            });
            // SAFETY: `on_event` lives for the whole program; the hooks
            // belong to this thread, which runs a message loop below.
            let hook = |event: u32| unsafe {
                SetWinEventHook(
                    event,
                    event,
                    None,
                    Some(on_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };
            let show = hook(EVENT_OBJECT_SHOW);
            let uncloak = hook(EVENT_OBJECT_UNCLOAKED);
            let ok = !show.0.is_null() || !uncloak.0.is_null();
            let _ = ready_tx.send(ok);
            if ok {
                message_loop();
            }
        })
        .map_err(FlyoutEventsError::Thread)?;
    match ready_rx.recv() {
        Ok(true) => Ok(()),
        Ok(false) => Err(FlyoutEventsError::Hook),
        Err(_) => Err(FlyoutEventsError::Startup),
    }
}

fn message_loop() {
    let mut message = MSG::default();
    // SAFETY: `message` is a valid MSG for every call. GetMessageW returns
    // 0 on WM_QUIT and -1 on error; both end the loop.
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    window: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW.0 || child != CHILDID_SELF as i32 || window.0.is_null() {
        return;
    }
    // A panic must not unwind into user32.
    let _ = std::panic::catch_unwind(|| {
        let mut pid = 0;
        // SAFETY: plain call; `pid` is writable.
        unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
        if pid != 0 {
            shown(pid, event);
        }
    });
}

fn shown(pid: u32, event: u32) {
    WATCH.with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else {
            return;
        };
        let Some(watch) = slot.as_mut() else {
            return;
        };
        if watch.known.len() > MAX_KNOWN {
            watch.known.clear();
        }
        let host = *watch.known.entry(pid).or_insert_with(|| {
            processes::exe_name(pid)
                .and_then(|name| Host::from_exe(&name))
                .filter(|host| host.is_sandboxed())
        });
        let Some(host) = host else {
            return;
        };
        let now = Instant::now();
        if watch
            .last
            .is_some_and(|(last_pid, at)| last_pid == pid && now.duration_since(at) < QUIET)
        {
            return;
        }
        watch.last = Some((pid, now));
        let kind = if event == EVENT_OBJECT_UNCLOAKED {
            "uncloaked"
        } else {
            "shown"
        };
        info!(?host, pid, kind, "shell host window appeared");
        (watch.on_shown)(host, pid);
    });
}
