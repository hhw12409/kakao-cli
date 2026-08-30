//! Key handling and the slash-command parser. Pure: takes a key + `&mut App`,
//! returns an [`Action`] for the event loop to carry out.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, Screen};
use super::worker::Job;

#[derive(Debug)]
pub enum Action {
    /// Redraw; nothing else.
    None,
    /// Tear down and exit.
    Quit,
    /// Hand this job to the worker.
    Job(Job),
}

pub fn handle(app: &mut App, key: KeyEvent) -> Action {
    // Ctrl-C always quits.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    // Disambiguation overlay: digits pick, Esc cancels. Takes precedence over
    // whichever screen is behind it.
    if app.picker.is_some() {
        return match key.code {
            KeyCode::Esc => {
                app.picker = None;
                app.push_system("선택을 취소했습니다.");
                Action::None
            }
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                match app.pick(c.to_digit(10).unwrap() as usize) {
                    Some(job) => Action::Job(job),
                    None => {
                        app.push_system("그 번호의 방이 없습니다.");
                        Action::None
                    }
                }
            }
            _ => Action::None,
        };
    }

    match app.screen {
        Screen::Rooms => handle_rooms(app, key),
        Screen::Chat => handle_chat(app, key),
    }
}

/// Room-list screen: arrow keys move the highlight, typing filters, `Enter`
/// opens the highlighted room.
fn handle_rooms(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Up => {
            app.move_selection(-1);
            Action::None
        }
        KeyCode::Down => {
            app.move_selection(1);
            Action::None
        }
        KeyCode::Enter => match app.enter_selected() {
            Some(job) => Action::Job(job),
            None => Action::None,
        },
        KeyCode::Esc => {
            // Esc backs out one level: clear the filter, or quit from the
            // top of the room list.
            if app.rooms_filter.is_empty() {
                Action::Quit
            } else {
                app.edit_filter(String::clear);
                Action::None
            }
        }
        KeyCode::Backspace => {
            app.edit_filter(|f| {
                f.pop();
            });
            Action::None
        }
        KeyCode::Char(c) => {
            app.edit_filter(|f| f.push(c));
            Action::None
        }
        _ => Action::None,
    }
}

/// Chat screen: type to send, slash commands, `Esc` back to the room list.
fn handle_chat(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            if !app.input.is_empty() {
                app.input.clear();
                Action::None
            } else {
                app.open_room_list();
                Action::Job(Job::ListRooms)
            }
        }
        KeyCode::PageUp => {
            app.scrollback = app.scrollback.saturating_add(10);
            Action::None
        }
        KeyCode::PageDown => {
            app.scrollback = app.scrollback.saturating_sub(10);
            Action::None
        }
        KeyCode::Backspace => {
            app.input.pop();
            Action::None
        }
        KeyCode::Char(c) => {
            app.input.push(c);
            Action::None
        }
        KeyCode::Enter => {
            let line = std::mem::take(&mut app.input);
            let line = line.trim();
            if line.is_empty() {
                Action::None
            } else if let Some(cmd) = line.strip_prefix('/') {
                parse_command(app, cmd)
            } else {
                Action::Job(Job::Send(line.to_string()))
            }
        }
        _ => Action::None,
    }
}

fn parse_command(app: &mut App, cmd: &str) -> Action {
    let mut parts = cmd.split_whitespace();
    let head = parts.next().unwrap_or("");
    let rest = cmd[head.len()..].trim();

    match head {
        "quit" | "q" | "exit" => Action::Quit,
        "help" | "?" => {
            app.push_system(App::help_text());
            Action::None
        }
        "rooms" | "r" => {
            app.open_room_list();
            Action::Job(Job::ListRooms)
        }
        "switch" | "s" => {
            if rest.is_empty() {
                app.push_system("사용법: /switch <방 이름 또는 @별칭>");
                Action::None
            } else {
                Action::Job(Job::Switch {
                    query: rest.to_string(),
                    exact: false,
                })
            }
        }
        "alias" => {
            let mut a = rest.split_whitespace();
            match a.next() {
                Some("add") => {
                    let name = a.next().unwrap_or("");
                    let query: String = a.collect::<Vec<_>>().join(" ");
                    if name.is_empty() || query.is_empty() {
                        app.push_system("사용법: /alias add <이름> <검색어>");
                        Action::None
                    } else {
                        Action::Job(Job::AliasAdd {
                            name: name.to_string(),
                            query,
                        })
                    }
                }
                Some("list") | None => Action::Job(Job::AliasList),
                Some("rm") | Some("remove") => {
                    let name = a.next().unwrap_or("");
                    if name.is_empty() {
                        app.push_system("사용법: /alias rm <이름>");
                        Action::None
                    } else {
                        Action::Job(Job::AliasRemove(name.to_string()))
                    }
                }
                Some(other) => {
                    app.push_system(format!("알 수 없는 alias 하위 명령: {other}"));
                    Action::None
                }
            }
        }
        other => {
            app.push_system(format!("알 수 없는 명령: /{other}  (/help 참고)"));
            Action::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::Screen;
    use crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn ctrl_c_quits_from_either_screen() {
        let mut app = App::new();
        let c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(matches!(handle(&mut app, c), Action::Quit));
        app.screen = Screen::Chat;
        assert!(matches!(handle(&mut app, c), Action::Quit));
    }

    #[test]
    fn room_list_arrows_move_and_enter_opens() {
        let mut app = App::new();
        app.rooms = vec![room("row:0", "가족"), room("row:1", "개발팀")];
        app.rooms_loading = false;

        assert!(matches!(handle(&mut app, key(KeyCode::Down)), Action::None));
        assert_eq!(app.rooms_selected, 1);
        match handle(&mut app, key(KeyCode::Enter)) {
            Action::Job(Job::SwitchTo(r)) => assert_eq!(r.title, "개발팀"),
            other => panic!("expected SwitchTo, got {other:?}"),
        }
    }

    #[test]
    fn room_list_esc_clears_filter_then_quits() {
        let mut app = App::new();
        app.rooms = vec![room("row:0", "가족")];
        app.rooms_loading = false;
        app.edit_filter(|f| f.push_str("가"));

        // First Esc: drop the filter.
        assert!(matches!(handle(&mut app, key(KeyCode::Esc)), Action::None));
        assert!(app.rooms_filter.is_empty());
        // Second Esc on the bare list: quit.
        assert!(matches!(handle(&mut app, key(KeyCode::Esc)), Action::Quit));
    }

    #[test]
    fn chat_esc_returns_to_room_list() {
        let mut app = App::new();
        app.screen = Screen::Chat;
        app.room_title = Some("가족".into());

        // Esc with pending input just clears the input.
        app.input.push_str("draft");
        assert!(matches!(handle(&mut app, key(KeyCode::Esc)), Action::None));
        assert!(app.input.is_empty());
        assert_eq!(app.screen, Screen::Chat);

        // Esc on an empty prompt leaves the room.
        match handle(&mut app, key(KeyCode::Esc)) {
            Action::Job(Job::ListRooms) => {}
            other => panic!("expected ListRooms, got {other:?}"),
        }
        assert_eq!(app.screen, Screen::Rooms);
    }

    fn room(id: &str, title: &str) -> kakao_contract::Room {
        kakao_contract::Room {
            room_id: id.into(),
            title: title.into(),
            member_count: None,
            unread_count: 0,
            last_message: None,
        }
    }
}
