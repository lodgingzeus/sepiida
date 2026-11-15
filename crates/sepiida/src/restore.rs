//! `sepiida restore`: put back the accent values sepiida replaced, and the
//! shell's own look.

use anyhow::{Context, Result};
use sepiida_core::{
    shell::{
        framework::Framework,
        hosts::{Host, SANDBOXED_HOSTS},
        protocol::Command,
    },
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
/// remove. Covers the taskbar, File Explorer and the sandboxed hosts
/// (Start, quick settings, the notification centre). Every copy is
/// restored, not detached: attaching again in the same explorer.exe has
/// ended it, and in a sandboxed host a second attachment cannot write any
/// value (XAML answers "element not found"), which left quick settings
/// half styled.
fn remove_shell_styling() {
    if let Ok(taskbar) = shell_hook::taskbar() {
        match shell_hook::send(taskbar.pid, Framework::System, Command::Restore) {
            Ok(_) => println!("taskbar styling removed"),
            Err(shell_hook::ShellHookError::NotAttached) => {}
            Err(error) => eprintln!("warning: could not remove the taskbar styling: {error}"),
        }
    }
    if let Ok(found) = processes::find_in_session(&[Host::Explorer.exe()]) {
        for process in found {
            if let Ok(windows) = shell_hook::send(process.pid, Framework::WinUi, Command::Restore) {
                println!(
                    "File Explorer styling removed (pid {}, {windows} thread(s))",
                    process.pid
                );
            }
        }
    }
    let found = match processes::find_in_session(&SANDBOXED_HOSTS) {
        Ok(found) => found,
        Err(error) => {
            eprintln!("warning: could not list the shell processes: {error}");
            return;
        }
    };
    for process in found {
        match shell_hook::send(process.pid, Framework::System, Command::Restore) {
            Ok(_) => println!("{} styling removed", process.name),
            Err(shell_hook::ShellHookError::NotAttached) => {}
            Err(error) => eprintln!(
                "warning: could not remove the {} styling: {error}",
                process.name
            ),
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
