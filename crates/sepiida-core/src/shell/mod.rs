//! The platform-independent side of shell styling: what sepiida.exe and
//! the hook DLL agree on, and how the DLL reads the XAML visual tree.

pub mod attach_gate;
pub mod colors;
pub mod crash_guard;
pub mod finder;
pub mod hosts;
pub mod protocol;
pub mod rules;
pub mod tree;
