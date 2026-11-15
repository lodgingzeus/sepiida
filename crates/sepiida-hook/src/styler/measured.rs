//! Measured layouts (`Effect::Measure`): values a Windhawk theme computes
//! from captured sizes, worked out from sizes read on the property chain.
//! The taskbar's glass bar ends before the tray
//! (`sepiida_core::shell::rules::taskbar_bar_margin`); a centred dock's
//! frame is at most as wide as its grid leaves beside the tray
//! (`measure::dock_max_width`); Start's View button moves up past the
//! pinned list (`measure::view_button_shift`). `layout` writes them.

use std::collections::HashMap;

use sepiida_core::shell::{
    finder::Match,
    layout::Value,
    measure::{dock_max_width, parse_length, view_button_shift},
    rules::{Part, taskbar_bar_margin},
    tree::Handle,
};

use crate::{log::log, properties::Xaml};

#[derive(Default)]
pub struct Measured {
    /// The element read for each part.
    parts: HashMap<Part, Handle>,
    /// The target of each computed part.
    outputs: HashMap<Part, Match>,
    /// The grid that holds the dock's frame.
    dock_container: Option<Handle>,
}

impl Measured {
    pub fn note(&mut self, found: Match, part: Part, parent: Option<Handle>) {
        self.parts.insert(part, found.handle);
        if part.placeholder().is_some() {
            self.outputs.insert(part, found);
        }
        if part == Part::Dock {
            self.dock_container = parent;
        }
    }

    pub fn forget(&mut self, handles: &[Handle]) {
        self.parts.retain(|_, h| !handles.contains(h));
        self.outputs.retain(|_, m| !handles.contains(&m.handle));
        if self.dock_container.is_some_and(|h| handles.contains(&h)) {
            self.dock_container = None;
        }
    }

    /// Each computed part's target and the value it should have now. A
    /// part whose inputs are missing or make no sense is left out.
    pub fn placements(&self, xaml: &Xaml) -> Vec<(Match, Value)> {
        self.outputs
            .iter()
            .filter_map(|(&part, &target)| Some((target, self.value(xaml, part)?)))
            .collect()
    }

    fn value(&self, xaml: &Xaml, part: Part) -> Option<Value> {
        match part {
            Part::Bar => {
                let width = self.read(xaml, Part::Tray, "ActualWidth");
                let margin = width.and_then(taskbar_bar_margin);
                if margin.is_none() {
                    log!("taskbar layout: tray width {width:?}; bar not placed");
                }
                margin
            }
            Part::Dock => {
                let container = xaml.value(self.dock_container?, "ActualWidth");
                let container = container.as_deref().and_then(parse_length);
                let tray = self.read(xaml, Part::Tray, "ActualWidth");
                let width = dock_max_width(container?, tray?);
                if width.is_none() {
                    log!("dock layout: grid {container:?}, tray {tray:?}; width not set");
                }
                width.map(Value::Length)
            }
            Part::ViewButton => {
                let height = self.read(xaml, Part::PinnedList, "ActualHeight");
                let shift = height.and_then(view_button_shift);
                if shift.is_none() {
                    log!("Start layout: pinned list height {height:?}; View button not moved");
                }
                shift.map(|(x, y)| Value::Translate { x, y })
            }
            Part::Tray | Part::PinnedList => None,
        }
    }

    /// A length property of the element read for `part`.
    fn read(&self, xaml: &Xaml, part: Part, property: &str) -> Option<f32> {
        let handle = *self.parts.get(&part)?;
        xaml.value(handle, property)
            .as_deref()
            .and_then(parse_length)
    }
}
