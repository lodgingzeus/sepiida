//! `sepiida shell dump [collect|stop]`: logs the XAML element tree of the
//! sandboxed shell hosts (Start, notification centre, quick settings,
//! calendar) so that styling rules can be written for them.
//!
//! The hosts are AppContainer processes. The DLL is made readable to them,
//! attached from sepiida.exe in dump mode, and writes its log to the host's
//! own temp folder (`%LOCALAPPDATA%\Packages\<package>\AC\Temp`), which
//! `collect` copies into sepiida's log folder.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use directories::BaseDirs;
use sepiida_core::shell::hosts::{FALLBACK_LOG_FILE, SANDBOXED_HOSTS};

use crate::{
    cli::DumpAction,
    paths::Paths,
    win::{file_access, processes, shell_hook},
};

pub fn run(action: DumpAction) -> Result<()> {
    match action {
        DumpAction::Start => start(),
        DumpAction::Collect => collect(),
        DumpAction::Stop => stop(),
    }
}

fn start() -> Result<()> {
    let dll = shell_hook::hook_dll_path()?;
    file_access::grant_app_packages_read(&dll)?;
    println!("{} is now readable by sandboxed apps", dll.display());

    let hosts = processes::find_in_session(&SANDBOXED_HOSTS)?;
    if hosts.is_empty() {
        println!("none of {} is running", SANDBOXED_HOSTS.join(", "));
        return Ok(());
    }
    for host in &hosts {
        match shell_hook::attach_dump(host.pid, &dll) {
            Ok((connection, hr)) => println!(
                "{} (pid {}): VisualDiagConnection{connection} returned {:#010x}",
                host.name, host.pid, hr.0
            ),
            Err(error) => println!("{} (pid {}): {error:#}", host.name, host.pid),
        }
    }
    println!(
        "\nNow open each surface once, so its elements are created: Start (also \
         \"All\" apps), the notification centre (Win+N), the calendar, quick settings \
         (Win+A). Then run `sepiida shell dump collect`."
    );
    Ok(())
}

fn stop() -> Result<()> {
    let hosts = processes::find_in_session(&SANDBOXED_HOSTS)?;
    let mut asked = 0;
    for host in &hosts {
        match shell_hook::detach_all(host.pid) {
            Ok(windows) => asked += windows,
            Err(error) => println!("{} (pid {}): {error}", host.name, host.pid),
        }
    }
    println!("asked {asked} dumping copies to stop; they stay loaded until the host restarts");
    Ok(())
}

fn collect() -> Result<()> {
    let dirs = BaseDirs::new().context("could not find the user profile folders")?;
    let logs = Paths::locate()?.log_dir;
    fs::create_dir_all(&logs).with_context(|| format!("could not create {}", logs.display()))?;
    let found = find_dump_logs(&dirs.data_local_dir().join("Packages"));
    if found.is_empty() {
        println!(
            "no dump logs found under %LOCALAPPDATA%\\Packages; if a host could write \
             sepiida's own log folder, its lines are in {}",
            logs.join("hook.log").display()
        );
        return Ok(());
    }
    for (package, source) in found {
        let target = logs.join(format!("dump-{package}.log"));
        fs::copy(&source, &target).with_context(|| {
            format!(
                "could not copy {} to {}",
                source.display(),
                target.display()
            )
        })?;
        println!("{}", target.display());
    }
    Ok(())
}

/// `(package folder name, log path)` for every package whose AppContainer
/// temp folder holds a DLL log.
fn find_dump_logs(packages: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(packages) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let log = entry.path().join("AC").join("Temp").join(FALLBACK_LOG_FILE);
            let package = entry.file_name().to_str()?.to_owned();
            log.is_file().then_some((package, log))
        })
        .collect();
    found.sort();
    found
}
