//! A Windhawk theme turned into our rules. The theme's target and style
//! lines are pasted as they are; its variables are replaced by ours
//! (`Var`), so its colours and blur become our palette and our tint. XAML
//! markup is loaded only when it passes `markup::is_safe`.
//!
//! Each target entry is all or nothing: one unsupported selector or style
//! skips the whole entry (the styles of an entry depend on each other), and
//! the reason is kept for the log and the tests. As in Windhawk, a later
//! line for the same target and property wins over an earlier one.

use super::{
    leak,
    selector::{Target, parse_targets},
    style::{Style, StyleValue, parse_style},
};
use crate::shell::{
    framework::Framework,
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
    /// Our own acrylic brush tinted with the role's colour, while the look
    /// is on (proven in Explorer, M5 step 2; the blur brush is not yet).
    Acrylic(Role),
    /// The theme's own XAML value, kept as written (a neutral border
    /// gradient, a veil from a `ThemeResource`), while the look is on.
    /// Must pass `markup::is_safe`.
    Xaml(&'static str),
}

/// A theme: Windhawk's `ThemeTargetStyles` entries and our variables, for
/// the elements of one XAML framework (its bare type names are that
/// framework's).
pub struct Theme {
    pub framework: Framework,
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
    build_all(&[theme])
}

/// Several themes read as one, in order: a theme split over files for
/// size, or ours after a pasted one.
pub fn build_all(themes: &[&Theme]) -> Built {
    let mut built = Built::default();
    for theme in themes {
        add_theme(&mut built, theme);
    }
    built
}

fn add_theme(built: &mut Built, theme: &Theme) {
    for &(target, styles) in theme.targets {
        match entry_rules(target, styles, theme.vars, theme.framework) {
            Ok(rules) => {
                for rule in rules {
                    built.rules.retain(|kept| !same_slot(kept, &rule));
                    built.rules.push(rule);
                }
            }
            Err(reason) => built.skipped.push(Skipped { target, reason }),
        }
    }
}

/// Whether two rules write the same property of the same selected element
/// in the same way (a colour, or a look value): the later one replaces the
/// earlier, as a later Windhawk line does.
fn same_slot(a: &Rule, b: &Rule) -> bool {
    let is_look = |r: &Rule| matches!(r.effect, Effect::Set(_));
    a.type_name == b.type_name
        && a.name == b.name
        && a.chain == b.chain
        && a.property == b.property
        && is_look(a) == is_look(b)
}

fn entry_rules(
    target: &'static str,
    styles: &[&'static str],
    vars: &[(&'static str, Var)],
    framework: Framework,
) -> Result<Vec<Rule>, String> {
    let targets = parse_targets(target, framework).map_err(|e| e.to_string())?;
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

/// `text` with each `$Name` inside it replaced by that text variable's
/// value (`$CornerRadius,0,0,$CornerRadius`). A value that is only a
/// variable is left for `effect`, which knows every kind of variable.
fn substitute(text: &'static str, vars: &[(&'static str, Var)]) -> Result<&'static str, String> {
    if !text.contains('$') || is_variable(text) {
        return Ok(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let name = &after[..len];
        match vars.iter().find(|(n, _)| *n == name) {
            Some(&(_, Var::Text(value))) => out.push_str(value),
            Some(_) => return Err(format!("${name} is not a text variable")),
            None => return Err(format!("unknown variable ${name}")),
        }
        rest = &after[len..];
    }
    out.push_str(rest);
    Ok(leak(out))
}

/// `$Name` and nothing else.
fn is_variable(text: &str) -> bool {
    text.strip_prefix('$').is_some_and(|name| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn effect(style: Style, vars: &[(&'static str, Var)]) -> Result<Effect, String> {
    let (text, xaml) = match style.value {
        StyleValue::Text(text) => (substitute(text, vars)?, false),
        StyleValue::Xaml(text) => (substitute(text, vars)?, true),
    };
    if is_variable(text) {
        let name = &text[1..];
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
            Var::Acrylic(role) => Effect::Set(Value::Acrylic(role)),
            Var::Xaml(text) => xaml_effect(text)?,
        });
    }
    // `Property:=7` or `:=Transparent`: a plain value, as with `=`.
    if xaml && text.trim_start().starts_with('<') {
        let compact: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        return match compact.as_str() {
            "<SolidColorBrush Color=\"Transparent\"/>"
            | "<SolidColorBrush Color=\"Transparent\" />" => Ok(Effect::Set(Value::Transparent)),
            _ if compact.starts_with("<WindhawkBlur") => windhawk_blur(&compact),
            _ => xaml_effect(text),
        };
    }
    text_effect(style.property, text)
}

fn xaml_effect(text: &'static str) -> Result<Effect, String> {
    let value = Value::Xaml(text);
    if value.is_valid() {
        Ok(Effect::Set(value))
    } else {
        Err("XAML value refused (unsafe or malformed)".to_owned())
    }
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
