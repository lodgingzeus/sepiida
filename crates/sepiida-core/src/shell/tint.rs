//! Tinting Windows' own acrylic brushes, the data side: what tint to set
//! on a brush, and which tint is the original to put back.
//!
//! The DLL keeps the shell's `AcrylicBrush` and changes its `TintColor`,
//! its `FallbackColor` (drawn instead of acrylic when acrylic cannot be
//! rendered: transparency effects off, battery saver, a weak GPU) and,
//! when asked, its `TintOpacity`. The brush belongs to Windows, which
//! may change its tint by itself (a theme change): whatever the brush holds
//! that is not what we set last is Windows' value, and becomes the original.

use std::fmt;

use crate::Color;

/// A colour with alpha, as `Windows.UI.Color` holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argb {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Argb {
    pub const fn new(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self { a, r, g, b }
    }
}

impl fmt::Display for Argb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "#{:02X}{:02X}{:02X}{:02X}",
            self.a, self.r, self.g, self.b
        )
    }
}

/// A tint opacity in percent, 0 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opacity(u8);

impl Opacity {
    pub const MAX: u8 = 100;

    pub fn from_percent(percent: u8) -> Option<Self> {
        (percent <= Self::MAX).then_some(Self(percent))
    }

    pub const fn percent(self) -> u8 {
        self.0
    }

    /// The value `TintOpacity` takes, 0.0 to 1.0.
    pub fn fraction(self) -> f64 {
        f64::from(self.0) / f64::from(Self::MAX)
    }
}

/// A brush's tint: its colour and opacity, and its fallback colour if it
/// could be read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    pub color: Argb,
    pub opacity: f64,
    pub fallback: Option<Argb>,
}

impl Tint {
    /// Equal up to the rounding a `double` round trip through XAML might
    /// add.
    fn same(self, other: Self) -> bool {
        self.color == other.color
            && self.fallback == other.fallback
            && (self.opacity - other.opacity).abs() < 1e-6
    }
}

/// One tinted brush: Windows' original tint and the tint we set last.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushTint {
    original: Tint,
    applied: Option<Tint>,
}

impl BrushTint {
    /// A brush seen for the first time, holding `current`.
    pub fn new(current: Tint) -> Self {
        Self {
            original: current,
            applied: None,
        }
    }

    pub fn original(&self) -> Tint {
        self.original
    }

    /// Records what the brush holds now. Returns whether Windows changed
    /// it since we last set it; if so, that is the new original.
    pub fn observe(&mut self, current: Tint) -> bool {
        match self.applied {
            Some(applied) if !applied.same(current) => {
                self.original = current;
                self.applied = None;
                true
            }
            Some(_) => false,
            None => {
                self.original = current;
                false
            }
        }
    }

    /// The tint for `color`: tint and fallback in our colour with the
    /// originals' alpha (opaque if that was zero, which would hide it), at
    /// `opacity` if given, else at the original opacity.
    pub fn target(&self, color: Color, opacity: Option<Opacity>) -> Tint {
        Tint {
            color: recolor(self.original.color, color),
            opacity: opacity.map_or(self.original.opacity, Opacity::fraction),
            fallback: self.original.fallback.map(|f| recolor(f, color)),
        }
    }

    /// Records that `tint` is now on the brush.
    pub fn applied(&mut self, tint: Tint) {
        self.applied = Some(tint);
    }

    /// Records that the original is back on the brush.
    pub fn restored(&mut self) {
        self.applied = None;
    }
}

/// `color` with `original`'s alpha, or opaque if that was zero.
fn recolor(original: Argb, color: Color) -> Argb {
    let alpha = match original.a {
        0 => 0xff,
        a => a,
    };
    Argb::new(alpha, color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: Tint = Tint {
        color: Argb::new(0xcc, 0x20, 0x20, 0x20),
        opacity: 0.5,
        fallback: Some(Argb::new(0xff, 0x20, 0x20, 0x20)),
    };
    const BLUE: Color = Color::new(0x33, 0x66, 0x99);

    #[test]
    fn opacity_is_a_percentage() {
        assert_eq!(Opacity::from_percent(0).map(Opacity::fraction), Some(0.0));
        assert_eq!(Opacity::from_percent(100).map(Opacity::fraction), Some(1.0));
        assert_eq!(Opacity::from_percent(40).map(Opacity::percent), Some(40));
        assert_eq!(Opacity::from_percent(101), None);
        assert_eq!(Opacity::from_percent(u8::MAX), None);
    }

    #[test]
    fn the_target_keeps_the_original_alpha_and_opacity() {
        let brush = BrushTint::new(WINDOWS);
        let tint = brush.target(BLUE, None);
        assert_eq!(tint.color, Argb::new(0xcc, 0x33, 0x66, 0x99));
        assert_eq!(tint.fallback, Some(Argb::new(0xff, 0x33, 0x66, 0x99)));
        assert_eq!(tint.opacity, 0.5);
        let tint = brush.target(BLUE, Opacity::from_percent(80));
        assert_eq!(tint.opacity, 0.8);
    }

    #[test]
    fn a_transparent_original_gets_an_opaque_tint() {
        let brush = BrushTint::new(Tint {
            color: Argb::new(0, 1, 2, 3),
            opacity: 0.0,
            fallback: Some(Argb::new(0, 1, 2, 3)),
        });
        let tint = brush.target(BLUE, None);
        assert_eq!(tint.color.a, 0xff);
        assert_eq!(tint.fallback.map(|f| f.a), Some(0xff));
    }

    #[test]
    fn an_unreadable_fallback_stays_untouched() {
        let brush = BrushTint::new(Tint {
            fallback: None,
            ..WINDOWS
        });
        assert_eq!(brush.target(BLUE, None).fallback, None);
    }

    #[test]
    fn our_own_tint_is_never_taken_for_the_original() {
        let mut brush = BrushTint::new(WINDOWS);
        let ours = brush.target(BLUE, None);
        brush.applied(ours);
        assert!(!brush.observe(ours));
        // Rounding in the opacity round trip is not a change by Windows.
        assert!(!brush.observe(Tint {
            opacity: ours.opacity + 1e-9,
            ..ours
        }));
        assert_eq!(brush.original(), WINDOWS);
        // A fallback Windows changed alone is a change too.
        assert!(brush.observe(Tint {
            fallback: Some(Argb::new(0xff, 9, 9, 9)),
            ..ours
        }));
    }

    #[test]
    fn a_tint_windows_set_after_ours_becomes_the_original() {
        let mut brush = BrushTint::new(WINDOWS);
        brush.applied(brush.target(BLUE, None));
        let light = Tint {
            color: Argb::new(0xcc, 0xf3, 0xf3, 0xf3),
            opacity: 0.6,
            fallback: Some(Argb::new(0xff, 0xf3, 0xf3, 0xf3)),
        };
        assert!(brush.observe(light));
        assert_eq!(brush.original(), light);
        assert_eq!(brush.target(BLUE, None).opacity, 0.6);
    }

    #[test]
    fn after_a_restore_the_current_tint_is_the_original() {
        let mut brush = BrushTint::new(WINDOWS);
        brush.applied(brush.target(BLUE, None));
        brush.restored();
        let other = Tint {
            color: Argb::new(0xff, 1, 1, 1),
            opacity: 0.1,
            fallback: None,
        };
        assert!(!brush.observe(other));
        assert_eq!(brush.original(), other);
    }

    #[test]
    fn argb_prints_as_xaml_writes_it() {
        assert_eq!(Argb::new(0xff, 0x0a, 0xbc, 0xde).to_string(), "#FF0ABCDE");
    }
}
