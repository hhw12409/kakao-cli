---
name: cli-core-implementation
description: "kakao-cli 공통부(OS 독립 계층)를 구현한다. 대화형 채팅 TUI(ratatui: 트랜스크립트·입력줄·상태줄·방 선택 오버레이), 어댑터 워커 스레드(Job/UiEvent, AX I/O를 UI에서 분리), serve 모드 클라이언트(ServeAdapter)와 StreamAdapter/MockStreamAdapter, 슬래시 명령(/switch·/rooms·/alias) 파서, 방 이름 해석(resolve_in_list, 동명 방 자동 선택 없음)과 @별칭, SQLite 스크롤백 캐시, send 안전 정책(Enter=확인, send_in_room)과 pending→sent/failed/unknown 상태 머신, 첫 실행 온보딩, doctor 명령을 작업할 때 사용."
---

# 공통부 구현

kakao-cli 공통부는 OS에 독립적인 모든 것이다. 카카오톡 UI를 직접 건드리지 않는다 — 그건 어댑터가
한다. 공통부는 어댑터가 `docs/adapter-contract.md`(v2.0.0)대로 데이터를 준다고 보고, 그 위에
**대화형 채팅 경험**을 만든다.

정본: `docs/adapter-contract.md`(계약), `docs/command-spec.md`(TUI 스펙), `docs/db-schema.sql`(스키마 v2),
`docs/kakao-cli-design.md` / `docs/adr/0003-interactive-tui.md`(설계 의도).

스택: Rust. TUI = `ratatui` + `crossterm`, DB = `rusqlite`(bundled), CLI 파싱 = `clap`. 런타임 0.

## CLI 표면 (아주 작다)

```
kakao-cli            → crate::tui::run()   (인자 없음)
kakao-cli doctor     → commands::doctor()  (one-shot healthCheck)
```

`cli.rs` 의 `Command` 는 `Doctor` 하나, `Cli.command` 는 `Option<Command>`. `None` 이면 TUI.
그 밖의 서브명령은 없다 (`send`/`rooms`/`open`/`inbox`/`search`/`alias`/`cache` 는 0.2.0에서 제거됨).

## 스레드 모델

```
UI 스레드 (tui/mod.rs)        어댑터 워커 스레드 (tui/worker.rs)
  터미널 소유, 렌더·입력   ──Job(mpsc)──▶   Box<dyn StreamAdapter> + Connection 소유
  AX I/O에 절대 블록 안 됨  ◀─UiEvent(mpsc)─   요청 라운드트립 + watch 이벤트 폴링
```

- `Job`: `ListRooms | Switch{query,exact} | SwitchTo(Room) | Send(String) | AliasAdd/List/Remove | Quit`
- `UiEvent`: `Rooms | Switched{room,history} | Ambiguous(Vec<Room>) | SwitchFailed | Incoming(Message) | Sent | SendFailed | SendUnknown | Notice | Warn | Disconnected`
- 워커는 `jobs.try_recv()` → 처리 → `adapter.next_event(150ms)` 폴링을 반복. 이 짧은 타임아웃이 루프 페이싱도 겸한다.
- 워커가 `Connection` 을 소유하므로 UI 스레드는 DB를 만지지 않는다. `rusqlite::Connection` 은 Send.

## 어댑터 계층

- `trait StreamAdapter` (`&mut self`): `list_rooms / open_room / read_recent / send_text / watch / unwatch / health_check / next_event(timeout) / shutdown`.
- `ServeAdapter` — `kakao-<os>-bridge serve` 를 spawn. 리더 스레드가 stdout 라인을 `ServeMessage` 로 파싱해 채널에 넣고, `request()` 가 `id` 로 응답을 상관시키며 그 사이 이벤트는 버퍼링.
- `MockStreamAdapter` (`KAKAO_CLI_STREAM_MOCK=<fixture>`) — 카카오톡 없이 TUI·테스트 구동. fixture 에 `rooms`/`history`/`incoming`(스크립트된 수신)/`send_text`(강제 결과).
- `trait Adapter`(one-shot) + `SubprocessAdapter` + `MockAdapter` 는 **`doctor` 전용으로 유지**.
- **어댑터 응답을 런타임 검증한다** (`serde_json::from_value` 실패 → `어댑터 계약 위반: …` 내부 오류).
- serve 요청/응답 셰이프는 `crates/kakao-contract` 의 `ServeRequest`/`ServeResponse`/`ServeEvent`.

## 채팅 TUI (`tui/`)

| 파일 | 책임 |
|------|------|
| `mod.rs` | crossterm raw mode + alt screen, 워커 spawn, 이벤트 루프, 첫 실행 온보딩 게이트, 정리 |
| `app.rs` | `App` 상태 + `apply(UiEvent)` 폴드. **렌더·I/O 없음** — `tests/tui_smoke.rs` 가 헤드리스 구동 |
| `ui.rs` | ratatui 렌더: 제목바 / 트랜스크립트(스크롤) / 상태줄 / 입력 박스 / 방 선택 오버레이 |
| `input.rs` | 키 → `Action` (`None`/`Quit`/`Job`), 슬래시 명령 파서 |
| `worker.rs` | `Job` 처리 + watch 이벤트 → `UiEvent`. `do_switch` = unwatch → open → readRecent → insert → watch |

- **첫 실행 게이트**: raw mode 진입 전 `adapter.health_check()`. 카카오톡 미실행/권한 없음이면
  스택트레이스 대신 `doctor` 수준 안내(`AppError::Onboarding`)로 종료.
- 트랜스크립트: `Line::Msg{at,who,body,outgoing}` / `Line::System(String)`. 시각은 로컬 타임존 표시,
  저장·계약은 UTC ISO 8601. `outgoing` 이면 "나".
- 스크롤: `scrollback` = 바닥에서 위로 올린 행 수. 새 메시지·시스템 라인은 `follow()` 로 바닥 복귀.

## 슬래시 명령

| 입력 | Job / 동작 |
|------|-----------|
| `/rooms`, `/r` | `Job::ListRooms` |
| `/switch <이름\|@별칭>`, `/s …` | `Job::Switch{query, exact:false}` |
| `/alias add <이름> <검색어>` / `/alias list` / `/alias rm <이름>` | `db::alias_*` |
| `/help`, `/?` | `App::help_text()` 를 트랜스크립트에 |
| `/quit`, `/q`, `/exit` | `Action::Quit` |
| 슬래시로 시작 안 함 + Enter | `Job::Send(line)` |

## 방 이름 해석 (`resolve_in_list`)

`/switch` 는 워커가 가져온 방 목록(`Vec<Room>`)에 대해 `resolve::resolve_in_list(rooms, conn, query, exact)`
→ `Resolution`:

- `One(room)` → 워커가 `do_switch`
- `Many(candidates)` → `UiEvent::Ambiguous` → TUI가 **번호 선택 오버레이**. **기본 선택값 없음.**
  `App::pick(n)` 이 `Job::SwitchTo(room)` 반환. 자동 선택 절대 금지 (오배송 방지)
- `None{query, near}` → "일치하는 방 없음" + 가까운 이름 힌트

`@별칭` 은 `expand_alias` 로 먼저 치환한 뒤 해석. 별칭이 가리키는 이름이 없거나 여러 방과 겹치면
일반 동명 방 UX로 폴백. 별칭은 절대 서버/텔레메트리로 나가지 않는다.

## send 안전 정책 (`send::send_in_room`)

`send_in_room(adapter, conn, room_id, room_title, body, max_chars) -> SendOutcome`:

1. `validate_body` — 빈 메시지(`EMPTY_MESSAGE`) / 초과(`MESSAGE_TOO_LONG`)는 `Err` 반환, **`pending` 미진입**.
2. `db::send_log_pending` — `pending` 행 기록.
3. `adapter.send_text` → `SendResult` → `db::send_log_resolve` 로 `pending → sent|failed|unknown`.
4. 상태는 이 경로로만 바뀐다. 코드 곳곳에서 `send_log.status` 를 직접 쓰지 않는다.

- **확인 = Enter.** 활성 방이 이미 정해져 있고 사용자가 직접 입력했으므로. `--dry-run`/`--yes`/`--stdin`/
  편집기 모드/`--max-chars` 같은 플래그는 없다 (0.1.0에서 제거).
- 보낸 메시지 텍스트는 **로컬 에코하지 않는다.** 카카오톡이 확인해 watch 폴링에 잡히면(1~2초)
  `Incoming` 이벤트로 트랜스크립트에 나타난다. 즉시 피드백은 상태줄(`✓ 전송됨 HH:MM`).
- `unknown`(SEND_VERIFY_TIMEOUT): 상태줄에 "전송 확인 불가", 트랜스크립트에 안내. **재전송 안 함.**
- `failed`: 원인 에러 코드 → 사용자 메시지.

## SQLite 스크롤백 캐시 (DB v2)

- `docs/db-schema.sql` DDL. `db::open()` 이 `migrate` 로 v1→v2 시 FTS 객체 DROP.
- 워커가 받은 모든 `message` 이벤트를 `db::insert_messages`(`INSERT OR IGNORE`, `UNIQUE(room_id,at,sender,text)` 로 멱등)로 적재 → 스크롤백·감사.
- `do_switch` 실패 시 `db::recent_messages(conn, room_id, 40)` 로 캐시에서 트랜스크립트 시드.
- **FTS5·`search`·`cache clear` 없음.** 별칭·`send_log` 는 유지.

## 첫 실행 온보딩

접근성 권한 미부여, 브리지 미발견, 카카오톡 미실행 등은 스택트레이스가 아니라 `doctor` 수준의
안내(무엇이/왜/복사할 명령·설정 경로)로 출력한다. `tui::run` 의 게이트와 `commands::onboarding_from_internal`
이 담당. 처음 쓰는 사람이 이 출력만 보고 다음 행동을 할 수 있어야 한다.

## 개인정보 / 창 비방해

- 방 이름·메시지·별칭·전송 로그는 로컬 SQLite에만. 로그·stderr·텔레메트리에 **메시지 본문 금지**
  (`send_log.text` 는 로컬 DB 한정 예외).
- 정상 상태 watch 폴링은 카카오톡 창을 앞으로 가져오지 않는다(어댑터 몫이지만 공통부도 불필요한
  `openRoom` 재호출을 피한다 — 대화가 열려 있으면 watch 만).

## 테스트

- `tests/tui_smoke.rs` — `MockStreamAdapter` + 워커를 헤드리스로 구동: `/rooms` → `/switch` → 수신 → 전송.
  **동명 방 자동 선택 안 함**을 assert.
- `tests/core_behaviour.rs` — `resolve_in_list` 의 `Resolution` 분기, `send_in_room` 상태 머신,
  `db::recent_messages`, 별칭 충돌.
- `KAKAO_CLI_STREAM_MOCK=crates/kakao-core/tests/fixtures/chat.json cargo run -p kakao-core` — 실제 바이너리 수동 확인.
- PTY 테스트 시 `TIOCSWINSZ` 로 창 크기 설정 필수(0x0이면 ratatui가 빈 화면).

## 이전 소스가 있을 때

전체 재작성하지 않는다. 사용자 피드백·QA 지적에 해당하는 모듈만 수정. 계약이 새 버전이면
`crates/kakao-contract` diff 확인 후 영향받는 파싱·매핑·이벤트 처리부만 갱신.
