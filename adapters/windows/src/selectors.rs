//! Version-specific UI Automation selectors for KakaoTalk Windows. Kept in one
//! map keyed by app version, mirroring the macOS adapter's `SelectorMap`.
//!
//! The values below are PLACEHOLDERS. KakaoTalk Windows renders much of its UI
//! with a custom/Chromium-ish layer, so the real `ControlType` /
//! `AutomationId` / `ClassName` strings must be captured with
//! `kakao-windows-bridge --dump-tree` (or Inspect.exe / Accessibility Insights)
//! against a real build before this adapter works live.

#[derive(Debug, Clone)]
pub struct SelectorMap {
    /// `ClassName` (or `Name`) of the main KakaoTalk window.
    pub main_window_class: &'static str,
    pub main_window_name: &'static str,
    /// `ControlType` of the chat-list container.
    pub room_list_control_type: &'static str,
    /// `ControlType` of a single chat-list row.
    pub room_item_control_type: &'static str,
    /// `AutomationId` of the title text within a room item (else match by position).
    pub room_title_automation_id: &'static str,
    /// `AutomationId` of the member-count text (groups only).
    pub room_member_count_automation_id: &'static str,
    /// `AutomationId` of the timestamp text within a room item.
    pub room_timestamp_automation_id: &'static str,
    /// `AutomationId` of the last-message preview within a room item.
    pub room_preview_automation_id: &'static str,

    /// `Name`/title of a conversation window (usually the room name, like macOS).
    /// The message area within it:
    pub message_list_control_type: &'static str,
    pub message_item_control_type: &'static str,
    /// `ControlType`/`AutomationId` of the compose edit box.
    pub compose_control_type: &'static str,
    pub compose_automation_id: &'static str,
    /// `Name` of the send button.
    pub send_button_name: &'static str,
}

pub const V_PLACEHOLDER: SelectorMap = SelectorMap {
    main_window_class: "EVA_Window_Dblclk",
    main_window_name: "카카오톡",
    room_list_control_type: "List",
    room_item_control_type: "ListItem",
    room_title_automation_id: "roomTitle",
    room_member_count_automation_id: "memberCount",
    room_timestamp_automation_id: "roomTimestamp",
    room_preview_automation_id: "roomPreview",
    message_list_control_type: "List",
    message_item_control_type: "ListItem",
    compose_control_type: "Edit",
    compose_automation_id: "messageInput",
    send_button_name: "전송",
};

/// Look up by KakaoTalk's file version. Falls back to the placeholder map.
///
/// TODO: key by `_version` once a real KakaoTalk Windows build is characterised
/// with `--dump-tree` (mirrors `SelectorMap.forVersion` on macOS).
pub fn for_version(_version: Option<&str>) -> Option<&'static SelectorMap> {
    Some(&V_PLACEHOLDER)
}
