//! Thin windows-rs wrappers over UI Automation COM. The verbose COM plumbing
//! is absorbed here; `bridge.rs` works with `UiaElement` and `snapshot()`.
//!
//! Windows-only. The parsers run on the `FixtureNode` snapshot, so the live
//! path and the fixture tests share the same parsing code.

#![cfg(windows)]

use windows::core::{BSTR, VARIANT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationValuePattern, UIA_AutomationIdPropertyId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_InvokePatternId, UIA_NamePropertyId, UIA_ValuePatternId,
    UIA_ValueValuePropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use kakao_contract::ErrorCode;

use crate::envelope::{BridgeError, BridgeResult};
use crate::node::FixtureNode;

pub struct Uia {
    pub automation: IUIAutomation,
}

impl Uia {
    pub fn new() -> BridgeResult<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|e| {
                    BridgeError::with(
                        ErrorCode::AccessibilityPermissionDenied,
                        format!("CoCreateInstance(CUIAutomation): {e}"),
                    )
                })?;
            Ok(Self { automation })
        }
    }

    pub fn root(&self) -> BridgeResult<UiaElement> {
        unsafe {
            self.automation
                .GetRootElement()
                .map(|raw| UiaElement { raw })
                .map_err(|e| {
                    BridgeError::with(ErrorCode::UiElementNotFound, format!("GetRootElement: {e}"))
                })
        }
    }
}

/// `UIA_ControlTypeIds` -> the short strings the parsers use.
fn control_type_name(id: i32) -> &'static str {
    match id {
        50000 => "Button",
        50004 => "Edit",
        50006 => "Image",
        50007 => "ListItem",
        50008 => "List",
        50020 => "Text",
        50023 => "Tree",
        50024 => "TreeItem",
        50026 => "Group",
        50030 => "Document",
        50032 => "Window",
        50033 => "Pane",
        50036 => "Table",
        _ => "Unknown",
    }
}

#[derive(Clone)]
pub struct UiaElement {
    pub raw: IUIAutomationElement,
}

impl UiaElement {
    fn prop_string(
        &self,
        prop: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
    ) -> Option<String> {
        unsafe {
            let v: VARIANT = self.raw.GetCurrentPropertyValue(prop).ok()?;
            let s = variant_to_string(&v);
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
    }

    pub fn control_type(&self) -> &'static str {
        unsafe {
            let id = self
                .raw
                .GetCurrentPropertyValue(UIA_ControlTypePropertyId)
                .ok()
                .map(|v| variant_to_i32(&v))
                .unwrap_or(0);
            control_type_name(id)
        }
    }

    pub fn name(&self) -> Option<String> {
        self.prop_string(UIA_NamePropertyId)
    }
    pub fn automation_id(&self) -> Option<String> {
        self.prop_string(UIA_AutomationIdPropertyId)
    }
    pub fn class_name(&self) -> Option<String> {
        self.prop_string(UIA_ClassNamePropertyId)
    }

    pub fn value(&self) -> Option<String> {
        unsafe {
            if let Ok(p) = self
                .raw
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            {
                if let Ok(b) = p.CurrentValue() {
                    let s = b.to_string();
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
        }
        self.prop_string(UIA_ValueValuePropertyId)
    }

    pub fn bounding_left(&self) -> Option<f64> {
        unsafe {
            self.raw
                .CurrentBoundingRectangle()
                .ok()
                .map(|r| r.left as f64)
        }
    }

    pub fn children(&self, uia: &Uia) -> Vec<UiaElement> {
        unsafe {
            let mut out = Vec::new();
            let Ok(walker) = uia.automation.RawViewWalker() else {
                return out;
            };
            let mut cur = walker.GetFirstChildElement(&self.raw).ok();
            while let Some(el) = cur {
                out.push(UiaElement { raw: el.clone() });
                cur = walker.GetNextSiblingElement(&el).ok();
            }
            out
        }
    }

    pub fn find_first(
        &self,
        uia: &Uia,
        max_visit: usize,
        pred: &dyn Fn(&UiaElement) -> bool,
    ) -> Option<UiaElement> {
        let mut budget = max_visit;
        self.find_first_inner(uia, &mut budget, pred)
    }

    fn find_first_inner(
        &self,
        uia: &Uia,
        budget: &mut usize,
        pred: &dyn Fn(&UiaElement) -> bool,
    ) -> Option<UiaElement> {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        if pred(self) {
            return Some(self.clone());
        }
        for c in self.children(uia) {
            if let Some(hit) = c.find_first_inner(uia, budget, pred) {
                return Some(hit);
            }
        }
        None
    }

    /// Snapshot the subtree into an owned `FixtureNode` (up to `max_depth`).
    /// The parsers then run on the snapshot, exactly as in the fixture tests.
    pub fn snapshot(&self, uia: &Uia, max_depth: u32) -> FixtureNode {
        let ct = self.control_type().to_string();
        let bounding_left = if ct == "Edit" || ct == "Document" {
            self.bounding_left()
        } else {
            None
        };
        let children = if max_depth == 0 {
            Vec::new()
        } else {
            self.children(uia)
                .iter()
                .map(|c| c.snapshot(uia, max_depth - 1))
                .collect()
        };
        FixtureNode {
            control_type: ct,
            name: self.name(),
            automation_id: self.automation_id(),
            value: self.value(),
            class_name: self.class_name(),
            bounding_left,
            children,
        }
    }

    pub fn invoke(&self) -> bool {
        unsafe {
            self.raw
                .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                .and_then(|p| p.Invoke())
                .is_ok()
        }
    }

    pub fn set_value(&self, text: &str) -> bool {
        unsafe {
            let bstr = BSTR::from(text);
            self.raw
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|p| p.SetValue(&bstr))
                .is_ok()
        }
    }

    pub fn process_id(&self) -> Option<u32> {
        unsafe {
            let hwnd = self.raw.CurrentNativeWindowHandle().ok()?;
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            (pid != 0).then_some(pid)
        }
    }
}

/// Windows of a process, found by walking the root's children.
pub fn windows_of(uia: &Uia, pid: u32) -> BridgeResult<Vec<UiaElement>> {
    let root = uia.root()?;
    Ok(root
        .children(uia)
        .into_iter()
        .filter(|w| w.control_type() == "Window" && w.process_id() == Some(pid))
        .collect())
}

// --- VARIANT helpers (windows-rs 0.58: `windows::core::VARIANT`) -----------

fn variant_to_string(v: &VARIANT) -> String {
    BSTR::try_from(v).map(|b| b.to_string()).unwrap_or_default()
}

fn variant_to_i32(v: &VARIANT) -> i32 {
    i32::try_from(v)
        .or_else(|_| i16::try_from(v).map(i32::from))
        .unwrap_or(0)
}
