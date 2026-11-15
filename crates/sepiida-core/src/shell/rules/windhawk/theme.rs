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
    rules::{Effect, Material, Paint, Role, Rule},
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
    /// A brush of the role's colour, while the look is on: the solid
    /// material's stand-in for glass.
    Fill(Role),
    /// One variable for the glass material and another for the solid one
    /// (a setting); each makes rules that hold for its material only.
    Material {
        glass: &'static Var,
        solid: &'static Var,
    },
}

/// A theme: Windhawk's `ThemeTargetStyles` entries and our variables, for
/// the elements of one XAML framework (its bare type names are that
/// framework's).
pub struct Theme {
    pub framework: Framework,
    /// The material the theme's look rules hold for; `None` for both.
    pub material: Option<Material>,
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
    build_on(&[], themes)
}

/// Themes over hand-written rules: a theme line replaces a hand-written
/// rule for the same slot, as a later Windhawk line does.
pub fn build_on(base: &[Rule], themes: &[&Theme]) -> Built {
    let mut built = Built {
        rules: base.to_vec(),
        skipped: Vec::new(),
    };
    for theme in themes {
        add_theme(&mut built, theme);
    }
    built
}

fn add_theme(built: &mut Built, theme: &Theme) {
    for &(target, styles) in theme.targets {
        match entry_rules(target, styles, theme) {
            Ok(rules) => {
                for rule in rules {
                    give_way(&mut built.rules, &rule);
                    built.rules.push(rule);
                }
            }
            Err(reason) => built.skipped.push(Skipped { target, reason }),
        }
    }
}

/// Earlier rules make way for `rule`, as for a later Windhawk line: one
/// for the same slot is replaced where their materials meet. An earlier
/// rule for both materials, met by a later one for one material, keeps
/// the other.
fn give_way(kept: &mut Vec<Rule>, rule: &Rule) {
    kept.retain_mut(|earlier| {
        if !same_slot(earlier, rule) {
            return true;
        }
        match (earlier.material, rule.material) {
            (None, Some(material)) => {
                earlier.material = Some(material.other());
                true
            }
            (Some(a), Some(b)) if a != b => true,
            _ => false,
        }
    });
}

/// Whether two rules write the same property of the same selected element
/// in the same way (a colour, or a look value), whatever their materials.
fn same_slot(a: &Rule, b: &Rule) -> bool {
    let is_look = |r: &Rule| matches!(r.effect, Effect::Set(_));
    a.type_name == b.type_name
        && a.name == b.name
        && a.chain == b.chain
        && a.position == b.position
        && a.conditions == b.conditions
        && a.property == b.property
        && is_look(a) == is_look(b)
}

fn entry_rules(
    target: &'static str,
    styles: &[&'static str],
    theme: &Theme,
) -> Result<Vec<Rule>, String> {
    let (vars, framework) = (theme.vars, theme.framework);
    let targets = parse_targets(target, framework).map_err(|e| e.to_string())?;
    let effects = styles
        .iter()
        .map(|&text| {
            let style = parse_style(text).map_err(|e| format!("{text}: {e}"))?;
            attached_ok(style, framework).map_err(|e| format!("{text}: {e}"))?;
            effects(style, vars)
                .map(|effects| {
                    effects.into_iter().map(move |(effect, material)| {
                        // Colours hold for both materials; a look line
                        // takes its variable's material, else the theme's.
                        let material = match effect {
                            Effect::Set(_) => material.or(theme.material),
                            _ => None,
                        };
                        (style.property, effect, material)
                    })
                })
                .map_err(|e| format!("{text}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    Ok(targets
        .iter()
        .flat_map(|t| {
            effects.iter().map(move |&(property, effect, material)| {
                rule(target, t, property, effect, material)
            })
        })
        .collect())
}

/// Attached properties are set through system XAML's `DependencyObject`
/// (the DLL has no WinUI 3 bindings for it), with a plain value.
fn attached_ok(style: Style, framework: Framework) -> Result<(), String> {
    if !crate::shell::markup::is_attached(style.property) {
        return Ok(());
    }
    if framework == Framework::WinUi {
        return Err("an attached property in WinUI 3 is not supported yet".to_owned());
    }
    match style.value {
        StyleValue::Text(_) => Ok(()),
        StyleValue::Xaml(text) if !text.trim_start().starts_with('<') => Ok(()),
        StyleValue::Xaml(_) => {
            Err("an attached property with a XAML value is not supported yet".to_owned())
        }
    }
}

fn rule(
    label: &'static str,
    target: &Target,
    property: &'static str,
    effect: Effect,
    material: Option<Material>,
) -> Rule {
    Rule {
        label,
        type_name: target.type_name,
        name: target.name,
        ancestor: None,
        parent: None,
        chain: target.chain,
        position: target.position,
        conditions: target.conditions,
        material,
        property,
        effect,
    }
}

/// What one style line does: usually one effect; a blur is a tint and a
/// blur brush; a material variable makes the effects of each of its two
/// variables, each for its material.
fn effects(
    style: Style,
    vars: &[(&'static str, Var)],
) -> Result<Vec<(Effect, Option<Material>)>, String> {
    if let StyleValue::Text(text) | StyleValue::Xaml(text) = style.value
        && let Some(name) = text.strip_prefix('$')
        && let Some(&(_, var)) = vars.iter().find(|(n, _)| *n == name)
    {
        return match var {
            Var::Material { glass, solid } => {
                let mut out = Vec::new();
                for (var, material) in [(*glass, Material::Glass), (*solid, Material::Solid)] {
                    for effect in var_effects(style.property, var)? {
                        let tagged = (effect, Some(material));
                        // A colour both make is kept once (it holds for both).
                        if matches!(effect, Effect::Set(_))
                            || !out.iter().any(|(e, _)| *e == effect)
                        {
                            out.push(tagged);
                        }
                    }
                }
                Ok(out)
            }
            var => Ok(var_effects(style.property, var)?
                .into_iter()
                .map(|e| (e, None))
                .collect()),
        };
    }
    effect(style, vars).map(|e| vec![(e, None)])
}

/// The effects of one variable used as a whole value.
fn var_effects(property: &str, var: Var) -> Result<Vec<Effect>, String> {
    Ok(match var {
        Var::Blur {
            amount,
            role,
            opacity,
        } => vec![color(Paint::Tint, role), blur(amount, role, opacity)?],
        Var::Text(text) => vec![text_effect(property, text)?],
        Var::Tint(role) => vec![color(Paint::Tint, role)],
        Var::Solid(role) => vec![color(Paint::Solid, role)],
        Var::Transparent => vec![Effect::Set(Value::Transparent)],
        Var::Acrylic(role) => vec![Effect::Set(Value::Acrylic(role))],
        Var::Fill(role) => vec![Effect::Set(Value::Solid(role))],
        Var::Xaml(text) => vec![xaml_effect(text)?],
        Var::Material { .. } => return Err("a material variable inside another".to_owned()),
    })
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

/// A colour as XAML writes one: `#RGB`, `#ARGB`, `#RRGGBB`, `#AARRGGBB`
/// or a name (`Red`).
fn is_colour(text: &str) -> bool {
    match text.strip_prefix('#') {
        Some(hex) => {
            matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
        }
        None => {
            !text.is_empty() && text.len() <= 32 && text.chars().all(|c| c.is_ascii_alphabetic())
        }
    }
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
        // Blur and material variables are handled by `effects`.
        return var_effects(style.property, var)?
            .pop()
            .ok_or_else(|| format!("${name} makes nothing"));
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
    let brush = BRUSH_PROPERTIES.contains(&property);
    let value = match text {
        _ if brush && text.eq_ignore_ascii_case("Transparent") => Value::Transparent,
        // `Background=#09FFFFFF` or `=Red`: XAML makes a brush from a
        // colour only in markup, so it becomes one.
        _ if brush && is_colour(text) => {
            Value::Xaml(leak(format!("<SolidColorBrush Color=\"{text}\" />")))
        }
        "Auto" => Value::Auto,
        _ => Value::Text(text),
    };
    if value.is_valid() {
        Ok(Effect::Set(value))
    } else {
        Err(format!("{text:?} is not a plain value"))
    }
}
