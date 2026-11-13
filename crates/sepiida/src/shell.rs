//! `sepiida shell …`: the experimental taskbar styling from the command
//! line (M3 spike).

use anyhow::Result;
use sepiida_core::hook_protocol::Command;

use crate::{cli::ShellCommand, win::shell_hook};

pub fn run_cli(command: ShellCommand) -> Result<()> {
    let pid = shell_hook::explorer_pid()?;
    match command {
        ShellCommand::Attach => {
            let dll = shell_hook::hook_dll_path()?;
            println!("attaching {} to Explorer (pid {pid})", dll.display());
            let attached = shell_hook::attach(pid, &dll)?;
            if attached.ready {
                println!(
                    "attached through {}; the DLL is watching the taskbar",
                    attached.connection
                );
            } else {
                println!(
                    "XAML accepted the request through {}, but the DLL did not report back; \
                     see %LOCALAPPDATA%\\sepiida\\logs\\hook.log",
                    attached.connection
                );
            }
        }
        ShellCommand::Color(color) => send(pid, Command::SetColor(color))?,
        ShellCommand::Restore => send(pid, Command::Restore)?,
        ShellCommand::Detach => send(pid, Command::Detach)?,
    }
    Ok(())
}

fn send(pid: u32, command: Command) -> Result<()> {
    let windows = shell_hook::send(pid, command)?;
    println!("sent to {windows} XAML thread(s) in Explorer; see hook.log for the result");
    Ok(())
}
