//! `sepiida shell attach [--from N] [--remote]`: the taskbar styling DLL
//! attached from the command line (M3 spike). Colours and the other
//! commands go through `shell::surfaces`, which reaches every host.

use anyhow::Result;

use crate::win::shell_hook::{self, Attached, Method, ModuleCheck, Outcome};

const LOG_HINT: &str = "%LOCALAPPDATA%\\sepiida\\logs\\hook.log";

pub fn attach(from: u32, remote: bool) -> Result<()> {
    let taskbar = shell_hook::taskbar()?;
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
    Ok(())
}

fn report(attached: &Attached) {
    if attached.replaced > 0 {
        println!(
            "asked {} control window(s) of an earlier copy to detach first",
            attached.replaced
        );
    }
    if let Some(error) = &attached.unhook_error {
        println!("warning: could not remove the hook: {error}");
    }
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
    match attached.outcome {
        Outcome::Ready => println!(
            "attached: the DLL is watching a XAML thread's visual tree. {LOG_HINT} says \
             whether it found the taskbar; if not, detach and try `--from <next number>`"
        ),
        Outcome::Failed => println!(
            "the DLL found no XAML diagnostics port in Explorer (its XAML may not be up \
             yet; try again in a few seconds); see {LOG_HINT}"
        ),
        Outcome::Exited => println!("Explorer exited during the attach"),
        Outcome::TimedOut => println!("the DLL did not report back; see {LOG_HINT}"),
    }
}

fn print_loaded(when: &str, check: &ModuleCheck) {
    match check {
        Ok(Some(path)) => println!("DLL loaded in Explorer {when}: yes ({})", path.display()),
        Ok(None) => println!("DLL loaded in Explorer {when}: no"),
        Err(error) => println!("DLL loaded in Explorer {when}: could not check ({error})"),
    }
}
