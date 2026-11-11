//! Set-of-Mark vision compression.
//!
//! Screenshots are the most expensive thing an operator can send a model. A 4K
//! frame is thousands of tokens and most of it is dead pixels. Spacetime's
//! approximation of OmniParser does what the real model does at a smaller scale:
//!
//! 1. walk the accessibility/UI tree the guest exposes;
//! 2. keep only *interactive* elements (buttons, fields, menu items, links);
//! 3. downscale the frame and stamp numbered boxes on those elements;
//! 4. send the model the numbered list plus the small image.
//!
//! Clicking `#14` then needs no pixel coordinate reasoning, which is where
//! computer-use models usually hallucinate. The real implementation swaps the
//! tree walk for an ONNX detector (`omniparser`/YOLOv8 via the `ort` crate) and
//! keeps this exact output format.

use spacetime_core::json::Value;

/// What an element does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Button,
    TextField,
    MenuItem,
    Link,
    Tab,
    Slider,
    Checkbox,
    Icon,
    Static,
}

impl ElementKind {
    pub fn label(self) -> &'static str {
        match self {
            ElementKind::Button => "button",
            ElementKind::TextField => "field",
            ElementKind::MenuItem => "menu",
            ElementKind::Link => "link",
            ElementKind::Tab => "tab",
            ElementKind::Slider => "slider",
            ElementKind::Checkbox => "checkbox",
            ElementKind::Icon => "icon",
            ElementKind::Static => "label",
        }
    }

    pub fn interactive(self) -> bool {
        !matches!(self, ElementKind::Static)
    }
}

/// A boxed, numbered UI element.
#[derive(Debug, Clone)]
pub struct MarkedElement {
    /// The `[n]` the model sees.
    pub mark: u32,
    pub kind: ElementKind,
    pub label: String,
    /// Bounds in screen pixels (x, y, w, h).
    pub rect: (u32, u32, u32, u32),
    pub enabled: bool,
}

impl MarkedElement {
    pub fn center(&self) -> (u32, u32) {
        (
            self.rect.0 + self.rect.2 / 2,
            self.rect.1 + self.rect.3 / 2,
        )
    }
}

/// One captured frame plus its compressed representation.
#[derive(Debug, Clone)]
pub struct Screen {
    /// The tab whose guest was captured.
    pub tab: u64,
    pub width: u32,
    pub height: u32,
    pub elements: Vec<MarkedElement>,
    /// Downscaled frame dimensions sent to the model.
    pub sent_width: u32,
    pub sent_height: u32,
}

impl Screen {
    /// Build a screen from raw elements, filtering out non-interactive ones and
    /// assigning marks in reading order (top-to-bottom, left-to-right).
    pub fn from_elements(
        tab: u64,
        width: u32,
        height: u32,
        mut elements: Vec<MarkedElement>,
    ) -> Screen {
        elements.retain(|e| e.kind.interactive() && e.enabled);
        elements.sort_by_key(|e| (e.rect.1, e.rect.0));
        for (index, element) in elements.iter_mut().enumerate() {
            element.mark = index as u32 + 1;
        }
        let scale = 0.5;
        Screen {
            tab,
            width,
            height,
            elements,
            sent_width: (width as f32 * scale) as u32,
            sent_height: (height as f32 * scale) as u32,
        }
    }

    pub fn find(&self, mark: u32) -> Option<&MarkedElement> {
        self.elements.iter().find(|e| e.mark == mark)
    }

    /// Resolve a model's `"click #7"` into absolute pixel coordinates.
    pub fn resolve(&self, target: &str) -> Option<(u32, u32, String)> {
        let mark: u32 = target
            .trim()
            .trim_start_matches('#')
            .trim_start_matches("click ")
            .trim_start_matches('#')
            .parse()
            .ok()?;
        let element = self.find(mark)?;
        let (x, y) = element.center();
        Some((x, y, element.label.clone()))
    }

    /// The text form sent to the model, one line per element.
    pub fn markup(&self) -> String {
        let mut out = String::new();
        for element in &self.elements {
            out.push_str(&format!(
                "[{}] {} \"{}\" at {},{} {}x{}\n",
                element.mark,
                element.kind.label(),
                element.label,
                element.rect.0,
                element.rect.1,
                element.rect.2,
                element.rect.3
            ));
        }
        out
    }

    /// Rough token cost estimate of what the model receives.
    pub fn token_cost(&self) -> u32 {
        // ~1 token per 4 chars of markup + a flat charge for the downscaled image.
        (self.markup().len() as u32 / 4) + 256
    }

    pub fn to_json(&self) -> Value {
        let elements: Vec<Value> = self
            .elements
            .iter()
            .map(|e| {
                Value::obj()
                    .with("mark", e.mark as u64)
                    .with("kind", e.kind.label())
                    .with("label", e.label.as_str())
                    .with("x", e.rect.0 as u64)
                    .with("y", e.rect.1 as u64)
                    .with("w", e.rect.2 as u64)
                    .with("h", e.rect.3 as u64)
            })
            .collect();
        Value::obj()
            .with("tab", self.tab)
            .with("width", self.width as u64)
            .with("height", self.height as u64)
            .with("sent_width", self.sent_width as u64)
            .with("sent_height", self.sent_height as u64)
            .with("tokens", self.token_cost() as u64)
            .with("elements", Value::Arr(elements))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(kind: ElementKind, label: &str, rect: (u32, u32, u32, u32)) -> MarkedElement {
        MarkedElement {
            mark: 0,
            kind,
            label: label.to_string(),
            rect,
            enabled: true,
        }
    }

    #[test]
    fn only_interactive_elements_are_marked() {
        let screen = Screen::from_elements(
            1,
            1920,
            1080,
            vec![
                el(ElementKind::Static, "Timeline", (0, 0, 1920, 40)),
                el(ElementKind::Button, "Split", (900, 500, 80, 30)),
                el(ElementKind::TextField, "Search", (10, 10, 200, 30)),
            ],
        );
        assert_eq!(screen.elements.len(), 2);
        assert!(screen.elements.iter().all(|e| e.mark > 0));
    }

    #[test]
    fn marks_follow_reading_order() {
        let screen = Screen::from_elements(
            1,
            1000,
            800,
            vec![
                el(ElementKind::Button, "bottom", (10, 700, 50, 20)),
                el(ElementKind::Button, "top-right", (800, 10, 50, 20)),
                el(ElementKind::Button, "top-left", (10, 10, 50, 20)),
            ],
        );
        assert_eq!(screen.elements[0].label, "top-left");
        assert_eq!(screen.elements[1].label, "top-right");
        assert_eq!(screen.elements[2].label, "bottom");
    }

    #[test]
    fn resolves_marks_to_pixels() {
        let screen = Screen::from_elements(
            1,
            1000,
            800,
            vec![el(ElementKind::Button, "Export", (100, 200, 80, 40))],
        );
        let (x, y, label) = screen.resolve("click #1").unwrap();
        assert_eq!((x, y), (140, 220));
        assert_eq!(label, "Export");
        assert!(screen.resolve("#99").is_none());
        assert!(screen.resolve("garbage").is_none());
    }

    #[test]
    fn markup_is_compact() {
        let screen = Screen::from_elements(
            1,
            3840,
            2160,
            vec![
                el(ElementKind::Button, "Split", (100, 100, 60, 20)),
                el(ElementKind::MenuItem, "File", (0, 0, 40, 20)),
            ],
        );
        let markup = screen.markup();
        assert!(markup.contains("[1] menu \"File\""));
        assert!(screen.token_cost() < 400);
        assert_eq!(screen.sent_width, 1920);
    }

    #[test]
    fn disabled_elements_are_dropped() {
        let mut disabled = el(ElementKind::Button, "Export", (0, 0, 10, 10));
        disabled.enabled = false;
        let screen = Screen::from_elements(1, 100, 100, vec![disabled]);
        assert!(screen.elements.is_empty());
    }
}
