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

/// Whether `property` names an attached property: `Owner.Property`, both
/// plain identifiers (`Grid.Row`, `Canvas.ZIndex`).
pub fn is_attached(property: &str) -> bool {
    property
        .split_once('.')
        .is_some_and(|(owner, name)| is_identifier(owner) && is_identifier(name))
}

/// A plain XAML identifier: a letter, then letters and digits.
fn is_identifier(part: &str) -> bool {
    part.len() <= 64
        && part.starts_with(|c: char| c.is_ascii_alphabetic())
        && part.chars().all(|c| c.is_ascii_alphanumeric())
}

/// A `Style` holding one setter of the attached `property` to the plain
/// `value`, for `XamlReader.Load`: XAML resolves the property and parses
/// the value to its type (the Windhawk stylers do the same). `None` unless
/// `property` is attached and `value` is a plain value (no markup,
/// markup extensions or quotes).
pub fn setter_document(property: &str, value: &str) -> Option<String> {
    if !is_attached(property) {
        return None;
    }
    style_document("FrameworkElement", property, value)
}

/// The setter document for a selector's `[property=value]` on an element
/// of `type_name` (as XAML Diagnostics reports it): an attached property
/// as in `setter_document`; a plain one with the element's own type as the
/// style's target, which XAML needs to resolve it, so only for the
/// controls and shapes of system XAML. `None` otherwise.
pub fn condition_document(type_name: &str, property: &str, value: &str) -> Option<String> {
    if is_attached(property) {
        return setter_document(property, value);
    }
    let short = ["Windows.UI.Xaml.Controls.", "Windows.UI.Xaml.Shapes."]
        .iter()
        .find_map(|prefix| type_name.strip_prefix(prefix))
        .filter(|short| is_identifier(short))?;
    if !is_identifier(property) {
        return None;
    }
    style_document(short, property, value)
}

/// The sizes of `FrameworkElement` that Windows' own code may set again
/// after a look has (Start sizes its pinned list from code): the DLL
/// watches these and writes ours back.
pub const SIZE_PROPERTIES: [&str; 6] = [
    "Width",
    "Height",
    "MinWidth",
    "MinHeight",
    "MaxWidth",
    "MaxHeight",
];

/// A one-setter `Style` that names a size property of
/// `FrameworkElement`, for the DLL to get its `DependencyProperty`.
/// `None` for any other property.
pub fn size_document(property: &str) -> Option<String> {
    SIZE_PROPERTIES
        .contains(&property)
        .then(|| style_document("FrameworkElement", property, "0"))?
}

fn style_document(target_type: &str, property: &str, value: &str) -> Option<String> {
    let plain = !value.is_empty()
        && value.len() <= 64
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>' | '{' | '}' | '"' | '&' | '\\'));
    plain.then(|| {
        format!(
            "<Style xmlns=\"{PRESENTATION}\" TargetType=\"{target_type}\">\
             <Setter Property=\"{property}\" Value=\"{value}\" /></Style>"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_owner_dot_property_is_attached() {
        assert!(is_attached("Grid.Row"));
        assert!(is_attached("ScrollViewer.VerticalScrollMode"));
        for name in [
            "Row",
            "Grid.",
            ".Row",
            "Grid.Row.X",
            "Grid.Ro w",
            "1Grid.Row",
            "Grid.Row\"",
        ] {
            assert!(!is_attached(name), "{name}");
        }
    }

    #[test]
    fn a_setter_document_holds_one_plain_value() {
        assert_eq!(
            setter_document("Canvas.ZIndex", "-99").as_deref(),
            Some(
                "<Style xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" \
                 TargetType=\"FrameworkElement\"><Setter Property=\"Canvas.ZIndex\" Value=\"-99\" /></Style>"
            )
        );
        assert!(setter_document("Grid.Row", "0").is_some());
        for value in ["", "{Binding X}", "a\"b", "<Grid/>", "a&b", "a\nb"] {
            assert_eq!(setter_document("Grid.Row", value), None, "{value:?}");
        }
        assert_eq!(setter_document("Width", "4"), None);
    }

    #[test]
    fn a_size_document_names_only_size_properties() {
        assert_eq!(
            size_document("Height").as_deref(),
            Some(
                "<Style xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" \
                 TargetType=\"FrameworkElement\"><Setter Property=\"Height\" Value=\"0\" /></Style>"
            )
        );
        for property in SIZE_PROPERTIES {
            assert!(size_document(property).is_some(), "{property}");
        }
        for property in ["Margin", "Visibility", "Grid.Row", "height", ""] {
            assert_eq!(size_document(property), None, "{property}");
        }
    }

    #[test]
    fn a_condition_document_targets_the_elements_type_for_a_plain_property() {
        assert_eq!(
            condition_document("Windows.UI.Xaml.Controls.TextBlock", "Text", "\u{E971}").as_deref(),
            Some(
                "<Style xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" \
                 TargetType=\"TextBlock\"><Setter Property=\"Text\" Value=\"\u{E971}\" /></Style>"
            )
        );
        assert_eq!(
            condition_document(
                "Taskbar.ExperienceToggleButton",
                "AutomationProperties.AutomationId",
                "StartButton"
            ),
            setter_document("AutomationProperties.AutomationId", "StartButton")
        );
        // A plain property of a type XAML cannot name without a prefix.
        assert_eq!(
            condition_document("Taskbar.TaskListButton", "Text", "a"),
            None
        );
        assert_eq!(
            condition_document("Windows.UI.Xaml.Controls.Primitives.Popup", "Tag", "a"),
            None
        );
        assert_eq!(
            condition_document("Windows.UI.Xaml.Controls.TextBlock", "Te xt", "a"),
            None
        );
        assert_eq!(
            condition_document("Windows.UI.Xaml.Controls.TextBlock", "Text", "a\"b"),
            None
        );
    }

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
