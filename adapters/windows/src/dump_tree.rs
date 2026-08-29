//! `kakao-windows-bridge --dump-tree` — serialize KakaoTalk's UIA tree to
//! `FixtureNode` JSON on stdout, with a per-window summary on stderr. Used to
//! capture per-version fixtures. Not part of the contract.

#![cfg(windows)]

use crate::kakao_app;
use crate::node::FixtureNode;
use crate::uia::{self, Uia};

pub fn run() -> ! {
    let Some(running) = kakao_app::running() else {
        eprintln!("KakaoTalk is not running");
        std::process::exit(1);
    };
    let uia = match Uia::new() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("UIA init failed: {:?}", e.code);
            std::process::exit(1);
        }
    };
    let ws = uia::windows_of(&uia, running.pid).unwrap_or_default();

    let mut summary = format!(
        "KakaoTalk {} — {} window(s):\n",
        kakao_app::version(&running.exe_path).unwrap_or_else(|| "?".into()),
        ws.len()
    );
    let mut dumped = Vec::new();
    for (i, w) in ws.iter().enumerate() {
        summary.push_str(&format!(
            "  [{i}] class={:?} name={:?}\n",
            w.class_name().unwrap_or_default(),
            w.name().unwrap_or_default()
        ));
        dumped.push(w.snapshot(&uia, 60));
    }
    eprint!("{summary}");

    let root = FixtureNode {
        control_type: "Pane".into(),
        name: Some("root".into()),
        automation_id: None,
        value: None,
        class_name: None,
        bounding_left: None,
        children: dumped,
    };
    match serde_json::to_string_pretty(&root) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("serialize failed: {e}"),
    }
    std::process::exit(0)
}
