//! The windowless build of sepiida: no console window appears when it is
//! started at sign-in or from the Start menu.

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    sepiida::gui_main();
}
