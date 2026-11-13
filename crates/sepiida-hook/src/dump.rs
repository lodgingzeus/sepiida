//! Dump mode: logs every visual tree change and styles nothing. Used to
//! find element names in shell processes we do not style yet.
//!
//! Unlike styling, several attachments may dump at once: a process such as
//! ShellHost has one XAML core per flyout, each reached through its own
//! diagnostics connection, and sepiida.exe attaches to all of them. So the
//! state here is process-wide (one counter, one stop flag) rather than per
//! attachment, and no attachment replaces another.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use sepiida_core::shell::hosts::MAX_DUMPED_ELEMENTS;

use crate::{
    control,
    log::log,
    xamlom::{ParentChildRelation, VisualElement, VisualMutationType},
};

static DUMPED: AtomicUsize = AtomicUsize::new(0);
static STOPPED: AtomicBool = AtomicBool::new(false);

/// One tree change, logged as `tree add <handle> < <parent> <Type>#<Name>`
/// or `tree remove <handle>`, prefixed with the attachment's generation so
/// that cores can be told apart.
pub fn on_change(
    generation: u64,
    relation: &ParentChildRelation,
    element: &VisualElement,
    mutation_type: VisualMutationType,
) {
    if STOPPED.load(Ordering::Acquire) {
        return;
    }
    // Lets `sepiida shell dump stop` reach this thread.
    control::ensure_window();
    let n = DUMPED.fetch_add(1, Ordering::AcqRel);
    if n == MAX_DUMPED_ELEMENTS {
        log!("dump stopped after {MAX_DUMPED_ELEMENTS} tree changes");
    }
    if n >= MAX_DUMPED_ELEMENTS {
        return;
    }
    if mutation_type == VisualMutationType::ADD {
        log!(
            "g{generation} tree add {:#x} < {:#x} {}#{}",
            element.Handle,
            relation.Parent,
            element.Type,
            element.Name
        );
    } else if mutation_type == VisualMutationType::REMOVE {
        log!("g{generation} tree remove {:#x}", element.Handle);
    }
}

/// Stops every dumping attachment in this process.
pub fn stop() {
    if !STOPPED.swap(true, Ordering::AcqRel) {
        log!("dump stopped");
    }
}
