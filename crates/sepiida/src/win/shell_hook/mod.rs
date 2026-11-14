//! Gets the styling DLL attached inside Explorer and sends it commands.
//!
//! The XAML Diagnostics call itself lives in the DLL (see its `inject`
//! module), so both ways of attaching use the same code. sepiida.exe loads
//! the DLL into itself and either:
//! - in-process (default, TranslucentTB's current method): installs the
//!   DLL's `WH_CALLWNDPROC` hook on the taskbar's thread only, which maps the
//!   DLL into Explorer, and sends the taskbar window the attach message; the
//!   DLL then attaches from inside Explorer; or
//! - remote: calls the DLL's export with Explorer's pid, so the XAML
//!   Diagnostics request comes from sepiida.exe.
//!
//! Either way the DLL sets a named event once it watches the visual tree.
//!
//! Only one copy of the DLL may style a process: attaching first asks every
//! copy already there (any version, any folder) to restore and detach.

mod attach;
mod control;
mod error;
mod library;
mod signals;

pub use attach::{
    Attached, Method, ModuleCheck, Taskbar, attach, attach_dump, hook_dll_path, taskbar,
};
pub use control::{detach_all, is_attached, send, wait_until_attached};
pub use error::ShellHookError;
pub use signals::Outcome;
