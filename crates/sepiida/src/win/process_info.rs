//! Facts about another process: which CPU architecture it runs as.
//!
//! A DLL can only be loaded into a process of its own architecture. On
//! Windows on ARM, Explorer runs as native ARM64 while an x64 sepiida.exe
//! runs under emulation, so an x64 hook DLL can never reach Explorer.

use windows::Win32::{
    Foundation::CloseHandle,
    System::{
        SystemInformation::{
            IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
        },
        Threading::{
            GetProcessInformation, OpenProcess, PROCESS_MACHINE_INFORMATION,
            PROCESS_QUERY_LIMITED_INFORMATION, ProcessMachineTypeInfo,
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
    Other(u16),
}

impl Arch {
    fn from_machine(machine: IMAGE_FILE_MACHINE) -> Self {
        match machine {
            IMAGE_FILE_MACHINE_AMD64 => Self::X64,
            IMAGE_FILE_MACHINE_ARM64 => Self::Arm64,
            other => Self::Other(other.0),
        }
    }

    /// The architecture this program (and the hook DLL built with it) was
    /// compiled for.
    pub const fn this_build() -> Self {
        if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else {
            Self::X64
        }
    }

    /// The suffix of the matching CI artifact (`sepiida-windows-<suffix>`).
    pub fn artifact_suffix(self) -> String {
        match self {
            Self::X64 => "x64".into(),
            Self::Arm64 => "arm64".into(),
            Self::Other(machine) => format!("machine {machine:#06x}"),
        }
    }
}

/// The architecture process `pid` runs as, or `None` if Windows cannot say
/// (`ProcessMachineTypeInfo` needs Windows 11; on Windows 10 everything that
/// matters here is x64 anyway).
pub fn process_arch(pid: u32) -> Option<Arch> {
    // SAFETY: the handle is closed below; the output struct has the size
    // passed.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut info = PROCESS_MACHINE_INFORMATION::default();
        let result = GetProcessInformation(
            process,
            ProcessMachineTypeInfo,
            (&raw mut info).cast(),
            size_of::<PROCESS_MACHINE_INFORMATION>() as u32,
        );
        let _ = CloseHandle(process);
        result
            .ok()
            .map(|()| Arch::from_machine(info.ProcessMachine))
    }
}
