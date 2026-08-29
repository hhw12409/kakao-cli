//! A read-only view of one UI Automation element. Both the live UIA tree and a
//! serialized fixture implement `Node`, so the parsers run unchanged in tests
//! without KakaoTalk (mirrors the macOS adapter's `UINode`).

use serde::{Deserialize, Serialize};

pub trait Node {
    /// UIA `ControlType` as a short string: "List", "ListItem", "Text",
    /// "Edit", "Button", "Group", "Pane", "Window", ...
    fn control_type(&self) -> &str;
    fn name(&self) -> Option<&str>;
    fn automation_id(&self) -> Option<&str>;
    /// `ValuePattern.Value` / `LegacyIAccessible.Value`.
    fn value(&self) -> Option<&str>;
    fn class_name(&self) -> Option<&str>;
    /// Screen-space left edge, when captured. Only message-body elements carry
    /// this (outgoing bubbles are right-aligned — same signal as macOS).
    fn bounding_left(&self) -> Option<f64>;
    fn children(&self) -> Vec<&dyn Node>;
}

/// Depth-first helpers shared by the parsers.
pub fn first_descendant<'a>(
    node: &'a dyn Node,
    pred: &dyn Fn(&dyn Node) -> bool,
) -> Option<&'a dyn Node> {
    if pred(node) {
        return Some(node);
    }
    for c in node.children() {
        if let Some(hit) = first_descendant(c, pred) {
            return Some(hit);
        }
    }
    None
}

pub fn descendants<'a>(
    node: &'a dyn Node,
    pred: &dyn Fn(&dyn Node) -> bool,
    out: &mut Vec<&'a dyn Node>,
) {
    for c in node.children() {
        if pred(c) {
            out.push(c);
        }
        descendants(c, pred, out);
    }
}

/// First non-empty text among value / name, trimmed.
pub fn any_text(node: &dyn Node) -> Option<String> {
    for t in [node.value(), node.name()].into_iter().flatten() {
        let t = t.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Fixture node (Codable) — tests + `--dump-tree` output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureNode {
    pub control_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounding_left: Option<f64>,
    #[serde(default)]
    pub children: Vec<FixtureNode>,
}

impl Node for FixtureNode {
    fn control_type(&self) -> &str {
        &self.control_type
    }
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn automation_id(&self) -> Option<&str> {
        self.automation_id.as_deref()
    }
    fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    fn class_name(&self) -> Option<&str> {
        self.class_name.as_deref()
    }
    fn bounding_left(&self) -> Option<f64> {
        self.bounding_left
    }
    fn children(&self) -> Vec<&dyn Node> {
        self.children.iter().map(|c| c as &dyn Node).collect()
    }
}
