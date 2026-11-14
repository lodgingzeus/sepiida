//! The taskbar's measured layout (`Effect::Measure`): reads the tray's
//! width from the property chain and works out where the glass bar ends
//! (`sepiida_core::shell::rules::taskbar_bar_margin`). `layout` writes it.

use std::collections::HashMap;

use sepiida_core::shell::{
    finder::Match,
    layout::Value,
    measure::parse_length,
    rules::{Part, taskbar_bar_margin},
    tree::Handle,
};

use crate::{log::log, properties::Xaml};

#[derive(Default)]
pub struct Measured {
    parts: HashMap<Part, Handle>,
    /// The bar's margin target.
    bar: Option<Match>,
}

impl Measured {
    pub fn note(&mut self, found: Match, part: Part) {
        self.parts.insert(part, found.handle);
        if part == Part::Bar {
            self.bar = Some(found);
        }
    }

    pub fn forget(&mut self, handles: &[Handle]) {
        self.parts.retain(|_, h| !handles.contains(h));
        if self.bar.is_some_and(|b| handles.contains(&b.handle)) {
            self.bar = None;
        }
    }

    /// The bar's margin target and the margin it should have now, or
    /// `None` if a part is missing or the reading makes no sense.
    pub fn placement(&self, xaml: &Xaml) -> Option<(Match, Value)> {
        let bar = self.bar?;
        let tray = *self.parts.get(&Part::Tray)?;
        let width = xaml.value(tray, "ActualWidth");
        let Some(margin) = width
            .as_deref()
            .and_then(parse_length)
            .and_then(taskbar_bar_margin)
        else {
            log!("taskbar layout: tray width {width:?}; bar not placed");
            return None;
        };
        Some((bar, margin))
    }
}
