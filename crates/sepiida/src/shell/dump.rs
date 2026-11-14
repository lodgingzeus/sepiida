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
    thread,
    time::Duration,
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
        DumpAction::Start { delay_secs } => start(delay_secs),
        DumpAction::Collect => collect(),
        DumpAction::Stop => stop(),
    }
}

fn start(delay_secs: u32) -> Result<()> {
    let dll = shell_hook::hook_dll_path()?;
    file_access::grant_app_packages_read_dll(&dll)?;
    println!(
        "{} and its folder are now readable by sandboxed apps",
        dll.display()
    );

    if delay_secs > 0 {
        println!("attaching in {delay_secs} s: open the flyout to dump now (Win+N, the clock)");
        thread::sleep(Duration::from_secs(u64::from(delay_secs)));
    }
    let hosts = processes::find_in_session(&SANDBOXED_HOSTS)?;
    if hosts.is_empty() {
        println!("none of {} is running", SANDBOXED_HOSTS.join(", "));
        return Ok(());
    }
    for host in &hosts {
        match shell_hook::attach_dump(host.pid, &dll) {
            Ok(answered) if answered.is_empty() => println!(
                "{} (pid {}): no XAML diagnostics port answered",
                host.name, host.pid
            ),
            Ok(answered) => {
                for (connection, hr) in answered {
                    println!(
                        "{} (pid {}): VisualDiagConnection{connection} returned {:#010x}",
                        host.name, host.pid, hr.0
                    );
                }
            }
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

/// `(name, log path)` for every DLL log inside a package's AppContainer
/// folder. Inside an AppContainer `%LOCALAPPDATA%` may already point at
/// that folder, so the DLL's paths can end up nested in it; each place it
/// can write to is checked (no directory walk).
fn find_dump_logs(packages: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(packages) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let Some(package) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let ac = entry.path().join("AC");
        let candidates = [
            ac.join("Temp").join(FALLBACK_LOG_FILE),
            ac.join("sepiida").join("logs").join("hook.log"),
            ac.join("Packages")
                .join(&package)
                .join("AC")
                .join("Temp")
                .join(FALLBACK_LOG_FILE),
        ];
        for (n, log) in candidates.into_iter().enumerate() {
            if log.is_file() {
                let name = if n == 0 {
                    package.clone()
                } else {
                    format!("{package}-{n}")
                };
                found.push((name, log));
            }
        }
    }
    found.sort();
    found
}
