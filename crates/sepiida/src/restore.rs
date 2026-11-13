//! `sepiida restore`: put back the accent values sepiida replaced, and the
//! taskbar's own look.

use anyhow::{Context, Result};
use sepiida_core::snapshot::{RestoreAction, Snapshot};

use crate::{
    paths::Paths,
    win::{shell_hook, theme},
};

pub fn run_cli(dry_run: bool) -> Result<()> {
    if !dry_run {
        remove_shell_styling();
    }
    restore_accent(dry_run)
}

/// Best effort: no Explorer or no DLL attached means nothing to remove.
fn remove_shell_styling() {
    let Ok(taskbar) = shell_hook::taskbar() else {
        return;
    };
    match shell_hook::detach_all(taskbar.pid) {
        Ok(0) => {}
        Ok(_) => println!("taskbar styling removed"),
        Err(error) => eprintln!("warning: could not remove the taskbar styling: {error}"),
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
