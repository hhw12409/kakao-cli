---
name: adapter-contract
description: "kakao-cli의 공통부↔OS 어댑터 인터페이스 계약(v2.0.0)을 설계·유지한다. serve 모드 프레이밍(요청/응답/이벤트, watch·dedup 의미), one-shot 프레이밍(doctor·self-test), 인터페이스 함수(listRooms/openRoom/readRecent/sendText/healthCheck)의 입출력 shape, roomId 규칙, 에러 코드 enum, send 상태 머신, 명령/TUI 스펙, SQLite 스키마, 기술 스택·배포·대화형 전환 ADR을 정의할 때 사용. 계약 변경·어댑터 인터페이스 분쟁·필드 불일치·이벤트 셰이프 논의 시에도 사용."
---

# 어댑터 계약 설계 (v2.0.0)

kakao-cli는 OS 독립적인 공통부(Rust)와, 각 OS의 카카오톡 앱을 접근성 API로 조작하는 어댑터로
나뉜다. 두 어댑터(macOS/Windows)가 **동일하게 동작**하는 유일한 근거가 이 계약이다. 계약이
모호하면 두 플랫폼이 갈라지고, 그게 이 프로젝트 런타임 버그의 1순위 원인이다.

정본은 `docs/adapter-contract.md`. `crates/kakao-contract` 크레이트가 이를 Rust 타입으로 구현하고,
공통부와 Windows 브리지가 직접 의존한다(macOS 브리지는 Swift라 `Contract.swift` 에 미러).

## 두 전송로

| 전송로 | 호출 | 용도 |
|--------|------|------|
| **serve** (주) | `kakao-<os>-bridge serve` — 장수 프로세스, stdin/stdout 줄 단위 JSON | 대화형 채팅 TUI |
| **one-shot** (유지) | `kakao-<os>-bridge <method> <argsJson>` — 한 줄 응답 후 종료 | `doctor`(healthCheck), 셀프테스트 |

계약 문서는 serve를 주 전송로로 규정한다. one-shot 은 제거하지 않는다(테스트 커버리지·doctor).

## 계약이 답해야 하는 질문

1. 어댑터는 각 함수에서 **정확히 어떤 필드**를 반환하는가 (이름·타입·필수 여부)
2. 응답을 **감싸는가** (`{ rooms: [...] }` vs `[...]`)
3. 필드명 규칙은 **camelCase**(JSON) / **snake_case**(DB), 변환은 어디서
4. serve 요청/응답을 **어떻게 상관**시키는가 (`id`), 이벤트는 어떻게 구별하는가 (`event` 키)
5. **watch** 중 새 메시지를 누가 판별하는가 (브리지 소유), 유실·중복 허용치는
6. 실패를 **어떻게 표현**하는가 (에러 코드 enum, 닫힌 집합)
7. `send`의 상태 전이와 각 상태의 **트리거 조건**

각 답은 `docs/adapter-contract.md`에 예시 JSON과 함께 못박는다.

## 설계 원칙

**계약은 플랫폼 중립이다.** "macOS에서는", "Windows에서는" 이 계약 본문에 들어가면 실패다.

**필드명은 한 규칙으로 고정한다.** JSON=camelCase, DB=snake_case. 공통부가 어댑터 응답을
**런타임 검증**한다 (계약 위반 시 조용히 넘기지 않고 명확한 내부 오류).

**방 제목을 식별자로 쓰지 않는다.** 어댑터는 세션 내에서 같은 방을 다시 지목할 수 있는 `roomId` —
**불투명 문자열** — 을 반환한다. 공통부는 내부 구조를 해석하지 않는다.

**오발송 방지를 구조로 보장한다.** `sendText`는 `roomId` 하나만 받는다. 이름 해석(여러 후보)은
공통부 책임이고, 어댑터는 이미 확정된 방에만 보낸다.

**전송은 확인 전까지 성공이 아니다.** `sendText`가 `sent`를 반환하려면 어댑터가 카카오톡 UI에서
내 메시지 반영을 확인해야 한다. 확인 실패는 `failed`가 아니라 `unknown`이고, 공통부는 재전송하지 않는다.

**watch dedup은 브리지가 소유한다.** 브리지만 UI 구조(행 개수·행 정체성)를 본다. `Message` 에 seq를
얹는 대신 브리지가 "새로 추가된 행만" `message` 이벤트로 보낸다 → 공통부는 append만, dedup 없음.

## 인터페이스 함수 (계약의 뼈대)

```
listRooms() → { rooms: Room[] }
  Room = { roomId: string, title: string, memberCount: number|null,
           unreadCount: number, lastMessage: { text, at /* ISO8601 UTC */, sender }|null }
  // memberCount 는 방을 안 열고 못 읽으면 null. 패리티 규칙: 한쪽 null이면 다른 쪽도 null

openRoom(roomId) → {} | error
  // 대화 행 클릭이 필요할 수 있어 카카오톡 창을 잠깐 앞으로 올린다

readRecent(roomId, limit) → { messages: Message[] }   // 오래된 것 → 최신
  Message = { sender: string, text: string, at: string /* ISO8601 UTC */,
              outgoing: boolean, kind: "text" | "unsupported" }
  // 사진/파일/이모티콘 = kind:"unsupported", text:""

sendText(roomId, text) → { status: "sent"|"failed"|"unknown",
                           at: string|null /* sent일 때 */, error: ErrorCode|null }

healthCheck() → { kakaoRunning, accessibilityGranted, appVersion: string|null,
                  issues: { code: ErrorCode, recovery: string }[] }
```

## serve 프레이밍 (§5)

`kakao-<os>-bridge serve` 는 stdin 줄 단위 요청을 읽고, stdout 에 줄 단위로 응답(`id` 상관)과
비동기 이벤트를 쓴다. stdin EOF 시 종료.

```
요청  {"id":1,"method":"listRooms","params":{}}
       {"id":5,"method":"watch","params":{"roomId":"row:3"}}
       {"id":6,"method":"unwatch","params":{}}
       {"id":0,"method":"shutdown","params":{}}          // 응답 없이 즉시 종료
응답  {"id":1,"ok":true,"data":{...}}                     // openRoom/watch/unwatch 성공은 data:{}
       {"id":4,"ok":false,"error":"SEND_INPUT_FAILED"}
이벤트 {"event":"message","roomId":"row:3","message":{...}}   // watch 중 새로 추가된 메시지
       {"event":"roomClosed","roomId":"row:3"}               // 대화가 카카오톡에서 닫힘 (2회 감지 실패)
       {"event":"error","code":"KAKAO_WINDOW_NOT_VISIBLE"}   // 일시적 조건. 브리지는 재시도
```

- `params` 셰이프는 one-shot 의 `argsJson` 과 동일
- 이벤트는 `event` 키로 응답과 구별 (공통부 `ServeMessage::parse`)
- `watch` 는 이전 watch 대체. 브리지가 요청 시점 메시지 tail 을 baseline 으로 잡고 그 이후 추가분만 방출
- 폴링 캐던스 1.5초 기준. 폭주·가상화 창 밖 메시지는 일부 유실 가능 (개인 도구 수준 허용 — 계약에 명시)

## one-shot 프레이밍 (§1)

```
$ kakao-macos-bridge healthCheck '{}'
{"ok":true,"data":{...}}          또는   {"ok":false,"error":"<CODE>"}
```
처리된 결과의 종료 코드는 항상 0. 비정상 종료(코드≠0)는 공통부가 내부 오류로 승격.

## 에러 코드 enum (닫힌 집합)

`KAKAO_NOT_RUNNING`, `KAKAO_WINDOW_NOT_VISIBLE`, `ACCESSIBILITY_PERMISSION_DENIED`,
`APP_VERSION_UNSUPPORTED`, `ROOM_NOT_FOUND`, `UI_ELEMENT_NOT_FOUND`, `SEND_INPUT_FAILED`,
`SEND_VERIFY_TIMEOUT`(→ status `unknown`), `EMPTY_MESSAGE`, `MESSAGE_TOO_LONG`.

`ROOM_AMBIGUOUS`(동명 방)는 **어댑터 에러가 아니다.** 공통부가 `/switch` 이름 해석에서 처리한다.
새 실패 유형은 계약에 코드를 추가한 뒤에만 쓴다.

## send 상태 머신

```
       sendText 호출 (공통부가 pending 로그 생성)
            │
       ┌────┼─────────────┐
  입력·전송 실패      UI 확인 성공        확인 폴링 타임아웃
       │                  │                     │
   [failed]            [sent]               [unknown]
 (SEND_INPUT_FAILED)  (at 기록)      (SEND_VERIFY_TIMEOUT, 재전송 금지)
```

- 상태는 이 머신을 통해서만 바뀐다.
- **사전 검증 실패(빈 메시지 `EMPTY_MESSAGE`, 초과 `MESSAGE_TOO_LONG`)는 `pending` 에 들어가지 않는다.**
- `unknown` 은 종착 상태 — 자동 재시도 없음. 사용자에게 "전송 여부 확인 불가 — 카카오톡에서 직접 확인".
- TUI 맥락에서 확인은 **Enter** 다 (활성 방이 정해져 있고 직접 입력했으므로). 별도 프롬프트 없음.
- `send_log` 에 각 시도의 최종 상태 기록. `send_log.text` 는 로컬 감사용 (전송·텔레메트리 금지).

## 명령 / TUI 스펙 (`docs/command-spec.md`)

현재 표면은 둘뿐:

| 표면 | 설명 | 종료 코드 |
|------|------|-----------|
| `kakao-cli` (인자 없음) | 채팅 TUI. TTY 아니면 거부(1) | 0 정상, 1 TTY 아님, 3/4 첫 실행 게이트 |
| `kakao-cli doctor` | 환경 진단 (one-shot healthCheck) | 0 정상, 3 권한, 4 미실행 |

TUI 스펙: 키(문자+Enter 전송, Backspace, Esc 입력 비움, PgUp/PgDn 스크롤, Ctrl-C 종료, 숫자=오버레이 선택),
슬래시 명령(`/rooms` `/switch <이름|@별칭>` `/alias add|list|rm` `/help` `/quit`),
상태줄(`✓ 전송됨 HH:MM` / `✗ …` / `? 전송 확인 불가`), 방 선택 오버레이(**기본 선택값 없음**).

종료 코드는 계약에서 확정: 0/1/2/3/4/6/7/8/9.

## 프로세스 간 통신 (§6)

- 공통부↔브리지는 **서브프로세스 + 파이프로 확정**. macOS 브리지가 별도 Swift 실행 파일이라 crate
  링크는 선택지가 아니고, Windows도 대칭을 위해 서브프로세스로 통일 (ADR 0001).
- serve 모드는 이 서브프로세스를 **장수화**해 warm 한 접근성 컨텍스트를 유지한다.
- 브리지 실행 파일 위치: `KAKAO_CLI_BRIDGE_PATH` → `<exe_dir>/../libexec/kakao-cli/` → `<exe_dir>/`.
- 어댑터 stderr 는 진단용 (메시지 본문 금지).

## SQLite 스키마 (`docs/db-schema.sql`) — v2

- `rooms` — `room_id`(불투명), `title`, `member_count`, `unread_count`, `last_message_*`, `list_order`, `synced_at`
- `messages` — `room_id`, `sender`, `text`, `at`, `outgoing`, `kind`, `UNIQUE(room_id, at, sender, text)` (watch 이벤트 재적재 멱등)
- `aliases` — `name`('@' 없이), `room_query`, `created_at`. **로컬 전용**
- `send_log` — `room_id`, `title_at_send`, `text`, `status`, `error_code`, `created_at`, `resolved_at`
- `meta` — 스키마 버전 등

**FTS5 없음.** `messages_fts` + 트리거는 v1→v2 마이그레이션에서 DROP (search 명령 제거).
컬럼은 snake_case. 어댑터 JSON(camelCase) → DB 저장 시 공통부가 변환.

## ADR

- `docs/adr/0001-tech-stack.md` — 공통부 Rust / macOS Swift / Windows Rust(windows-rs). IPC=서브프로세스. **확정, 재평가 안 함.**
- `docs/adr/0002-distribution.md` — Homebrew tap(소스 빌드, ad-hoc 서명) + Scoop. 공증·유료 계정 미사용.
- `docs/adr/0003-interactive-tui.md` — 대화형 전환. serve 모드 브리지·1.5초 폴링·포커스 탈취 트레이드오프.

## 계약 유지 (변경 시)

`docs/adapter-contract.md` 가 있으면 전면 재작성하지 않는다:
- 파일 상단 변경 이력에 `버전 / 날짜 / 변경 / 사유` 추가, 바뀐 섹션만 수정
- `crates/kakao-contract/src/lib.rs` 의 `CONTRACT_VERSION` 과 타입을 함께 갱신
- 영향받는 필드·함수·이벤트를 명시하고 core-engineer, 양 어댑터 엔지니어, qa-inspector 에게 SendMessage
- 와이어 타입 변경이면 메이저 버전, 추가만이면 마이너
