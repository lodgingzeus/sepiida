//! A Windhawk theme turned into our rules. The theme's target and style
//! lines are pasted as they are; its variables are replaced by ours
//! (`Var`), so its colours and blur become our palette and our tint. XAML
//! markup is never loaded: a `:=` value must be one of our variables or a
//! transparent brush.
//!
//! Each target entry is all or nothing: one unsupported selector or style
//! skips the whole entry (the styles of an entry depend on each other), and
//! the reason is kept for the log and the tests.

use super::{
    selector::{Target, parse_targets},
    style::{Style, StyleValue, parse_style},
};
use crate::shell::{
    layout::Value,
    rules::{Effect, Paint, Role, Rule},
};

/// What one of the theme's `$Variables` stands for here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Var {
    /// A value as written (`14`).
    Text(&'static str),
    /// Windows' own acrylic brush, tinted with the role's colour (always
    /// on while styling). Stands in for the theme's blur and acrylic.
    Tint(Role),
    /// A solid brush of the role's colour (always on while styling).
    Solid(Role),
    /// Our transparent brush, while the look is on.
    Transparent,
    /// The theme's blur: Windows' acrylic tinted with the role's colour
    /// always, and our blur brush (`amount`, the colour at `opacity`)
    /// over it while the look is on.
    Blur {
        amount: f32,
        role: Role,
        opacity: f32,
    },
}

/// A theme: Windhawk's `ThemeTargetStyles` entries and our variables.
pub struct Theme {
    pub targets: &'static [(&'static str, &'static [&'static str])],
    pub vars: &'static [(&'static str, Var)],
}

/// A target entry left out, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub target: &'static str,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Built {
    pub rules: Vec<Rule>,
    pub skipped: Vec<Skipped>,
}

/// Properties that hold a brush, where `Transparent` means our brush.
const BRUSH_PROPERTIES: [&str; 5] = ["Background", "BorderBrush", "Fill", "Foreground", "Stroke"];

pub fn build(theme: &Theme) -> Built {
    let mut built = Built::default();
    for &(target, styles) in theme.targets {
        match entry_rules(target, styles, theme.vars) {
            Ok(rules) => built.rules.extend(rules),
            Err(reason) => built.skipped.push(Skipped { target, reason }),
        }
    }
    built
}

fn entry_rules(
    target: &'static str,
    styles: &[&'static str],
    vars: &[(&'static str, Var)],
) -> Result<Vec<Rule>, String> {
    let targets = parse_targets(target).map_err(|e| e.to_string())?;
    let effects = styles
        .iter()
        .map(|&text| {
            let style = parse_style(text).map_err(|e| format!("{text}: {e}"))?;
            effects(style, vars)
                .map(|effects| effects.into_iter().map(move |e| (style.property, e)))
                .map_err(|e| format!("{text}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    Ok(targets
        .iter()
        .flat_map(|t| {
            effects
                .iter()
                .map(move |&(property, effect)| rule(target, t, property, effect))
        })
        .collect())
}

fn rule(label: &'static str, target: &Target, property: &'static str, effect: Effect) -> Rule {
    Rule {
        label,
        type_name: target.type_name,
        name: target.name,
        ancestor: None,
        parent: None,
        chain: target.chain,
        property,
        effect,
    }
}

/// What one style line does: usually one effect; a blur is a tint and a
/// blur brush.
fn effects(style: Style, vars: &[(&'static str, Var)]) -> Result<Vec<Effect>, String> {
    if let StyleValue::Text(text) | StyleValue::Xaml(text) = style.value
        && let Some(name) = text.strip_prefix('$')
        && let Some(&(
            _,
            Var::Blur {
                amount,
                role,
                opacity,
            },
        )) = vars.iter().find(|(n, _)| *n == name)
    {
        return Ok(vec![color(Paint::Tint, role), blur(amount, role, opacity)?]);
    }
    effect(style, vars).map(|e| vec![e])
}

fn blur(amount: f32, role: Role, opacity: f32) -> Result<Effect, String> {
    let value = Value::Blur {
        amount,
        role,
        opacity,
    };
    if value.is_valid() {
        Ok(Effect::Set(value))
    } else {
        Err(format!("blur {amount} at {opacity} is out of range"))
    }
}

/// `<WindhawkBlur BlurAmount="18" TintColor="#80000000"/>`: its amount and
/// the tint's alpha; the tint's colour is replaced by the Surface colour.
fn windhawk_blur(text: &str) -> Result<Effect, String> {
    let attribute = |name: &str| {
        let start = text.find(&format!("{name}=\""))? + name.len() + 2;
        let end = text[start..].find('"')? + start;
        Some(&text[start..end])
    };
    let amount = match attribute("BlurAmount") {
        Some(a) => a
            .parse::<f32>()
            .map_err(|_| format!("bad BlurAmount {a:?}"))?,
        None => 0.0,
    };
    let opacity = match attribute("TintColor") {
        Some(tint) => {
            let hex = tint.strip_prefix('#').ok_or("TintColor is not #AARRGGBB")?;
            match hex.len() {
                8 => u8::from_str_radix(&hex[..2], 16).map_err(|_| "bad TintColor")?,
                6 => 0xff,
                _ => return Err("TintColor is not #AARRGGBB".to_owned()),
            }
        }
        None => 0,
    };
    blur(amount, Role::Surface, f32::from(opacity) / 255.0)
}

fn effect(style: Style, vars: &[(&'static str, Var)]) -> Result<Effect, String> {
    let (text, xaml) = match style.value {
        StyleValue::Text(text) => (text, false),
        StyleValue::Xaml(text) => (text, true),
    };
    if let Some(name) = text.strip_prefix('$') {
        let var = vars
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, var)| var)
            .ok_or_else(|| format!("unknown variable ${name}"))?;
        return Ok(match var {
            Var::Text(text) => text_effect(style.property, text)?,
            Var::Tint(role) => color(Paint::Tint, role),
            Var::Solid(role) => color(Paint::Solid, role),
            Var::Transparent => Effect::Set(Value::Transparent),
            Var::Blur {
                amount,
                role,
                opacity,
            } => blur(amount, role, opacity)?,
        });
    }
    if xaml {
        let compact: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        return match compact.as_str() {
            "<SolidColorBrush Color=\"Transparent\"/>"
            | "<SolidColorBrush Color=\"Transparent\" />" => Ok(Effect::Set(Value::Transparent)),
            _ if compact.starts_with("<WindhawkBlur") => windhawk_blur(&compact),
            _ => {
                let value = Value::Xaml(text);
                if value.is_valid() {
                    Ok(Effect::Set(value))
                } else {
                    Err("XAML value refused (unsafe or malformed)".to_owned())
                }
            }
        };
    }
    text_effect(style.property, text)
}

fn color(paint: Paint, role: Role) -> Effect {
    Effect::Color { paint, role }
}

fn text_effect(property: &str, text: &'static str) -> Result<Effect, String> {
    let value = match text {
        "Transparent" if BRUSH_PROPERTIES.contains(&property) => Value::Transparent,
        "Auto" => Value::Auto,
        _ => Value::Text(text),
    };
    if value.is_valid() {
        Ok(Effect::Set(value))
    } else {
        Err(format!("{text:?} is not a plain value"))
    }
}
