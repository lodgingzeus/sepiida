//! The platform-independent side of shell styling: what sepiida.exe and
//! the hook DLL agree on, and how the DLL reads the XAML visual tree.

pub mod attach_gate;
pub mod attach_request;
pub mod backdrop;
pub mod carried;
pub mod colors;
pub mod crash_guard;
pub mod finder;
pub mod folder_attach;
pub mod framework;
pub mod gdi_glass;
pub mod hosts;
pub mod layout;
pub mod markup;
pub mod measure;
pub mod protocol;
pub mod rules;
pub mod tint;
pub mod tree;
