//! The XAML visual tree of a shell process, as XAML Diagnostics reports it.
//!
//! The hook DLL receives one callback per element added to or removed from
//! the tree, each naming the element, its parent, its type and its `x:Name`.
//! This module keeps that information so the DLL can tell which elements to
//! style. Everything here is plain data, so the matching rules are tested on
//! Linux.

use std::collections::HashMap;

use super::rules::{Rule, Step};

/// XAML Diagnostics' `InstanceHandle`. Zero means "no element".
pub type Handle = u64;

/// A shell process holds a few thousand elements. This cap only stops a
/// broken or hostile stream of callbacks from growing memory inside the
/// shell process without bound.
pub const MAX_ELEMENTS: usize = 200_000;
/// Ancestors `ElementTree::path` lists before stopping.
pub const MAX_PATH_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub parent: Option<Handle>,
    pub type_name: Box<str>,
    pub name: Box<str>,
    /// Its place among its parent's children (0-based) as XAML reported
    /// it when the element was added; `None` if not reported.
    pub index: Option<u32>,
}

/// Whether an element is a rule's target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Yes,
    No,
    /// Depends on an ancestor not reported yet.
    NotYet,
}

impl Verdict {
    /// Both must hold: `No` wins over `NotYet`, which wins over `Yes`.
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::No, _) | (_, Self::No) => Self::No,
            (Self::NotYet, _) | (_, Self::NotYet) => Self::NotYet,
            (Self::Yes, Self::Yes) => Self::Yes,
        }
    }
}

impl From<bool> for Verdict {
    fn from(yes: bool) -> Self {
        if yes { Self::Yes } else { Self::No }
    }
}

#[derive(Debug, Default)]
pub struct ElementTree {
    elements: HashMap<Handle, Element>,
}

impl ElementTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn get(&self, handle: Handle) -> Option<&Element> {
        self.elements.get(&handle)
    }

    /// Records an added element; a parent of 0 marks a root. Adding a handle
    /// that is already known replaces it (the element moved). Returns false,
    /// and records nothing, for handle 0 or when the tree is full.
    pub fn add(&mut self, handle: Handle, parent: Handle, type_name: &str, name: &str) -> bool {
        self.add_at(handle, parent, None, type_name, name)
    }

    /// `add`, with the element's place among its parent's children.
    pub fn add_at(
        &mut self,
        handle: Handle,
        parent: Handle,
        index: Option<u32>,
        type_name: &str,
        name: &str,
    ) -> bool {
        if handle == 0 {
            return false;
        }
        if self.elements.len() >= MAX_ELEMENTS && !self.elements.contains_key(&handle) {
            return false;
        }
        let element = Element {
            parent: (parent != 0 && parent != handle).then_some(parent),
            type_name: type_name.into(),
            name: name.into(),
            index,
        };
        self.elements.insert(handle, element);
        true
    }

    /// Forgets an element and everything below it, and returns every handle
    /// that was forgotten. XAML Diagnostics may or may not report each
    /// descendant's removal separately; this way nothing stale stays behind
    /// either way.
    pub fn remove(&mut self, handle: Handle) -> Vec<Handle> {
        if !self.elements.contains_key(&handle) {
            return Vec::new();
        }
        let mut children: HashMap<Handle, Vec<Handle>> = HashMap::new();
        for (&child, element) in &self.elements {
            if let Some(parent) = element.parent {
                children.entry(parent).or_default().push(child);
            }
        }
        let mut removed = Vec::new();
        let mut pending = vec![handle];
        while let Some(next) = pending.pop() {
            if self.elements.remove(&next).is_some() {
                removed.push(next);
                if let Some(below) = children.remove(&next) {
                    pending.extend(below);
                }
            }
        }
        removed
    }

    /// The element's known ancestors, nearest first. Stops at the first
    /// unknown parent, and after `len()` steps so that a cycle in the
    /// reported parents cannot loop forever.
    pub fn ancestors(&self, handle: Handle) -> impl Iterator<Item = &Element> {
        let mut current = self.elements.get(&handle).and_then(|e| e.parent);
        let mut steps_left = self.elements.len();
        std::iter::from_fn(move || {
            if steps_left == 0 {
                return None;
            }
            steps_left -= 1;
            let element = self.elements.get(&current?)?;
            current = element.parent;
            Some(element)
        })
    }

    /// Whether the element's chain of parents is known all the way to the
    /// tree's root. A complete chain cannot gain ancestors later (XAML
    /// reports a move as a removal and an addition), so a rule that does
    /// not match such an element never will.
    pub fn reaches_root(&self, handle: Handle) -> bool {
        let mut current = handle;
        for _ in 0..=self.elements.len() {
            match self.elements.get(&current) {
                None => return false,
                Some(element) => match element.parent {
                    None => return true,
                    Some(parent) => current = parent,
                },
            }
        }
        // A cycle: never complete.
        false
    }

    /// The element's parent, if it has one and it is known.
    pub fn parent(&self, handle: Handle) -> Option<Handle> {
        self.elements.get(&handle)?.parent
    }

    /// The element and its known ancestors as `Type#Name < Type#Name < …`,
    /// nearest first, for logs. At most `MAX_PATH_DEPTH` ancestors.
    pub fn path(&self, handle: Handle) -> String {
        let describe = |e: &Element| {
            if e.name.is_empty() {
                e.type_name.to_string()
            } else {
                format!("{}#{}", e.type_name, e.name)
            }
        };
        let Some(element) = self.elements.get(&handle) else {
            return format!("(unknown {handle:#x})");
        };
        std::iter::once(element)
            .chain(self.ancestors(handle).take(MAX_PATH_DEPTH))
            .map(describe)
            .collect::<Vec<_>>()
            .join(" < ")
    }

    /// Whether the element is one `rule` styles: right type and name, and
    /// the required ancestors among its known ancestors.
    pub fn matches(&self, handle: Handle, rule: &Rule) -> bool {
        self.verdict(handle, rule) == Verdict::Yes
    }

    /// Whether the element is one `rule` styles, or `NotYet` while an
    /// ancestor the answer depends on has not been reported.
    pub fn verdict(&self, handle: Handle, rule: &Rule) -> Verdict {
        let Some(element) = self.elements.get(&handle) else {
            return Verdict::No;
        };
        if !rule.selects(&element.type_name, &element.name) || !at(element, rule.position) {
            return Verdict::No;
        }
        let parent = match rule.parent {
            None => Verdict::Yes,
            Some(name) => match element.parent {
                None => Verdict::No,
                Some(parent) => match self.elements.get(&parent) {
                    None => Verdict::NotYet,
                    Some(parent) => Verdict::from(&*parent.name == name),
                },
            },
        };
        let ancestor = match rule.ancestor {
            None => Verdict::Yes,
            Some(name) if self.ancestors(handle).any(|a| &*a.name == name) => Verdict::Yes,
            Some(_) if self.reaches_root(handle) => Verdict::No,
            Some(_) => Verdict::NotYet,
        };
        let chain = self.chain_verdict(element.parent, rule.chain, self.elements.len());
        parent.and(ancestor).and(chain)
    }

    /// Whether the ancestors from `start` up match `steps`, nearest first.
    /// `AnyDepth` tries every number of skipped ancestors, none first.
    /// `budget` bounds the walk, so a cycle in the reported parents cannot
    /// loop forever.
    fn chain_verdict(&self, start: Option<Handle>, steps: &[Step], budget: usize) -> Verdict {
        let Some((step, rest)) = steps.split_first() else {
            return Verdict::Yes;
        };
        if budget == 0 {
            return Verdict::No;
        }
        match *step {
            Step::Element {
                type_name,
                name,
                position,
            } => {
                let Some(handle) = start else {
                    return Verdict::No;
                };
                let Some(element) = self.elements.get(&handle) else {
                    return Verdict::NotYet;
                };
                if &*element.type_name == type_name
                    && name.accepts(&element.name)
                    && at(element, position)
                {
                    self.chain_verdict(element.parent, rest, budget - 1)
                } else {
                    Verdict::No
                }
            }
            Step::AnyDepth => {
                let (mut current, mut undecided) = (start, false);
                for left in (0..budget).rev() {
                    match self.chain_verdict(current, rest, left) {
                        Verdict::Yes => return Verdict::Yes,
                        Verdict::NotYet => undecided = true,
                        Verdict::No => {}
                    }
                    let Some(handle) = current else {
                        break;
                    };
                    match self.elements.get(&handle) {
                        Some(element) => current = element.parent,
                        None => {
                            undecided = true;
                            break;
                        }
                    }
                }
                if undecided {
                    Verdict::NotYet
                } else {
                    Verdict::No
                }
            }
        }
    }

    /// Every element `rule` styles, in no particular order.
    pub fn matching(&self, rule: &Rule) -> Vec<Handle> {
        self.elements
            .keys()
            .copied()
            .filter(|&h| self.matches(h, rule))
            .collect()
    }
}

/// Whether `element` is at `position` among its parent's children, if a
/// position is asked for.
fn at(element: &Element, position: Option<u32>) -> bool {
    position.is_none_or(|p| element.index == Some(p))
}

#[cfg(test)]
mod tests;
