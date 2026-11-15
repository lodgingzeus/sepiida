//! The two XAML frameworks in the shell, and how each one's diagnostics
//! service is reached.
//!
//! The taskbar, Start and the flyouts use system XAML (`Windows.UI.Xaml`).
//! File Explorer's tab bar, address bar, command bar, Home, Gallery and
//! details pane use WinUI 3 (`Microsoft.UI.Xaml`), whose
//! `InitializeXamlDiagnosticsEx` is exported by
//! `Microsoft.Internal.FrameworkUdk.dll` and whose ports are named
//! `WinUIVisualDiagConnection<N>` (as in Windhawk's File Explorer Styler,
//! GPL-3.0). explorer.exe runs both, one diagnostics connection each.

/// A XAML framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Framework {
    /// `Windows.UI.Xaml`, part of Windows.
    #[default]
    System,
    /// `Microsoft.UI.Xaml` (Windows App SDK), used by File Explorer.
    WinUi,
}

/// Connection numbers tried for system XAML. Explorer creates few XAML
/// threads; the taskbar's is among the first.
pub const MAX_CONNECTION: u32 = 256;

/// Connection numbers tried for WinUI 3. Windhawk tries up to 10 000 and
/// attaches once per process, expecting one port to answer; the number is
/// not known in advance.
pub const MAX_WINUI_CONNECTION: u32 = 4096;

impl Framework {
    /// The DLL exporting this framework's `InitializeXamlDiagnosticsEx`.
    pub const fn diagnostics_module(self) -> &'static str {
        match self {
            Self::System => "Windows.UI.Xaml.dll",
            Self::WinUi => "Microsoft.Internal.FrameworkUdk.dll",
        }
    }

    /// The name of every diagnostics port, without its number.
    pub const fn connection_prefix(self) -> &'static str {
        match self {
            Self::System => "VisualDiagConnection",
            Self::WinUi => "WinUIVisualDiagConnection",
        }
    }

    /// The highest connection number worth trying.
    pub const fn max_connection(self) -> u32 {
        match self {
            Self::System => MAX_CONNECTION,
            Self::WinUi => MAX_WINUI_CONNECTION,
        }
    }

    /// The namespace its element and value types live in (`Grid` is
    /// `<namespace>.Controls.Grid`).
    pub const fn namespace(self) -> &'static str {
        match self {
            Self::System => "Windows.UI.Xaml",
            Self::WinUi => "Microsoft.UI.Xaml",
        }
    }

    /// Every connection name to try from number `from` on, in order. A
    /// `from` of 0 counts as 1.
    pub fn connection_names(self, from: u32) -> impl Iterator<Item = (u32, String)> {
        let prefix = self.connection_prefix();
        (from.max(1)..=self.max_connection()).map(move |n| (n, format!("{prefix}{n}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_framework_names_its_own_ports() {
        let system: Vec<_> = Framework::System.connection_names(0).take(2).collect();
        assert_eq!(
            system,
            [
                (1, "VisualDiagConnection1".to_owned()),
                (2, "VisualDiagConnection2".to_owned())
            ]
        );
        let winui = Framework::WinUi.connection_names(7).next();
        assert_eq!(winui, Some((7, "WinUIVisualDiagConnection7".to_owned())));
    }

    #[test]
    fn names_stop_at_the_framework_limit() {
        assert_eq!(Framework::System.connection_names(1).count(), 256);
        assert_eq!(Framework::WinUi.connection_names(4096).count(), 1);
        assert_eq!(Framework::WinUi.connection_names(4097).count(), 0);
    }

    #[test]
    fn modules_and_namespaces() {
        assert_eq!(
            Framework::WinUi.diagnostics_module(),
            "Microsoft.Internal.FrameworkUdk.dll"
        );
        assert_eq!(Framework::System.namespace(), "Windows.UI.Xaml");
        assert_eq!(Framework::WinUi.namespace(), "Microsoft.UI.Xaml");
    }
}
