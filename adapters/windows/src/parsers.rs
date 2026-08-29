//! Pure UIA-tree -> contract-type parsers. Operate on `dyn Node`, so the same
//! code runs against the live UIA tree and JSON fixtures in tests.
//!
//! The logic mirrors the macOS adapter's `Parsers` so the two produce
//! byte-identical output for the same fixture scenario:
//!  - room title / member count (groups only, else 2) / unread badge / preview
//!  - message body + carried-forward sender + carried-forward Korean timestamp
//!  - outgoing = right-aligned bubble (bounding_left vs window left)

use kakao_contract::{LastMessage, Message, MessageKind, Room};
use time::OffsetDateTime;

use crate::korean_time;
use crate::node::{any_text, descendants, first_descendant, Node};
use crate::selectors::SelectorMap;

// --------------------------------------------------------------------------
// listRooms
// --------------------------------------------------------------------------

pub fn rooms(root: &dyn Node, sel: &SelectorMap, now: OffsetDateTime) -> Vec<Room> {
    let Some(list) = first_descendant(root, &|n| n.control_type() == sel.room_list_control_type)
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let items: Vec<&dyn Node> = list
        .children()
        .into_iter()
        .filter(|n| n.control_type() == sel.room_item_control_type)
        .collect();

    for (idx, item) in items.iter().enumerate() {
        let Some(title) = field_text(*item, sel.room_title_automation_id)
            .or_else(|| first_static_non_badge(*item))
        else {
            continue; // spacer / non-room row
        };
        if title.is_empty() {
            continue;
        }

        let preview = field_text(*item, sel.room_preview_automation_id).unwrap_or_default();
        let ts_label = field_text(*item, sel.room_timestamp_automation_id).unwrap_or_default();
        let at = korean_time::to_iso(&ts_label, now);

        out.push(Room {
            room_id: format!("row:{idx}"),
            title,
            member_count: member_count(*item, sel),
            unread_count: unread_badge(*item, sel),
            last_message: if preview.is_empty() {
                None
            } else {
                Some(LastMessage {
                    text: preview,
                    at,
                    sender: String::new(),
                })
            },
        });
    }
    out
}

fn field_text(item: &dyn Node, automation_id: &str) -> Option<String> {
    first_descendant(item, &|n| n.automation_id() == Some(automation_id)).and_then(any_text)
}

fn first_static_non_badge(item: &dyn Node) -> Option<String> {
    let mut texts = Vec::new();
    descendants(item, &|n| n.control_type() == "Text", &mut texts);
    texts
        .into_iter()
        .filter_map(any_text)
        .find(|t| !is_badge_number(t))
}

/// `memberCount` element present -> that number. Absent -> 2 (a 1:1 chat).
fn member_count(item: &dyn Node, sel: &SelectorMap) -> Option<u32> {
    if let Some(raw) = field_text(item, sel.room_member_count_automation_id) {
        let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u32>() {
            return Some(n);
        }
    }
    Some(2)
}

/// Unread badge = a bare numeric Text with no automation id. Absent -> 0.
fn unread_badge(item: &dyn Node, _sel: &SelectorMap) -> u32 {
    let mut texts = Vec::new();
    descendants(item, &|n| n.control_type() == "Text", &mut texts);
    for n in texts {
        if n.automation_id().is_some() {
            continue;
        }
        if let Some(t) = any_text(n) {
            if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(v) = t.parse() {
                    return v;
                }
            }
        }
    }
    0
}

fn is_badge_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

// --------------------------------------------------------------------------
// readRecent
// --------------------------------------------------------------------------

pub fn messages(
    conversation: &dyn Node,
    sel: &SelectorMap,
    window_left: Option<f64>,
    now: OffsetDateTime,
) -> Vec<Message> {
    let Some(list) = first_descendant(conversation, &|n| {
        n.control_type() == sel.message_list_control_type
    }) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut current_at = String::new();
    let mut current_sender = String::new();

    for item in list.children() {
        if item.control_type() != sel.message_item_control_type {
            continue;
        }
        let mut statics = Vec::new();
        descendants(item, &|n| n.control_type() == "Text", &mut statics);
        let static_texts: Vec<String> = statics.iter().filter_map(|n| any_text(*n)).collect();

        let has_profile = {
            let mut btns = Vec::new();
            descendants(item, &|n| n.control_type() == "Button", &mut btns);
            btns.iter()
                .any(|n| n.name() == Some("프로필") || n.automation_id() == Some("profile"))
        };

        // Timestamp: the static text whose last line is a clock.
        if let Some(ts) = static_texts
            .iter()
            .find(|t| korean_time::parse_hour_minute(t).is_some())
        {
            let iso = korean_time::to_iso(ts, now);
            if !iso.is_empty() {
                current_at = iso;
            }
        }
        // New sender run: profile marker + a non-clock static text.
        if has_profile {
            if let Some(name) = static_texts
                .iter()
                .find(|t| korean_time::parse_hour_minute(t).is_none())
            {
                current_sender = name.clone();
            }
        }

        // The message body is an Edit/Document control (labels are Text). If a
        // real dump shows KakaoTalk Windows using Text for message bodies, add
        // that control type here (and tighten media detection accordingly).
        let body_node = first_descendant(item, &|n| {
            n.control_type() == "Edit" || n.control_type() == "Document"
        });
        let body = body_node.and_then(any_text);
        let bubble_left = body_node.and_then(|n| n.bounding_left());

        if let Some(text) = body.filter(|b| !b.is_empty()) {
            let outgoing = is_outgoing(bubble_left, window_left, &current_sender);
            out.push(Message {
                sender: if outgoing {
                    String::new()
                } else {
                    current_sender.clone()
                },
                text,
                at: current_at.clone(),
                outgoing,
                kind: MessageKind::Text,
            });
        } else if is_media_item(item) {
            out.push(Message {
                sender: current_sender.clone(),
                text: String::new(),
                at: current_at.clone(),
                outgoing: false,
                kind: MessageKind::Unsupported,
            });
        }
    }
    out
}

fn is_media_item(item: &dyn Node) -> bool {
    let mut imgs = Vec::new();
    descendants(item, &|n| n.control_type() == "Image", &mut imgs);
    !imgs.is_empty()
}

/// Outgoing = the bubble is right-aligned. Incoming bubbles sit near the window
/// left edge; outgoing ones are pushed well right. Matches the macOS threshold.
pub fn is_outgoing(bubble_left: Option<f64>, window_left: Option<f64>, sender: &str) -> bool {
    if let (Some(b), Some(w)) = (bubble_left, window_left) {
        return (b - w) > 120.0;
    }
    // No geometry: an empty sender (no profile/name ever seen) is likely ours.
    sender.is_empty()
}
