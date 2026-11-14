//! XAML values from theme lines (`Property:=<SolidColorBrush … />`): which
//! ones are safe to load inside a shell process, and the document the DLL
//! hands to `XamlReader.Load`. The text only ever comes from rules built
//! into sepiida, never from a file or the network; even so, anything that
//! could reach outside the process (a URI, a code-behind class) is refused.

/// Longest XAML value accepted.
pub const MAX_XAML: usize = 2048;

const PRESENTATION: &str = "http://schemas.microsoft.com/winfx/2006/xaml/presentation";
const XAML: &str = "http://schemas.microsoft.com/winfx/2006/xaml";

/// Text that would make XAML load or name something outside the process
/// (URIs, UNC and drive paths, code-behind).
const REFUSED: [&str; 7] = [
    "://",
    "file:",
    "ms-appdata:",
    "ms-appx",
    "x:Class",
    "<?",
    "\\",
];

/// Whether `text` is one XAML element we are willing to load: starts with
/// `<`, no URIs or code-behind, no declarations of its own namespaces (we
/// add them), and within `MAX_XAML`.
pub fn is_safe(text: &str) -> bool {
    let text = text.trim();
    text.starts_with('<')
        && text.ends_with('>')
        && text.len() <= MAX_XAML
        && !text.contains("xmlns")
        && !REFUSED.iter().any(|r| text.contains(r))
        && !text.chars().any(|c| c.is_control() && !c.is_whitespace())
}

/// `text` with the XAML namespaces declared on its root element, ready for
/// `XamlReader.Load`; `None` if it is not safe.
pub fn document(text: &str) -> Option<String> {
    if !is_safe(text) {
        return None;
    }
    let text = text.trim();
    // The root element's name ends at the first space, `/` or `>`.
    let name_end = text[1..].find([' ', '\t', '\r', '\n', '/', '>'])? + 1;
    if name_end == 1 {
        return None;
    }
    let namespaces = format!(
        " xmlns=\"{PRESENTATION}\" xmlns:x=\"{XAML}\" xmlns:muxc=\"using:Microsoft.UI.Xaml.Controls\""
    );
    Some(format!(
        "{}{namespaces}{}",
        &text[..name_end],
        &text[name_end..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_namespaces_go_on_the_root_element() {
        assert_eq!(
            document("<SolidColorBrush Color=\"Red\" />").as_deref(),
            Some(
                "<SolidColorBrush xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" \
                 xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\" \
                 xmlns:muxc=\"using:Microsoft.UI.Xaml.Controls\" Color=\"Red\" />"
            )
        );
        assert!(
            document("<AcrylicBrush/>")
                .is_some_and(|d| d.starts_with("<AcrylicBrush xmlns=") && d.ends_with("/>"))
        );
        let nested =
            "<LinearGradientBrush>\n  <GradientStop Color=\"Red\" />\n</LinearGradientBrush>";
        assert!(document(nested).is_some_and(|d| d.ends_with("</LinearGradientBrush>")));
    }

    #[test]
    fn anything_reaching_outside_is_refused() {
        for text in [
            "<ImageBrush ImageSource=\"https://example.com/a.png\" />",
            "<ImageBrush ImageSource=\"file:///C:/a.png\" />",
            "<ImageBrush ImageSource=\"ms-appdata:///local/a.png\" />",
            "<ImageBrush ImageSource=\"ms-appx:///a.png\" />",
            "<Grid x:Class=\"Evil\" />",
            "<ImageBrush ImageSource=\"\\\\host\\share\\a.png\" />",
            "<ImageBrush ImageSource=\"C:\\a.png\" />",
            "<Grid xmlns:e=\"using:Evil\" />",
            "<?xml version=\"1.0\"?><Grid/>",
            "SolidColorBrush",
            "<>",
            "",
        ] {
            assert!(document(text).is_none(), "{text}");
        }
        assert!(document(&format!("<Grid Tag=\"{}\"/>", "a".repeat(MAX_XAML))).is_none());
    }
}
