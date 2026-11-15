//! Work on the XAML UI thread: the visual tree callback, the finder (with
//! this host's rules) and the styler. XAML calls the callback on the thread that owns the
//! elements, and commands arrive on the same thread through the control
//! window, so all of this state is thread-local and needs no locking.

use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

use sepiida_core::{
    Color,
    shell::{
        attach_request::AttachRequest, finder::Finder, hosts::AttachMode, protocol::Command,
        rules::Role,
    },
};
use windows::{
    Win32::Foundation::{E_UNEXPECTED, S_OK},
    core::{HRESULT, PCWSTR, implement},
};

use crate::{
    backdrop::{self, WindowBackdrop},
    control, dump, guard, host,
    log::log,
    session,
    styler::Styler,
    xamlom::{
        IVisualTreeService3, IVisualTreeServiceCallback_Impl, IVisualTreeServiceCallback2,
        IVisualTreeServiceCallback2_Impl, IXamlDiagnostics, InstanceHandle, ParentChildRelation,
        VisualElement, VisualElementState, VisualMutationType,
    },
};

/// Shortest time between two checks for swapped brushes on tree activity.
const RECHECK_EVERY: Duration = Duration::from_millis(250);

struct UiState {
    generation: u64,
    finder: Finder,
    styler: Styler,
    /// File Explorer's glass behind this thread's folder windows.
    backdrop: WindowBackdrop,
    last_recheck: Option<Instant>,
}

impl UiState {
    fn set_color(&mut self, role: Role, color: Color) {
        self.styler.set_color(role, color);
        if role == Role::Surface {
            self.backdrop.set_tint(color);
        }
    }

    fn set_look(&mut self, on: bool) {
        self.styler.set_look(on);
        self.backdrop.set_look(on);
    }

    /// Puts back everything Windows had: the original values and backdrop.
    fn restore_all(&mut self) {
        self.styler.restore_all();
        self.backdrop.restore();
    }

    fn log_summary(&self) {
        log!(
            "state: {} elements, anchor seen: {}, {} target(s), {} near miss(es), {} dropped",
            self.finder.element_count(),
            self.finder.saw_anchor(),
            self.finder.matched_count(),
            self.finder.near_miss_count(),
            self.finder.pending_dropped_count()
        );
    }
}

thread_local! {
    static STATE: RefCell<Option<UiState>> = const { RefCell::new(None) };
}

#[implement(IVisualTreeServiceCallback2)]
pub struct Watcher {
    generation: u64,
    mode: AttachMode,
    /// Colours the attach brought, given to each thread's styler first.
    request: AttachRequest,
    diagnostics: IXamlDiagnostics,
    service: IVisualTreeService3,
}

impl Watcher {
    pub fn new(
        generation: u64,
        request: AttachRequest,
        diagnostics: IXamlDiagnostics,
        service: IVisualTreeService3,
    ) -> Self {
        Self {
            generation,
            mode: request.mode,
            request,
            diagnostics,
            service,
        }
    }
}

impl IVisualTreeServiceCallback_Impl for Watcher_Impl {
    unsafe fn OnVisualTreeChange(
        &self,
        relation: ParentChildRelation,
        element: VisualElement,
        mutation_type: VisualMutationType,
    ) -> HRESULT {
        guard("OnVisualTreeChange", E_UNEXPECTED, || {
            if self.mode == AttachMode::Dump {
                dump::on_change(
                    self.generation,
                    self.request.framework,
                    &relation,
                    &element,
                    mutation_type,
                );
            } else if session::is_current(self.generation) {
                on_change(self, relation, &element, mutation_type);
            }
            // `element` drops here and frees its strings, which XAML handed
            // over to us.
            S_OK
        })
    }
}

impl IVisualTreeServiceCallback2_Impl for Watcher_Impl {
    unsafe fn OnElementStateChanged(
        &self,
        _element: InstanceHandle,
        _state: VisualElementState,
        _context: PCWSTR,
    ) -> HRESULT {
        S_OK
    }
}

fn on_change(
    watcher: &Watcher,
    relation: ParentChildRelation,
    element: &VisualElement,
    mutation_type: VisualMutationType,
) {
    STATE.with(|cell| {
        // A styling call could in theory call back into us on this thread;
        // skip the event rather than panic on a second borrow.
        let Ok(mut slot) = cell.try_borrow_mut() else {
            log!("re-entrant tree callback skipped");
            return;
        };
        if let Some(owner) = slot.as_ref().map(|s| s.generation)
            && owner != watcher.generation
            && session::is_current(owner)
        {
            // Another live attachment (one beside us on a further XAML
            // core) already styles this thread; two stylers would fight.
            return;
        }
        if slot
            .as_ref()
            .is_none_or(|s| s.generation != watcher.generation)
        {
            // Attached again without a detach: put the originals back first,
            // or the new styler would take our brush for the original.
            if let Some(old) = slot.as_mut() {
                old.restore_all();
            }
            let framework = watcher.request.framework;
            control::ensure_window(framework);
            let rules = host::rules(framework);
            log!(
                "{:?} ({framework:?} XAML): {} rule(s) on this thread",
                host::current(),
                rules.len()
            );
            *slot = Some(UiState {
                generation: watcher.generation,
                finder: Finder::new(rules),
                styler: Styler::new(
                    watcher.diagnostics.clone(),
                    watcher.service.clone(),
                    framework,
                ),
                backdrop: WindowBackdrop::new(framework),
                last_recheck: None,
            });
            if let Some(state) = slot.as_mut() {
                start_like_the_others(state, watcher);
            }
        }
        let Some(state) = slot.as_mut() else {
            return;
        };

        if mutation_type == VisualMutationType::REMOVE {
            let gone: Vec<_> = state
                .finder
                .removed(element.Handle)
                .into_iter()
                .map(|m| m.handle)
                .collect();
            state.styler.forget(&gone);
            return;
        }
        if mutation_type != VisualMutationType::ADD {
            return;
        }

        let name = element.Name.to_string();
        let type_name = element.Type.to_string();
        let added = state
            .finder
            .added(element.Handle, relation.Parent, &type_name, &name);
        if let Some(anchor) = added.first_anchor {
            log!("found {anchor} ({type_name})");
        }
        if let Some(path) = added.near_miss {
            log!("candidate not under its required ancestor (yet): {path}");
        }
        for found in added.matched {
            if let Some(rule) = state.finder.rule(found.rule) {
                state.styler.found(found, rule);
            }
        }
        if state.styler.look_on() {
            control::schedule_relayout();
        }
        let now = Instant::now();
        if state
            .last_recheck
            .is_none_or(|at| now.duration_since(at) >= RECHECK_EVERY)
        {
            state.last_recheck = Some(now);
            state.styler.recheck_colors();
        }
    });
    backdrop::flush();
}

/// Gives a new thread's styler the colours the attach brought, then what
/// the attachment has been told since (a window opened after the last
/// command). Kept until targets appear: none exists yet.
fn start_like_the_others(state: &mut UiState, watcher: &Watcher) {
    for &(role, color) in watcher.request.colors() {
        state.set_color(role, color);
    }
    let carried = session::carried(watcher.generation);
    for &(role, color) in carried.colors() {
        state.set_color(role, color);
    }
    for &(role, opacity) in carried.opacities() {
        state.styler.set_opacity(role, opacity);
    }
    if carried.look() {
        log!("look on, as on the attachment's other threads");
        state.set_look(true);
    }
}

/// Re-measures the look's layout and puts the glass back where Explorer
/// reset it or on a window that has none yet (the control window's timer).
pub fn relayout() {
    STATE.with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut()
            && let Some(state) = slot.as_mut()
        {
            state.styler.relayout();
            state.backdrop.refresh();
        }
    });
    backdrop::flush();
}

/// Carries out a command from sepiida.exe. Called by the control window on
/// this thread.
pub fn run_command(command: Command) {
    log!("command: {command:?}");
    if command == Command::Detach {
        dump::stop();
    }
    let detached = STATE.with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else {
            log!("command arrived during a tree callback; ignored");
            return None;
        };
        match slot.as_ref() {
            Some(state) => {
                state.log_summary();
                session::record(state.generation, command);
            }
            None => log!("state: no visual tree seen on this thread"),
        }
        match command {
            Command::SetColor(role, color) => {
                if let Some(state) = slot.as_mut() {
                    state.set_color(role, color);
                }
                None
            }
            Command::SetTintOpacity(role, opacity) => {
                if let Some(state) = slot.as_mut() {
                    state.styler.set_opacity(role, opacity);
                }
                None
            }
            Command::SetLook(on) => {
                if let Some(state) = slot.as_mut() {
                    state.set_look(on);
                    if on {
                        // Again once the new layout has settled.
                        control::schedule_relayout();
                    }
                }
                None
            }
            Command::SetGdiAlpha(on) => {
                if let Some(state) = slot.as_mut() {
                    state.backdrop.set_gdi(on);
                }
                None
            }
            Command::Restore => {
                if let Some(state) = slot.as_mut() {
                    state.restore_all();
                }
                None
            }
            Command::Detach => Some(slot.take()),
        }
    });
    backdrop::flush();
    // Outside the borrow: restoring and destroying the window can send
    // messages back to this thread.
    if let Some(state) = detached {
        if let Some(mut state) = state {
            state.restore_all();
            backdrop::flush();
        }
        session::detach(control::framework());
        control::destroy_window();
    }
}
