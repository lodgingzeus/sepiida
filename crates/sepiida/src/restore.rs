//! `sepiida restore`: put back the accent values sepiida replaced.

use anyhow::{Context, Result};
use sepiida_core::snapshot::{RestoreAction, Snapshot};

use crate::{paths::Paths, win::theme};

pub fn run_cli(dry_run: bool) -> Result<()> {
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
