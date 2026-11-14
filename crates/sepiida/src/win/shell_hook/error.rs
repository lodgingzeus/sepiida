//! What can go wrong getting the DLL attached and talking to it.

use std::path::PathBuf;

use windows::core::HRESULT;

use crate::win::process_info::Arch;

#[derive(Debug, thiserror::Error)]
pub enum ShellHookError {
    #[error("the taskbar window was not found; is Explorer running?")]
    NoExplorer,
    #[error("{0} was not found next to sepiida.exe")]
    MissingDll(PathBuf),
    #[error("could not locate sepiida.exe")]
    OwnPath(#[source] std::io::Error),
    #[error("could not load {0}")]
    LoadDll(PathBuf, #[source] windows::core::Error),
    #[error("{0} does not export {1}; is it from the same build as sepiida.exe?")]
    MissingExport(PathBuf, &'static str),
    #[error("could not install the hook on the taskbar thread")]
    Hook(#[source] windows::core::Error),
    #[error("could not send the attach message to the taskbar")]
    Trigger(#[source] windows::core::Error),
    #[error(
        "Explorer runs as {} but this build is {}: a DLL can only be loaded into a process \
         of its own architecture. Use the sepiida-windows-{} build.",
        explorer.artifact_suffix(), ours.artifact_suffix(), explorer.artifact_suffix()
    )]
    WrongArch { explorer: Arch, ours: Arch },
    #[error("no XAML diagnostics port answered in that process (is its XAML up yet?)")]
    NoConnection,
    #[error("XAML Diagnostics refused through VisualDiagConnection{connection}: {hr}")]
    Refused { connection: u32, hr: HRESULT },
    #[error("could not create the ready event")]
    Event(#[source] windows::core::Error),
    #[error("the styling DLL is not attached; run `sepiida shell attach` first")]
    NotAttached,
    #[error("could not send the command")]
    Post(#[source] windows::core::Error),
}
