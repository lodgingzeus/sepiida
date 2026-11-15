//! Glass under a held acrylic: when an animation holds a surface's
//! background (quick settings on 26200: Windows' `AcrylicBrush`, at
//! source Animation), our blur cannot be written there. The held brush is
//! Windows' own, which we already tint in place; here we also lower its
//! `TintLuminosityOpacity` (0.96 on that brush, which makes it nearly
//! opaque) while the glass look is on, and put the original back after.

use crate::shell::layout::{Skip, Value, ValueSource};

/// The luminosity we give a held acrylic: low enough for the wallpaper to
/// show through, as the Down Aero `$Card` pill (`.5`) does at a lighter
/// tint.
pub const HELD_LUMINOSITY: &str = "0.25";

/// The type XAML Diagnostics makes `HELD_LUMINOSITY` as.
pub const LUMINOSITY_TYPE: &str = "Windows.Foundation.Double";

/// The brush property we lower.
pub const LUMINOSITY_PROPERTY: &str = "TintLuminosityOpacity";

/// Whether a skipped write of `value` should lower the held brush's
/// luminosity instead: only our glass (a blur), only under an animation.
pub fn thins_held(value: &Value, skip: &Skip) -> bool {
    matches!(value, Value::Blur { .. }) && matches!(skip, Skip::Driven(ValueSource::Animation))
}

/// How to put a brush's luminosity back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuminosityRestore {
    /// It was set on the brush: set this value again.
    Recreate(String),
    /// It came from elsewhere (default, style): clearing ours brings it
    /// back.
    Clear,
}

/// The restore for the luminosity the brush held before (`source`,
/// `value` as XAML formats it). A local value that is not a fraction is
/// not recreated: clearing is the safe way back.
pub fn luminosity_restore(source: ValueSource, value: &str) -> LuminosityRestore {
    let fraction = value
        .trim()
        .parse::<f64>()
        .is_ok_and(|n| n.is_finite() && (0.0..=1.0).contains(&n));
    if source == ValueSource::Local && fraction {
        LuminosityRestore::Recreate(value.trim().to_owned())
    } else {
        LuminosityRestore::Clear
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::rules::Role;

    const BLUR: Value = Value::Blur {
        amount: 30.0,
        role: Role::Surface,
        opacity: 0.23,
    };

    #[test]
    fn only_glass_under_an_animation_thins() {
        assert!(thins_held(&BLUR, &Skip::Driven(ValueSource::Animation)));
        assert!(!thins_held(&BLUR, &Skip::Driven(ValueSource::VisualState)));
        assert!(!thins_held(&BLUR, &Skip::Invalid));
        assert!(!thins_held(
            &Value::Transparent,
            &Skip::Driven(ValueSource::Animation)
        ));
        assert!(!thins_held(
            &Value::Acrylic(Role::Surface),
            &Skip::Driven(ValueSource::Animation)
        ));
    }

    #[test]
    fn a_local_fraction_is_recreated() {
        assert_eq!(
            luminosity_restore(ValueSource::Local, "0.96"),
            LuminosityRestore::Recreate("0.96".into())
        );
        assert_eq!(
            luminosity_restore(ValueSource::Local, " 1 "),
            LuminosityRestore::Recreate("1".into())
        );
    }

    #[test]
    fn anything_else_is_cleared() {
        for (source, value) in [
            (ValueSource::Other, "0.96"),
            (ValueSource::Animation, "0.5"),
            (ValueSource::Local, ""),
            (ValueSource::Local, "NaN"),
            (ValueSource::Local, "1.5"),
            (ValueSource::Local, "-0.1"),
            (ValueSource::Local, "<null>"),
        ] {
            assert_eq!(
                luminosity_restore(source, value),
                LuminosityRestore::Clear,
                "{source:?} {value}"
            );
        }
    }

    #[test]
    fn the_held_luminosity_is_a_fraction() {
        assert_eq!(
            luminosity_restore(ValueSource::Local, HELD_LUMINOSITY),
            LuminosityRestore::Recreate(HELD_LUMINOSITY.into())
        );
    }
}
