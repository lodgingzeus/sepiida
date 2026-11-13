//! `sepiida shell …`: the experimental taskbar styling from the command
//! line (M3 spike).

use anyhow::Result;
use sepiida_core::shell::protocol::Command;

use crate::{
    cli::ShellCommand,
    win::shell_hook::{self, Attached, Method, ModuleCheck},
};

const LOG_HINT: &str = "%LOCALAPPDATA%\\sepiida\\logs\\hook.log";

pub fn run_cli(command: ShellCommand) -> Result<()> {
    let taskbar = shell_hook::taskbar()?;
    match command {
        ShellCommand::Attach { from, remote } => {
            let dll = shell_hook::hook_dll_path()?;
            let method = if remote {
                Method::Remote
            } else {
                Method::InProcess
            };
            println!(
                "attaching {} to Explorer (pid {}), {method:?}, connections from {from}",
                dll.display(),
                taskbar.pid
            );
            report(&shell_hook::attach(&taskbar, &dll, from, method)?);
        }
        ShellCommand::Color(color) => send(taskbar.pid, Command::SetColor(color))?,
        ShellCommand::Restore => send(taskbar.pid, Command::Restore)?,
        ShellCommand::Detach => send(taskbar.pid, Command::Detach)?,
    }
    Ok(())
}

fn report(attached: &Attached) {
    if let Some((connection, hr)) = attached.remote_result {
        println!(
            "InitializeXamlDiagnosticsEx: VisualDiagConnection{connection} returned {:#010x}",
            hr.0
        );
    }
    if let Some(check) = &attached.loaded_while_hooked {
        print_loaded("while the hook was installed", check);
    }
    print_loaded("now", &attached.loaded);
    if attached.ready {
        println!(
            "attached: the DLL is watching a XAML thread's visual tree. {LOG_HINT} says \
             whether it found the taskbar; if not, detach and try `--from <next number>`"
        );
    } else {
        println!("the DLL did not report back; see {LOG_HINT}");
    }
}

fn print_loaded(when: &str, check: &ModuleCheck) {
    match check {
        Ok(Some(path)) => println!("DLL loaded in Explorer {when}: yes ({})", path.display()),
        Ok(None) => println!("DLL loaded in Explorer {when}: no"),
        Err(error) => println!("DLL loaded in Explorer {when}: could not check ({error})"),
    }
}

fn send(pid: u32, command: Command) -> Result<()> {
    let windows = shell_hook::send(pid, command)?;
    println!("sent to {windows} XAML thread(s) in Explorer; see {LOG_HINT} for the result");
    Ok(())
}
