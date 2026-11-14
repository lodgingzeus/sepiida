//! `sepiida restore`: put back the accent values sepiida replaced, and the
//! shell's own look.

use anyhow::{Context, Result};
use sepiida_core::{
    shell::hosts::SANDBOXED_HOSTS,
    snapshot::{RestoreAction, Snapshot},
};

use crate::{
    paths::Paths,
    win::{processes, shell_hook, theme},
};

pub fn run_cli(dry_run: bool) -> Result<()> {
    if !dry_run {
        remove_shell_styling();
    }
    restore_accent(dry_run)
}

/// Best effort: no shell process or no DLL attached means nothing to
/// remove. Covers the taskbar and the sandboxed hosts (Start, quick
/// settings, the notification centre).
fn remove_shell_styling() {
    let mut targets = Vec::new();
    if let Ok(taskbar) = shell_hook::taskbar() {
        targets.push(("taskbar".to_owned(), taskbar.pid));
    }
    match processes::find_in_session(&SANDBOXED_HOSTS) {
        Ok(found) => targets.extend(found.into_iter().map(|p| (p.name, p.pid))),
        Err(error) => eprintln!("warning: could not list the shell processes: {error}"),
    }
    for (name, pid) in targets {
        match shell_hook::detach_all(pid) {
            Ok(0) => {}
            Ok(_) => println!("{name} styling removed"),
            Err(error) => eprintln!("warning: could not remove the {name} styling: {error}"),
        }
    }
}

fn restore_accent(dry_run: bool) -> Result<()> {
    let paths = Paths::locate()?;
    let snapshot = Snapshot::load(&paths.saved_theme)
        .with_context(|| format!("could not use {}", paths.saved_theme.display()))?;
    let actions = snapshot.restore_actions();
    if actions.is_empty() {
        println!("nothing to restore");
        // An empty or all-unknown snapshot has done its job either way.
        if !dry_run {
            Snapshot::discard(&paths.saved_theme)?;
        }
        return Ok(());
    }
    for action in &actions {
        match action {
            RestoreAction::Set(write) => println!(
                "set        HKCU\\{}\\{} = {}",
                write.key, write.name, write.data
            ),
            RestoreAction::Delete { key, name } => println!("delete     HKCU\\{key}\\{name}"),
        }
    }
    if dry_run {
        println!("dry run: nothing was changed");
        return Ok(());
    }
    theme::restore(&actions)?;
    // Kept on failure so the restore can be tried again.
    Snapshot::discard(&paths.saved_theme)?;
    println!("restored");
    Ok(())
}
