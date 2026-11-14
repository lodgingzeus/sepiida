//! The shell processes besides Explorer that draw with XAML, and how the
//! DLL is told what to do in them.
//!
//! Start and the notification centre run in sandboxed (AppContainer)
//! processes. Before styling them we need their element names, so the first
//! step is a dump mode: the DLL logs every element it sees and styles
//! nothing. The mode travels as the initialisation string of
//! `InitializeXamlDiagnosticsEx`, which the DLL reads back with
//! `IXamlDiagnostics::GetInitializationData`.

/// Processes that may host the Start menu, the notification centre,
/// quick settings and the calendar. Which one hosts what differs between
/// Windows 11 builds (community sources disagree on the notification
/// centre), so the dump tries every one that is running.
pub const SANDBOXED_HOSTS: [&str; 3] = [
    "StartMenuExperienceHost.exe",
    "ShellExperienceHost.exe",
    "ShellHost.exe",
];

/// A shell process the DLL styles. Each has its own rule set
/// (`rules::rules_for`), chosen by the DLL from its own executable name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Host {
    /// explorer.exe: the taskbar.
    Explorer,
    /// The Start menu.
    StartMenu,
    /// Quick settings and the media controls (Windows 11 24H2).
    ShellHost,
    /// The notification centre, the calendar and toasts.
    ShellExperience,
}

impl Host {
    pub const ALL: [Self; 4] = [
        Self::Explorer,
        Self::StartMenu,
        Self::ShellHost,
        Self::ShellExperience,
    ];

    /// The executable's file name.
    pub const fn exe(self) -> &'static str {
        match self {
            Self::Explorer => "explorer.exe",
            Self::StartMenu => SANDBOXED_HOSTS[0],
            Self::ShellExperience => SANDBOXED_HOSTS[1],
            Self::ShellHost => SANDBOXED_HOSTS[2],
        }
    }

    /// The host an executable file name belongs to (case-insensitive, as
    /// Windows file names are).
    pub fn from_exe(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|host| host.exe().eq_ignore_ascii_case(name))
    }

    /// Whether the host runs sandboxed (AppContainer): our `Local\` events
    /// do not reach it, and the DLL file must be readable by app packages.
    pub const fn is_sandboxed(self) -> bool {
        !matches!(self, Self::Explorer)
    }
}

/// Elements the dump logs per attachment. A Start menu holds a few
/// thousand; the cap bounds the log if a tree keeps changing.
pub const MAX_DUMPED_ELEMENTS: usize = 20_000;

/// The DLL's log file name when it cannot write sepiida's log folder (in a
/// sandboxed process) and writes to that process's temp folder instead.
pub const FALLBACK_LOG_FILE: &str = "sepiida-hook.log";

const DUMP_INIT_DATA: &str = "sepiida:dump";
const ALONGSIDE_INIT_DATA: &str = "sepiida:style-alongside";

/// What the DLL does once attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttachMode {
    /// Watch for styling targets and take commands, replacing any earlier
    /// style attachment in the process.
    #[default]
    Style,
    /// Like `Style`, but beside the attachments already there: for the
    /// further XAML cores of a process (ShellHost keeps quick settings
    /// behind another diagnostics connection than its first).
    StyleAlongside,
    /// Log every element and style nothing.
    Dump,
}

impl AttachMode {
    /// The initialisation string passed to `InitializeXamlDiagnosticsEx`;
    /// `None` passes no data.
    pub fn init_data(self) -> Option<&'static str> {
        match self {
            Self::Style => None,
            Self::StyleAlongside => Some(ALONGSIDE_INIT_DATA),
            Self::Dump => Some(DUMP_INIT_DATA),
        }
    }

    /// The mode an initialisation string asks for. Anything unknown,
    /// including no data, means the default.
    pub fn from_init_data(data: &str) -> Self {
        match data {
            DUMP_INIT_DATA => Self::Dump,
            ALONGSIDE_INIT_DATA => Self::StyleAlongside,
            _ => Self::Style,
        }
    }

    /// The number the DLL's attach export takes for this mode.
    pub fn id(self) -> u32 {
        match self {
            Self::Style => 0,
            Self::Dump => 1,
            Self::StyleAlongside => 2,
        }
    }

    pub fn from_id(id: u32) -> Option<Self> {
        match id {
            0 => Some(Self::Style),
            1 => Some(Self::Dump),
            2 => Some(Self::StyleAlongside),
            _ => None,
        }
    }
}

/// Longest package family name Windows allows (`PACKAGE_FAMILY_NAME_MAX_LENGTH`).
pub const MAX_PACKAGE_FAMILY_NAME: usize = 64;

/// Whether `name` looks like a package family name (`Name_PublisherId`):
/// letters, digits, `.`, `-` and `_` only, so it is safe as one path
/// component of `%LOCALAPPDATA%\Packages\<name>`.
pub fn is_package_family_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_PACKAGE_FAMILY_NAME
        && name.contains('_')
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Whether `name` is one of the sandboxed hosts (case-insensitive, as
/// Windows file names are).
pub fn is_sandboxed_host(name: &str) -> bool {
    SANDBOXED_HOSTS.iter().any(|h| h.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_round_trip_through_init_data_and_ids() {
        for mode in [
            AttachMode::Style,
            AttachMode::StyleAlongside,
            AttachMode::Dump,
        ] {
            assert_eq!(
                AttachMode::from_init_data(mode.init_data().unwrap_or("")),
                mode
            );
            assert_eq!(AttachMode::from_id(mode.id()), Some(mode));
        }
        assert_eq!(AttachMode::from_id(3), None);
    }

    #[test]
    fn unknown_init_data_means_style() {
        assert_eq!(AttachMode::from_init_data(""), AttachMode::Style);
        assert_eq!(
            AttachMode::from_init_data("SEPIIDA:DUMP"),
            AttachMode::Style
        );
        assert_eq!(
            AttachMode::from_init_data("sepiida:dump "),
            AttachMode::Style
        );
    }

    #[test]
    fn package_family_names_are_validated() {
        assert!(is_package_family_name(
            "Microsoft.Windows.ShellExperienceHost_cw5n1h2txyewy"
        ));
        for bad in [
            "",
            "NoPublisher",
            "..\\evil_x",
            "a/b_c",
            "..._x",
            "with space_x",
            &"a".repeat(MAX_PACKAGE_FAMILY_NAME + 1),
        ] {
            assert!(!is_package_family_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn hosts_are_found_by_exe_name_without_case() {
        for host in Host::ALL {
            assert_eq!(Host::from_exe(host.exe()), Some(host));
            assert_eq!(Host::from_exe(&host.exe().to_uppercase()), Some(host));
            assert_eq!(host.is_sandboxed(), is_sandboxed_host(host.exe()));
        }
        assert_eq!(Host::from_exe("ShellHost.exe"), Some(Host::ShellHost));
        assert_eq!(Host::from_exe("explorer"), None);
        assert_eq!(Host::from_exe("sepiida_hook-1234.exe"), None);
        assert_eq!(Host::from_exe(""), None);
    }

    #[test]
    fn every_sandboxed_host_is_a_host() {
        for exe in SANDBOXED_HOSTS {
            assert!(Host::from_exe(exe).is_some_and(Host::is_sandboxed), "{exe}");
        }
    }

    #[test]
    fn hosts_match_without_case() {
        assert!(is_sandboxed_host("startmenuexperiencehost.exe"));
        assert!(is_sandboxed_host("ShellHost.exe"));
        assert!(!is_sandboxed_host("explorer.exe"));
    }
}
