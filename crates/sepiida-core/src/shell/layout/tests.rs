use super::*;

const THICKNESS: &str = "Windows.UI.Xaml.Thickness";
const DOUBLE: &str = "Windows.Foundation.Double";

fn current<'a>(type_name: &'a str, source: ValueSource, value: &'a str) -> Current<'a> {
    Current {
        type_name,
        source,
        value,
    }
}

#[test]
fn values_are_formatted_the_way_xaml_parses_them() {
    assert_eq!(
        Value::Thickness([8.0, 4.0, 8.0, 4.0]).candidates(),
        ["8,4,8,4"]
    );
    assert_eq!(
        Value::CornerRadius([8.5, 0.0, -0.0, 12.25]).candidates(),
        ["8.5,0,0,12.25"]
    );
    assert_eq!(Value::Length(0.5).candidates(), ["0.5"]);
    assert_eq!(Value::Length(16.0).candidates(), ["16"]);
    assert_eq!(
        Value::Visibility(Visibility::Collapsed).candidates(),
        ["Collapsed", "1"]
    );
    assert_eq!(
        Value::Visibility(Visibility::Visible).candidates(),
        ["Visible", "0"]
    );
    assert_eq!(
        Value::Thickness([1.0, 2.0, 3.0, 4.0]).to_string(),
        "1,2,3,4"
    );
    assert_eq!(
        Value::Visibility(Visibility::Collapsed).to_string(),
        "Collapsed"
    );
}

#[test]
fn out_of_range_values_are_invalid() {
    assert!(Value::Thickness([-8.0, 0.0, 0.0, 0.0]).is_valid());
    assert!(!Value::Thickness([f32::NAN, 0.0, 0.0, 0.0]).is_valid());
    assert!(!Value::Thickness([0.0, f32::INFINITY, 0.0, 0.0]).is_valid());
    assert!(!Value::Thickness([0.0, 0.0, 5000.0, 0.0]).is_valid());
    assert!(!Value::CornerRadius([-1.0, 0.0, 0.0, 0.0]).is_valid());
    assert!(!Value::Length(-1.0).is_valid());
    assert!(Value::Length(MAX_LENGTH).is_valid());
    assert!(!Value::Length(MAX_LENGTH + 1.0).is_valid());
    assert_eq!(
        plan_write(
            &Value::Length(f32::NAN),
            &current(DOUBLE, ValueSource::Other, "0")
        ),
        Err(Skip::Invalid)
    );
}

#[test]
fn a_local_original_is_recreated_from_its_own_string() {
    let write = plan_write(
        &Value::Thickness([8.0, 4.0, 8.0, 4.0]),
        &current(THICKNESS, ValueSource::Local, "0,0,0,0"),
    );
    assert_eq!(
        write,
        Ok(Write {
            make: Make::Parse {
                type_name: THICKNESS.to_owned(),
                candidates: vec!["8,4,8,4".to_owned()],
            },
            restore: Restore::Recreate {
                type_name: THICKNESS.to_owned(),
                value: "0,0,0,0".to_owned(),
            },
        })
    );
}

#[test]
fn a_styled_or_default_original_is_cleared() {
    for source in [
        ValueSource::Other,
        ValueSource::from_id(3),
        ValueSource::from_id(1),
    ] {
        let write = plan_write(&Value::Length(8.0), &current(DOUBLE, source, "0"));
        assert_eq!(write.map(|w| w.restore), Ok(Restore::Clear), "{source:?}");
    }
}

#[test]
fn animated_and_visual_state_properties_are_left_alone() {
    for (id, source) in [(12, ValueSource::Animation), (14, ValueSource::VisualState)] {
        assert_eq!(ValueSource::from_id(id), source);
        assert_eq!(
            plan_write(&Value::Length(8.0), &current(DOUBLE, source, "0")),
            Err(Skip::Driven(source))
        );
    }
}

#[test]
fn a_value_for_another_type_is_refused() {
    assert_eq!(
        plan_write(
            &Value::Thickness([1.0; 4]),
            &current(DOUBLE, ValueSource::Other, "0")
        ),
        Err(Skip::WrongType {
            expected: ".Thickness",
            found: DOUBLE.to_owned(),
        })
    );
    // A suffix of the type's last part is not enough.
    assert!(
        plan_write(
            &Value::Length(1.0),
            &current("Windows.Foundation.MyDouble", ValueSource::Other, "0")
        )
        .is_err()
    );
    assert!(
        plan_write(
            &Value::Visibility(Visibility::Collapsed),
            &current("Windows.UI.Xaml.Visibility", ValueSource::Local, "Visible")
        )
        .is_ok()
    );
}

#[test]
fn source_ids_follow_xamlom() {
    assert_eq!(ValueSource::from_id(4), ValueSource::Local);
    for id in [0, 1, 2, 3, 5, 11, 13, 15, -1] {
        assert_eq!(ValueSource::from_id(id), ValueSource::Other, "{id}");
    }
}

#[test]
fn an_acrylic_brush_replaces_a_styled_brush_and_never_a_local_one() {
    const BRUSH: &str = "Windows.UI.Xaml.Media.Brush";
    let acrylic = Value::Acrylic(Role::Surface);
    assert_eq!(
        plan_write(&acrylic, &current(BRUSH, ValueSource::Other, "")),
        Ok(Write {
            make: Make::Acrylic(Role::Surface),
            restore: Restore::Clear,
        })
    );
    assert_eq!(
        plan_write(&acrylic, &current(BRUSH, ValueSource::Local, "x")),
        Err(Skip::LocalObject)
    );
    assert!(plan_write(&acrylic, &current(THICKNESS, ValueSource::Other, "0")).is_err());
    assert!(acrylic.is_valid());
    assert_eq!(acrylic.to_string(), "acrylic in the Surface colour");
}

#[test]
fn transparent_clears_a_styled_brush_and_holds_a_local_one() {
    const BRUSH: &str = "Windows.UI.Xaml.Media.Brush";
    let clear = Value::Transparent;
    assert_eq!(
        plan_write(&clear, &current(BRUSH, ValueSource::Other, "")),
        Ok(Write {
            make: Make::Transparent,
            restore: Restore::Clear,
        })
    );
    assert_eq!(
        plan_write(&clear, &current(BRUSH, ValueSource::Local, "2186123665552")),
        Ok(Write {
            make: Make::Transparent,
            restore: Restore::Hold,
        })
    );
    assert_eq!(
        plan_write(&clear, &current(BRUSH, ValueSource::VisualState, "")),
        Err(Skip::Driven(ValueSource::VisualState))
    );
    assert!(plan_write(&clear, &current(THICKNESS, ValueSource::Other, "0")).is_err());
    assert!(clear.is_valid());
    assert_eq!(clear.to_string(), "transparent");
}

#[test]
fn a_local_original_without_a_string_is_held() {
    const CORNERS: &str = "Windows.UI.Xaml.CornerRadius";
    let write = plan_write(
        &Value::CornerRadius([8.0; 4]),
        &current(CORNERS, ValueSource::Local, ""),
    );
    assert_eq!(write.map(|w| w.restore), Ok(Restore::Hold));
}

#[test]
fn auto_and_alignment_offer_name_then_number() {
    assert_eq!(Value::Auto.candidates(), ["Auto", "NaN"]);
    assert_eq!(Value::Auto.type_suffix(), ".Double");
    assert_eq!(
        Value::HorizontalAlignment(HorizontalAlignment::Center).candidates(),
        ["Center", "1"]
    );
    assert_eq!(
        Value::HorizontalAlignment(HorizontalAlignment::Stretch).candidates(),
        ["Stretch", "3"]
    );
    assert_eq!(
        Value::HorizontalAlignment(HorizontalAlignment::Left).to_string(),
        "Left"
    );
    assert!(
        plan_write(
            &Value::HorizontalAlignment(HorizontalAlignment::Center),
            &current(
                "Windows.UI.Xaml.HorizontalAlignment",
                ValueSource::Other,
                "3"
            )
        )
        .is_ok()
    );
}

#[test]
fn a_xaml_value_is_loaded_and_a_local_original_held() {
    const BRUSH: &str = "Windows.UI.Xaml.Media.Brush";
    let value = Value::Xaml("<SolidColorBrush Color=\"Red\" />");
    let write = plan_write(&value, &current(BRUSH, ValueSource::Local, "123")).unwrap();
    assert_eq!(write.restore, Restore::Hold);
    assert!(matches!(&write.make, Make::Xaml(d) if d.starts_with("<SolidColorBrush xmlns=")));
    assert_eq!(
        plan_write(&value, &current(BRUSH, ValueSource::Other, "")).map(|w| w.restore),
        Ok(Restore::Clear)
    );
    let unsafe_value = Value::Xaml("<ImageBrush ImageSource=\"https://x/a.png\" />");
    assert_eq!(
        plan_write(&unsafe_value, &current(BRUSH, ValueSource::Other, "")),
        Err(Skip::Invalid)
    );
}

#[test]
fn text_is_made_by_xaml_and_short_forms_are_spelled_out() {
    const CORNERS: &str = "Windows.UI.Xaml.CornerRadius";
    const BRUSH: &str = "Windows.UI.Xaml.Media.Brush";
    let parse = |value: Value, type_name: &str, source, original: &str| {
        plan_write(&value, &current(type_name, source, original))
    };
    assert_eq!(
        parse(Value::Text("14"), CORNERS, ValueSource::Other, ""),
        Ok(Write {
            make: Make::Parse {
                type_name: CORNERS.to_owned(),
                candidates: vec!["14,14,14,14".to_owned(), "14".to_owned()],
            },
            restore: Restore::Clear,
        })
    );
    assert_eq!(
        parse(Value::Text("4,8"), THICKNESS, ValueSource::Local, "1,2,3,4"),
        Ok(Write {
            make: Make::Parse {
                type_name: THICKNESS.to_owned(),
                candidates: vec!["4,8,4,8".to_owned(), "4,8".to_owned()],
            },
            restore: Restore::Recreate {
                type_name: THICKNESS.to_owned(),
                value: "1,2,3,4".to_owned(),
            },
        })
    );
    // A local brush's string is a handle, not the brush: held instead.
    assert_eq!(
        parse(
            Value::Text("Red"),
            BRUSH,
            ValueSource::Local,
            "2186123665552"
        )
        .map(|w| w.restore),
        Ok(Restore::Hold)
    );
    for bad in ["", "<Grid/>", "{ThemeResource X}", "a\nb"] {
        assert!(!Value::Text(bad).is_valid(), "{bad}");
    }
    let long: &'static str = Box::leak("9".repeat(MAX_TEXT + 1).into_boxed_str());
    assert!(!Value::Text(long).is_valid());
    assert_eq!(Value::Text("Center").to_string(), "Center");
}
