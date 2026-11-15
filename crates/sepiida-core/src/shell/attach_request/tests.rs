use super::*;

const BLUE: Color = Color::new(0x33, 0x66, 0x99);
const BLACK: Color = Color::new(0, 0, 0);

#[test]
fn requests_round_trip_through_init_data_and_the_export() {
    for request in [
        AttachRequest::new(AttachMode::Style, &[]),
        AttachRequest::new(AttachMode::Style, &[(Role::Surface, BLUE)]),
        AttachRequest::new(
            AttachMode::StyleAlongside,
            &[(Role::Accent, BLACK), (Role::Surface, BLUE)],
        ),
        AttachRequest::new(AttachMode::StyleAlongside, &[]),
        AttachRequest::new(AttachMode::Dump, &[]),
        AttachRequest::new(
            AttachMode::Style,
            &[
                (Role::SurfaceRaised, BLUE),
                (Role::Accent, BLACK),
                (Role::Surface, BLUE),
            ],
        ),
    ] {
        let data = request.init_data().unwrap_or_default();
        assert_eq!(AttachRequest::from_init_data(&data), request, "{data}");
        assert_eq!(
            AttachRequest::from_export(request.mode, request.export_colors()),
            Some(request.clone())
        );
    }
}

#[test]
fn the_string_is_tag_then_role_colours() {
    let request = AttachRequest::new(
        AttachMode::Style,
        &[(Role::Accent, BLACK), (Role::Surface, BLUE)],
    );
    assert_eq!(
        request.init_data().as_deref(),
        Some("sepiida:style;1=336699;2=000000")
    );
    assert_eq!(AttachRequest::new(AttachMode::Style, &[]).init_data(), None);
    let every_role: Vec<(Role, Color)> = Role::ALL.iter().map(|&r| (r, BLUE)).collect();
    assert!(
        AttachRequest::new(AttachMode::StyleAlongside, &every_role)
            .init_data()
            .is_some_and(|d| d.len() <= MAX_INIT_DATA)
    );
}

#[test]
fn dump_mode_carries_no_colours_and_the_last_colour_wins() {
    let dump = AttachRequest::new(AttachMode::Dump, &[(Role::Surface, BLUE)]);
    assert!(dump.colors().is_empty());
    assert_eq!(dump.init_data().as_deref(), Some("sepiida:dump"));
    let twice = AttachRequest::new(
        AttachMode::Style,
        &[(Role::Surface, BLACK), (Role::Surface, BLUE)],
    );
    assert_eq!(twice.colors(), &[(Role::Surface, BLUE)]);
}

#[test]
fn malformed_colours_are_all_dropped() {
    for data in [
        "sepiida:style;1=33669",
        "sepiida:style;1=3366999",
        "sepiida:style;1=33669g",
        "sepiida:style;4=336699",
        "sepiida:style;0=336699",
        "sepiida:style;1=336699;1=000000",
        "sepiida:style;1=336699;",
        "sepiida:style;11=336699",
        "sepiida:style;1=+36699",
        "sepiida:style;1=336699;2",
    ] {
        let request = AttachRequest::from_init_data(data);
        assert_eq!(request.mode, AttachMode::Style, "{data}");
        assert!(request.colors().is_empty(), "{data}");
    }
    let alongside = AttachRequest::from_init_data("sepiida:style-alongside;x");
    assert_eq!(alongside.mode, AttachMode::StyleAlongside);
    assert!(alongside.colors().is_empty());
}

#[test]
fn the_winui_suffix_round_trips_for_every_mode() {
    let dump = AttachRequest::new(AttachMode::Dump, &[]).for_framework(Framework::WinUi);
    assert_eq!(dump.init_data().as_deref(), Some("sepiida:dump@winui"));
    // Style without colours still says which framework it is for.
    let style = AttachRequest::default().for_framework(Framework::WinUi);
    assert_eq!(style.init_data().as_deref(), Some("sepiida:style@winui"));
    let colored = AttachRequest::new(AttachMode::Style, &[(Role::Surface, BLUE)])
        .for_framework(Framework::WinUi);
    assert_eq!(
        colored.init_data().as_deref(),
        Some("sepiida:style@winui;1=336699")
    );
    for request in [dump, style, colored] {
        let data = request.init_data().unwrap_or_default();
        assert_eq!(AttachRequest::from_init_data(&data), request);
    }
    assert_eq!(
        AttachRequest::from_init_data("sepiida:style-alongside@winui").framework,
        Framework::WinUi
    );
}

#[test]
fn system_requests_and_unknown_suffixes_are_system() {
    assert_eq!(AttachRequest::default().framework, Framework::System);
    assert_eq!(AttachRequest::default().init_data(), None);
    for data in ["sepiida:dump", "sepiida:style;1=336699"] {
        assert_eq!(
            AttachRequest::from_init_data(data).framework,
            Framework::System
        );
    }
    for data in [
        "sepiida:dump@wpf",
        "sepiida:style@WINUI",
        "sepiida:dump@winui@winui",
    ] {
        let request = AttachRequest::from_init_data(data);
        assert_eq!(request, AttachRequest::default(), "{data}");
    }
}

#[test]
fn unknown_tags_and_long_strings_mean_plain_style() {
    for data in [
        "other;1=336699",
        "SEPIIDA:STYLE;1=336699",
        "sepiida:dump;1=336699",
        &format!("sepiida:style;1=336699{}", " ".repeat(MAX_INIT_DATA)),
    ] {
        let request = AttachRequest::from_init_data(data);
        assert!(request.colors().is_empty(), "{data}");
    }
    assert_eq!(
        AttachRequest::from_init_data("sepiida:dump;1=336699").mode,
        AttachMode::Dump
    );
}

#[test]
fn export_values_need_the_colour_marker() {
    assert_eq!(
        AttachRequest::from_export(AttachMode::Style, [0x0133_6699, 0, 0])
            .map(|r| r.colors().to_vec()),
        Some(vec![(Role::Surface, BLUE)])
    );
    // Black is a colour, not "none".
    assert_eq!(
        AttachRequest::from_export(AttachMode::Style, [0x0100_0000, 0, 0])
            .map(|r| r.colors().to_vec()),
        Some(vec![(Role::Surface, BLACK)])
    );
    assert_eq!(
        AttachRequest::from_export(AttachMode::Style, [0x0233_6699, 0, 0]),
        None
    );
    assert_eq!(
        AttachRequest::from_export(AttachMode::Style, [0x0033_6699, 0, 0]),
        None
    );
}
