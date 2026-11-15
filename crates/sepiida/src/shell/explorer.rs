//! `sepiida shell attach explorer`: the styling DLL attached to File
//! Explorer's WinUI 3 XAML from the command line (M6). Colours and the
//! other commands go through `shell::surfaces`, which reaches it too.

use anyhow::Result;
use sepiida_core::shell::framework::Framework;

use crate::win::shell_hook::{self, Outcome};

const LOG_HINT: &str = "%LOCALAPPDATA%\\sepiida\\logs\\hook.log";

pub fn attach() -> Result<()> {
    let dll = shell_hook::hook_dll_path()?;
    let window = shell_hook::explorer_window()?;
    // A second WinUI attach in one explorer.exe has ended it.
    if shell_hook::is_attached(window.pid, Framework::WinUi) {
        println!(
            "File Explorer (pid {}) is already attached; send commands with `sepiida shell …`",
            window.pid
        );
        return Ok(());
    }
    match shell_hook::taskbar() {
        Ok(taskbar) => println!(
            "taskbar: explorer.exe pid {}; folder window: explorer.exe pid {}",
            taskbar.pid, window.pid
        ),
        Err(error) => println!("taskbar: {error}; folder window: pid {}", window.pid),
    }
    let (replaced, outcome) =
        shell_hook::attach_winui(&window, &dll, sepiida_core::shell::hosts::AttachMode::Style)?;
    if replaced > 0 {
        println!("asked {replaced} control window(s) of an earlier Explorer copy to detach first");
    }
    match outcome {
        Outcome::Ready => println!(
            "attached to File Explorer's XAML; now send a colour with `sepiida shell color \
             <rrggbb>` and turn the look on with `sepiida shell look on`. See {LOG_HINT}"
        ),
        Outcome::Failed => println!("no WinUI 3 diagnostics port answered; see {LOG_HINT}"),
        Outcome::Exited => println!("Explorer exited during the attach"),
        Outcome::TimedOut => println!("the DLL did not report back; see {LOG_HINT}"),
    }
    Ok(())
}
