//! `sepiida shell attach <surface>`, and the commands that reach every
//! attached shell process: `color`, `opacity`, `restore`, `detach`.
//!
//! The sandboxed hosts (Start, quick settings, the notification centre)
//! get the DLL in style mode from sepiida.exe, like `shell dump` does; the
//! DLL picks the rules of the process it lands in.

use std::{thread, time::Duration};

use anyhow::{Result, bail};
use sepiida_core::shell::{
    hosts::{Host, SANDBOXED_HOSTS},
    protocol::Command,
};

use crate::win::{
    file_access, processes,
    shell_hook::{self, SandboxAttached, ShellHookError},
};

/// How long to wait for the DLL's control window after an attach. A host
/// that is running (not suspended) answers within a second.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);

const LOG_HINT: &str = "`sepiida shell dump collect` copies the hosts' logs into \
                        %LOCALAPPDATA%\\sepiida\\logs";

pub fn attach(host: Host, delay_secs: u32) -> Result<()> {
    let dll = shell_hook::hook_dll_path()?;
    file_access::grant_app_packages_read_dll(&dll)?;
    if delay_secs > 0 {
        println!("attaching in {delay_secs} s: open the surface now");
        thread::sleep(Duration::from_secs(u64::from(delay_secs)));
    }
    let found = processes::find_in_session(&[host.exe()])?;
    if found.is_empty() {
        println!(
            "{} is not running; open the surface once and try again",
            host.exe()
        );
        return Ok(());
    }
    for process in found {
        let who = format!("{} (pid {})", process.name, process.pid);
        match shell_hook::attach_sandboxed(process.pid, &dll, &[], CONFIRM_TIMEOUT) {
            Ok(attached) => report(&who, &attached),
            Err(error) => println!("{who}: {error:#}"),
        }
    }
    println!("{LOG_HINT}");
    Ok(())
}

fn report(who: &str, attached: &SandboxAttached) {
    if attached.replaced > 0 {
        println!(
            "{who}: asked {} control window(s) of an earlier copy to detach first",
            attached.replaced
        );
    }
    for (connection, hr) in &attached.answered {
        println!(
            "{who}: VisualDiagConnection{connection} returned {:#010x}",
            hr.0
        );
    }
    if attached.confirmed {
        println!("{who}: attached; now send a colour with `sepiida shell color <rrggbb>`");
    } else {
        println!(
            "{who}: no answer from the DLL yet. If the surface was closed, open it once, \
             then send a colour; if `color` still finds nothing, the DLL did not load"
        );
    }
}

/// Sends `command` to every attached copy of the DLL: Explorer's and the
/// sandboxed hosts'. `Detach` reaches copies of any version.
pub fn broadcast(command: Command) -> Result<()> {
    let mut targets = Vec::new();
    if let Ok(taskbar) = shell_hook::taskbar() {
        targets.push((Host::Explorer.exe().to_owned(), taskbar.pid));
    }
    for process in processes::find_in_session(&SANDBOXED_HOSTS)? {
        targets.push((process.name, process.pid));
    }
    let mut reached = 0;
    for (name, pid) in targets {
        let sent = if command == Command::Detach {
            shell_hook::detach_all(pid)
        } else {
            shell_hook::send(pid, command)
        };
        match sent {
            Ok(0) | Err(ShellHookError::NotAttached) => {}
            Ok(windows) => {
                println!("{name} (pid {pid}): sent to {windows} XAML thread(s)");
                reached += windows;
            }
            Err(error) => println!("{name} (pid {pid}): {error}"),
        }
    }
    if reached == 0 {
        bail!(
            "the styling DLL is not attached anywhere; run `sepiida shell attach` (taskbar) \
             or `sepiida shell attach <surface>` first"
        );
    }
    println!("see %LOCALAPPDATA%\\sepiida\\logs\\hook.log (taskbar) for the result; {LOG_HINT}");
    Ok(())
}
