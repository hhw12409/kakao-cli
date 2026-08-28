---
name: adapter-contract
description: "kakao-cli의 공통부↔OS 어댑터 인터페이스 계약을 설계·유지한다. 5개 인터페이스 함수(listRooms/openRoom/readRecent/sendText/healthCheck)의 입출력 shape, 필드명 규칙, roomId 규칙, 에러 코드 enum, 프로세스 간 통신 형식, send 상태 머신, 명령어 스펙, SQLite 스키마, 기술 스택 ADR을 정의할 때 사용. 계약 변경·어댑터 인터페이스 분쟁·필드 불일치 논의 시에도 사용."
---

# 어댑터 계약 설계

kakao-cli는 OS에 독립적인 공통부와, 각 OS의 카카오톡 앱을 접근성 API로 조작하는 어댑터로 나뉜다. 두 어댑터(macOS/Windows)가 **동일하게 동작**하는 유일한 근거가 이 계약이다. 계약이 모호하면 두 플랫폼이 갈라지고, 그게 이 프로젝트 런타임 버그의 1순위 원인이다.

## 계약이 답해야 하는 질문

1. 어댑터는 각 함수에서 **정확히 어떤 필드**를 반환하는가 (이름·타입·필수 여부)
2. 응답을 **감싸는가** (`{ rooms: [...] }` vs `[...]`)
3. 필드명 규칙은 **camelCase인가 snake_case인가**, 변환은 어디서 일어나는가
4. 공통부와 어댑터는 **어떻게 통신**하는가 (프로세스, 직렬화 형식)
5. 실패를 **어떻게 표현**하는가 (에러 코드 enum, 예외 vs 결과값)
6. `send`의 상태 전이는 무엇이며 각 상태의 **트리거 조건**은 무엇인가

각 답은 `docs/adapter-contract.md`에 예시 JSON과 함께 못박는다.

## 설계 원칙

**계약은 플랫폼 중립이다.** "macOS에서는", "Windows에서는" 이 계약 본문에 들어가면 실패다. 두 어댑터가 똑같이 만족시킬 수 있는 것만 계약이 된다. 어댑터별 구현 방법은 계약이 아니라 어댑터 코드의 관심사다.

**필드명은 한 규칙으로 고정한다.** JSON 경계에서는 camelCase, DB 컬럼은 snake_case. 변환 지점(어댑터→공통부 파싱, 공통부→DB)을 계약에 명시한다. TypeScript 제네릭이나 `any` 캐스팅으로 shape 불일치가 컴파일러를 통과하는 상황을 막으려면, 공통부가 어댑터 응답을 **런타임 검증**하도록 계약에 요구한다.

**방 제목을 식별자로 쓰지 않는다.** 제목은 바뀌고 중복된다. 어댑터는 세션 내에서 같은 방을 다시 지목할 수 있는 `roomId` — **불투명 문자열** — 을 반환해야 한다. 공통부는 이 문자열의 내부 구조를 해석하면 안 된다. macOS가 경로 해시를 쓰든 Windows가 인덱스를 쓰든, 계약상 타입은 `string`이고 공통부에게는 불투명하다.

**오발송 방지를 구조로 보장한다.** 계약이 "정확히 하나 일치 시에만 자동 전송"을 강제하도록: `sendText`는 `roomId` 하나만 받는다. 이름 해석(여러 후보 처리)은 공통부의 책임이고, 어댑터는 이미 확정된 방에만 보낸다. 어댑터에 이름을 넘겨 "알아서 찾아 보내라"고 하면 안 된다.

**전송은 확인 전까지 성공이 아니다.** `sendText`가 `sent`를 반환하려면 어댑터가 카카오톡 UI에서 내 메시지 반영을 확인해야 한다. 확인 실패는 `failed`가 아니라 `unknown`이며, 공통부는 `unknown`에 대해 재전송하지 않는다.

## 인터페이스 함수 (계약의 뼈대)

`docs/kakao-cli-design.md`의 5개 함수를 다음 수준으로 구체화한다.

```
listRooms() → { rooms: Room[] }
  Room = {
    roomId: string,        // 불투명, 세션 내 안정적
    title: string,
    memberCount: number,   // 1:1이면 2
    unreadCount: number,
    lastMessage: { text: string, at: string /* ISO 8601 UTC */, sender: string } | null
  }

openRoom(roomId: string) → { ok: true } | { ok: false, error: ErrorCode }
  // 기본적으로 카카오톡 창을 전면으로 가져오지 않는다

readRecent(roomId: string, limit: number) → { messages: Message[] }
  Message = {
    sender: string,
    text: string,
    at: string,            // ISO 8601 UTC
    outgoing: boolean       // 내가 보낸 메시지인지
  }
  // 텍스트 메시지만. 사진/파일/이모티콘/특수 메시지는 { text: "", kind: "unsupported" } 또는 스킵 — 계약에서 택1하고 고정

sendText(roomId: string, text: string) → SendResult
  SendResult = {
    status: "sent" | "failed" | "unknown",
    at: string | null,     // sent일 때 전송 확인 시각 (ISO 8601 UTC)
    error: ErrorCode | null
  }

healthCheck() → {
    kakaoRunning: boolean,
    accessibilityGranted: boolean,   // macOS=TCC, Windows=UIA 접근 가능
    appVersion: string | null,
    issues: { code: ErrorCode, recovery: string }[]   // recovery는 사용자가 실행할 안내
  }
```

위는 출발점이다. 어댑터 엔지니어의 "이 필드는 접근성 트리에서 안정적으로 못 얻는다" 피드백을 받아 조정한다. 조정 시 계약 버전을 올리고 **양쪽 어댑터 엔지니어 모두에게** 브로드캐스트한다.

## 에러 코드 enum

계약에 닫힌 목록으로 정의한다. 어댑터는 이 목록 밖의 문자열을 반환하지 않는다. 새 실패 유형은 계약에 코드를 추가한 뒤에만 쓴다.

출발 목록:
`KAKAO_NOT_RUNNING`, `ACCESSIBILITY_PERMISSION_DENIED`, `APP_VERSION_UNSUPPORTED`, `ROOM_NOT_FOUND`, `UI_ELEMENT_NOT_FOUND`(UI 변경 의심), `SEND_INPUT_FAILED`, `SEND_VERIFY_TIMEOUT`(→ status `unknown`), `EMPTY_MESSAGE`, `MESSAGE_TOO_LONG`.

각 코드에 대해: 어떤 함수가 낼 수 있는가 / 공통부의 처리 / 사용자에게 보일 복구 안내의 출처(`docs/errors.md`, docs-writer 작성).

`ROOM_AMBIGUOUS`(동명 방 다수)는 **어댑터 에러가 아니다.** 공통부가 이름 해석 단계에서 처리한다.

## send 상태 머신

```
       sendText 호출
            │
        [pending]
       ┌────┼─────────────┐
  입력·전송 실패      전송 후 UI 확인      확인 폴링 타임아웃
       │                  │                     │
   [failed]            [sent]               [unknown]
 (SEND_INPUT_FAILED)  (at 기록)      (SEND_VERIFY_TIMEOUT, 재전송 금지)
```

- 상태는 이 머신을 통해서만 바뀐다. 코드 곳곳에서 임의로 `status`를 세팅하지 않는다.
- `--dry-run`은 `pending`에도 진입하지 않는다. 대상·메시지만 출력하고 `sendText`를 호출하지 않는다.
- `unknown`은 사용자에게 "전송 여부를 확인할 수 없음 — 카카오톡에서 직접 확인하세요"로 표시하고, 자동 재시도하지 않는다.
- 전송 로그 테이블에 각 시도의 최종 상태를 기록 (메시지 본문은 저장하되 로컬 SQLite에만, 텔레메트리 금지).

## 명령어 스펙 (`docs/command-spec.md`)

각 명령에 대해 표로: 인자 / 플래그 / 출력 형식(정확한 예시) / 종료 코드.

| 명령 | 핵심 플래그 | 종료 코드 규칙 |
|------|------------|---------------|
| `inbox` | — | 0=성공, 3=권한, 4=카카오톡 미실행 |
| `rooms [검색어]` | `--exact` | 0=결과 있음/없음 구분 없이 0, 3/4=환경 |
| `open <방>` | `--exact`, `--limit` | 2=방 없음, 5=동명 방(비대화형에서) |
| `send <방> <메시지>` | `--stdin` `--exact` `--yes` `--dry-run` `--max-chars` (인자 생략 시 편집기) | 0=sent, 5=동명 방, 6=unknown, 7=failed, 8=빈 메시지/너무 김 |
| `search <검색어>` | `--room` | 0 |
| `doctor` | — | 0=정상, 3=권한, 4=미실행 |
| `alias add/list/remove` | — | 0, 9=별칭 충돌 |
| `cache clear` | `--yes` | 0 |

출력 형식은 design 문서의 예시를 정본으로 삼는다 (`✓ 개발팀에 전송됨  14:32`, inbox의 `●` 안읽음 표시 등). 종료 코드는 여기서 확정하고 공통부·어댑터가 동일하게 쓴다.

## 프로세스 간 통신

macOS 브리지(Swift)는 공통부(Rust)와 다른 언어이므로 **반드시 서브프로세스**다. Windows 브리지(Rust)는 서브프로세스 또는 crate 링크 중 ADR이 정한다.

권장 IPC: 어댑터를 **서브프로세스로 실행하고, 한 줄 JSON 요청 → 한 줄 JSON 응답** (stdout). 또는 어댑터가 짧게 살고 명령 인자로 요청받아 JSON을 stdout에 출력하고 종료. Windows를 crate로 링크하더라도 **같은 JSON 요청/응답 shape의 함수 인터페이스**를 노출하여, 디스패치 계층에서 두 어댑터가 구분되지 않게 한다.

계약에 고정할 것: 요청/응답 봉투 형식(서브프로세스든 crate든 동일), 타임아웃, 어댑터 stderr/로그는 진단용(메시지 본문 금지), 종료 코드 vs JSON `error` 필드의 역할 분담.

## SQLite 스키마 (`docs/db-schema.sql`)

- `rooms` — `room_id` (어댑터의 불투명 문자열), `title`, `member_count`, `unread_count`, `last_message_text`, `last_message_at`, `synced_at`
- `messages` — `room_id`, `sender`, `text`, `at`, `outgoing`, 캐시된 최근 메시지
- `messages_fts` — FTS5 가상 테이블, `search` 명령용
- `aliases` — `name` (예: `dev`), `title`, `created_at`. **로컬 전용.** 별칭이 가리키는 방이 사라졌거나 제목이 여러 방과 겹치면 전송 전 재확인 (공통부 로직)
- `send_log` — `room_id`, `title_at_send`, `text`, `status`, `at`, `error_code`. 최종 상태 기록

컬럼은 snake_case. 어댑터 JSON(camelCase) → DB 저장 시 공통부가 변환.

## 기술 스택 ADR (`docs/adr/0001-tech-stack.md`)

스택은 **확정**되었다. ADR은 이 결정과 근거를 기록하는 문서이지, 언어를 재평가하는 자리가 아니다.

- **공통부**: Rust. 근거 — 런타임 없는 단일 바이너리, `enum`+exhaustive match로 send 상태 머신·에러 코드를 컴파일 타임에 강제, rusqlite에 SQLite+FTS5 번들, Windows 브리지와 툴체인 통일.
- **macOS 브리지**: Swift. AX API 1급 지원, 유니버설 바이너리.
- **Windows 브리지**: Rust(windows-rs). UIA COM 직접 바인딩, 단일 네이티브 exe, 공통부와 타입·테스트 하네스 공유.
- 결과: 툴체인 2개(Rust, Swift), 런타임 0.

**ADR에서 아직 결정할 것**: 공통부와 Windows 브리지를 (a) 별도 서브프로세스로 둘지 (b) 하나의 Rust 바이너리에 crate로 링크할지. macOS 브리지는 별도 Swift 바이너리이므로 반드시 서브프로세스다. 어느 쪽이든 **계약의 IPC 추상화는 유지** — 디스패치 계층은 "macOS 어댑터"와 "Windows 어댑터"를 동일한 인터페이스로 호출하고, 내부가 서브프로세스 호출인지 함수 호출인지 몰라야 한다. (b)를 택해도 Windows 어댑터는 계약이 정한 것과 동일한 JSON shape을 반환한다.

## 계약 유지 (변경 시)

`docs/adapter-contract.md`가 이미 있으면 전면 재작성하지 않는다:
- 파일 상단 변경 이력에 `버전 / 날짜 / 변경 / 사유` 추가
- 바뀐 섹션만 수정
- 영향받는 필드·함수를 명시하고 core-engineer, 양쪽 어댑터 엔지니어, qa-inspector에게 SendMessage
- qa-inspector는 새 계약 버전 기준으로 회귀 검증
