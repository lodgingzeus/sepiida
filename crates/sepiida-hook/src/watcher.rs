//! Work on the XAML UI thread: the visual tree callback, the taskbar
//! finder and the styler. XAML calls the callback on the thread that owns the
//! elements, and commands arrive on the same thread through the control
//! window, so all of this state is thread-local and needs no locking.

use std::cell::RefCell;

use sepiida_core::shell::{
    finder::Finder,
    hosts::{AttachMode, MAX_DUMPED_ELEMENTS},
    protocol::Command,
    rules::TASKBAR_RULES,
};
use windows::{
    Win32::Foundation::{E_UNEXPECTED, S_OK},
    core::{HRESULT, PCWSTR, implement},
};

use crate::{
    control, guard,
    log::log,
    session,
    styler::Styler,
    xamlom::{
        IVisualTreeService3, IVisualTreeServiceCallback_Impl, IVisualTreeServiceCallback2,
        IVisualTreeServiceCallback2_Impl, IXamlDiagnostics, InstanceHandle, ParentChildRelation,
        VisualElement, VisualElementState, VisualMutationType,
    },
};

struct UiState {
    generation: u64,
    mode: AttachMode,
    finder: Finder,
    styler: Styler,
    /// Tree changes logged so far in dump mode.
    dumped: usize,
}

impl UiState {
    /// Dump mode: one log line per tree change, up to the cap. The lines
    /// are `tree add <handle> < <parent> <Type>#<Name>`, enough to rebuild
    /// the tree offline.
    fn dump(&mut self, line: std::fmt::Arguments<'_>) {
        if self.mode != AttachMode::Dump || self.dumped > MAX_DUMPED_ELEMENTS {
            return;
        }
        self.dumped += 1;
        if self.dumped > MAX_DUMPED_ELEMENTS {
            log!("dump stopped after {MAX_DUMPED_ELEMENTS} tree changes");
        } else {
            log!("{line}");
        }
    }
}

impl UiState {
    fn log_summary(&self) {
        log!(
            "state: {} elements, anchor seen: {}, {} target(s), {} near miss(es)",
            self.finder.element_count(),
            self.finder.saw_anchor(),
            self.finder.matched_count(),
            self.finder.near_miss_count()
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
    diagnostics: IXamlDiagnostics,
    service: IVisualTreeService3,
}

impl Watcher {
    pub fn new(
        generation: u64,
        mode: AttachMode,
        diagnostics: IXamlDiagnostics,
        service: IVisualTreeService3,
    ) -> Self {
        Self {
            generation,
            mode,
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
            if session::is_current(self.generation) {
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
        if slot
            .as_ref()
            .is_none_or(|s| s.generation != watcher.generation)
        {
            // Attached again without a detach: put the originals back first,
            // or the new styler would take our brush for the original.
            if let Some(old) = slot.as_mut() {
                old.styler.restore_all();
            }
            control::ensure_window();
            *slot = Some(UiState {
                generation: watcher.generation,
                mode: watcher.mode,
                finder: Finder::new(TASKBAR_RULES),
                styler: Styler::new(watcher.diagnostics.clone(), watcher.service.clone()),
                dumped: 0,
            });
        }
        let Some(state) = slot.as_mut() else {
            return;
        };

        if mutation_type == VisualMutationType::REMOVE {
            state.dump(format_args!("tree remove {:#x}", element.Handle));
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
        state.dump(format_args!(
            "tree add {:#x} < {:#x} {type_name}#{name}",
            element.Handle, relation.Parent
        ));
        if state.mode == AttachMode::Dump {
            return;
        }
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
    });
}

/// Carries out a command from sepiida.exe. Called by the control window on
/// this thread.
pub fn run_command(command: Command) {
    log!("command: {command:?}");
    let detached = STATE.with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else {
            log!("command arrived during a tree callback; ignored");
            return None;
        };
        match slot.as_ref() {
            Some(state) => state.log_summary(),
            None => log!("state: no visual tree seen on this thread"),
        }
        match command {
            Command::SetColor(role, color) => {
                if let Some(state) = slot.as_mut() {
                    state.styler.set_color(role, color);
                }
                None
            }
            Command::Restore => {
                if let Some(state) = slot.as_mut() {
                    state.styler.restore_all();
                }
                None
            }
            Command::Detach => Some(slot.take()),
        }
    });
    // Outside the borrow: restoring and destroying the window can send
    // messages back to this thread.
    if let Some(state) = detached {
        if let Some(mut state) = state {
            state.styler.restore_all();
        }
        session::detach();
        control::destroy_window();
    }
}
