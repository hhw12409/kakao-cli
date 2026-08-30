//! Key handling and the slash-command parser. Pure: takes a key + `&mut App`,
//! returns an [`Action`] for the event loop to carry out.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::App;
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

    // Disambiguation overlay: digits pick, Esc cancels.
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

    match key.code {
        KeyCode::Esc => {
            app.input.clear();
            Action::None
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
        "rooms" | "r" => Action::Job(Job::ListRooms),
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
