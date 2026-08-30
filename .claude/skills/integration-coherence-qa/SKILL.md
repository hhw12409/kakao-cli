---
name: integration-coherence-qa
description: "kakao-cli의 통합 정합성을 검증한다. 공통부↔OS 어댑터 계약 정합성(serve 프레이밍 요청/응답/이벤트 셰이프, one-shot 응답, 필드명·래핑·roomId 타입), macOS↔Windows 동작 패리티(같은 입력→같은 출력·에러 코드·이벤트), send 안전 불변식(동명 방 자동 선택 금지·Enter=확인·unknown 재전송 금지·watch dedup은 브리지 소유), TUI 스펙 준수, 메시지 본문 유출 여부를 교차 비교로 점검할 때 사용. 각 모듈 완성 직후 점진적으로 실행."
---

# 통합 정합성 QA

kakao-cli에서 런타임 버그의 1순위 원인은 **경계면 불일치** — 공통부와 어댑터가 각각 "맞게"
구현됐지만 연결 지점에서 계약이 어긋나는 것. 2순위는 **플랫폼 갈라짐** — macOS와 Windows가 같은
요청에 다르게 반응하는 것.

검증의 핵심 질문은 "이게 존재하는가?"가 아니라 **"생산자의 출력이 소비자의 기대와 정확히 일치하는가?"**

정본: `docs/adapter-contract.md`(v2.0.0), `docs/command-spec.md`.

## 왜 정적 리뷰·빌드 통과로는 못 잡나

- serde `Value` / 제네릭 / untagged enum 이 쓰이면 셰이프가 어긋나도 컴파일러가 통과시킨다.
- 각 컴포넌트를 개별로 보면 다 정상. 어긋남은 **연결 지점**에만 있다.
- "브리지에 serve 디스패치가 있는가"와 "그 응답 라인이 공통부 `ServeAdapter` 의 파싱 기대와 맞는가"는 전혀 다른 검증.

## 검증 방법: 양쪽 동시 읽기

경계면은 반드시 **양쪽 코드를 같이 열어** 비교한다.

| 검증 대상 | 왼쪽 (생산자) | 오른쪽 (소비자) |
|----------|-------------|---------------|
| serve 응답 shape | `adapters/*/` serve 디스패치가 쓰는 `data` | `crates/kakao-core/src/adapter/serve.rs` 의 `request_typed` + `crates/kakao-contract` 타입 |
| serve 이벤트 shape | 브리지의 `MessageEvent`/`RoomClosedEvent`/`ErrorEvent` (Swift) / `ServeEvent` (Rust) | `ServeMessage::parse` + 워커의 `translate` + `App::apply` |
| one-shot 응답 | `adapters/*/` 의 `Envelope`/`envelope` | `adapter/subprocess.rs` (doctor 경로) |
| 플랫폼 패리티 | macOS 브리지 serve 출력 예시 JSON | Windows 브리지 serve 출력 예시 JSON |
| send 상태 전이 | `docs/adapter-contract.md` §3 | `send::send_in_room` + `db::send_log_resolve` + 브리지 `sendText` |
| 에러 코드 | 계약의 에러 enum | `render::error_message` 매핑 + 브리지의 에러 반환 |
| TUI 출력 | `docs/command-spec.md` | `tui/ui.rs`, `tui/app.rs::apply` |
| DB 매핑 | `docs/db-schema.sql`(snake_case) | 어댑터 JSON(camelCase) → `db::insert_messages`/`upsert_rooms` 변환 |

## 우선순위별 체크리스트

### 1. 계약 정합성 (최우선)

- [ ] serve 요청: `{"id","method","params"}` — 공통부 `ServeRequest::new` 가 만드는 것과 브리지가 파싱하는 것이 일치
- [ ] serve 응답: 성공 `{"id","ok":true,"data":...}` / 실패 `{"id","ok":false,"error":"<CODE>"}`. `id` 상관 정확
- [ ] serve 이벤트: `event` 키로 응답과 구별됨. `message`/`roomClosed`/`error` 셰이프가 `ServeEvent` 와 일치 (`roomId` camelCase, `message` 는 `Message` 셰이프)
- [ ] `openRoom`/`watch`/`unwatch` 성공 `data` 가 `{}`
- [ ] one-shot(doctor): `{"ok":true|false,...}` 한 줄, 처리된 결과는 exit 0
- [ ] 각 함수 반환 shape이 계약과 정확히 일치 (필드명·타입·필수 여부)
- [ ] `roomId` 가 양 어댑터 모두 `string`, 공통부가 내부 구조 해석 안 함
- [ ] camelCase(JSON) ↔ snake_case(DB) 변환이 한 곳에서 일관되게
- [ ] 공통부가 어댑터 응답을 **런타임 검증** (`request_typed` 실패 시 "계약 위반" 내부 오류)
- [ ] 어댑터가 계약 enum 밖의 에러 문자열을 반환하지 않음

### 2. 플랫폼 패리티

- [ ] 같은 시나리오에 대해 macOS·Windows serve 요청/응답/이벤트 JSON 을 diff → 필드·순서·포맷 동일
- [ ] 타임스탬프 형식 동일 (ISO 8601 UTC, 로케일 문자열 없음)
- [ ] `null` 허용 필드(`memberCount`, `lastMessage`, `appVersion`)를 한쪽만 채우지 않음
- [ ] 같은 실패 상황에서 같은 에러 코드
- [ ] `kind:"unsupported"` 메시지 처리 방식 동일
- [ ] watch 이벤트 발화 조건 동일 (새로 추가된 행만, baseline 이후)
- [ ] `roomClosed` 발화 조건 동일 (대화가 카카오톡에서 안 보임 N회 연속)
- [ ] 타입은 `crates/kakao-contract` 를 Windows 가 직접 의존 → 구조적으로 일치. macOS `Contract.swift` 미러가 어긋나지 않았는지 `--self-test` 로 확인

### 3. send / watch 안전 불변식

- [ ] `/switch` 가 동명 방 여럿과 일치할 때 오버레이에 **기본 선택값이 없다** (`App::apply(Ambiguous)` 이후 `room_title` 안 바뀜)
- [ ] `App::pick(n)` 만이 방을 선택 → 자동 선택 경로가 코드에 없음
- [ ] 전송 확인은 Enter — 별도 프롬프트 없음. `--yes`/`--dry-run` 같은 우회 경로가 없음(플래그 자체가 없음)
- [ ] `status:"unknown"`(SEND_VERIFY_TIMEOUT)에 대해 **자동 재시도가 없다** (`send_in_room` 이 그냥 `SendOutcome` 반환, `App::apply(SendUnknown)` 은 상태줄만)
- [ ] 사전 검증 실패(빈/초과)는 `send_log` 에 안 들어감 (`pending` 미진입)
- [ ] `status` 가 상태 머신 경로(`db::send_log_resolve`)로만 설정됨 — `grep 'send_log.*status'` 전수
- [ ] **watch dedup 은 브리지 소유** — 공통부 워커에 자체 dedup 로직이 없고 받은 `message` 이벤트를 그대로 append (`db::insert_messages` 의 `UNIQUE` 는 재적재 멱등용이지 이벤트 필터가 아님)
- [ ] 보낸 메시지를 공통부가 로컬 에코하지 않음 (watch 폴링이 잡을 때만 트랜스크립트에)

### 4. TUI 스펙 준수

- [ ] 인자 없이 실행 → TUI, `doctor` → 진단. 다른 서브명령 없음 (`cli.rs` 의 `Command` 가 `Doctor` 하나)
- [ ] TTY 아니면 거부 (exit 1)
- [ ] 슬래시 명령: `/rooms` `/switch` `/alias` `/help` `/quit` 파싱 정확
- [ ] 상태줄: `✓ 전송됨 HH:MM` / `✗ …` / `? 전송 확인 불가`
- [ ] 종료 코드가 스펙대로 (0/1/2/3/4/6/7/8/9)
- [ ] 첫 실행 게이트: 권한 없음/미실행 시 스택트레이스 아닌 doctor 수준 안내

### 5. 개인정보

- [ ] 로그·stderr·(있다면)텔레메트리에 **메시지 본문이 없다** — 로그/`eprintln` 호출부 전수 grep. 브리지 stderr `diagnostic` 도
- [ ] `send_log.text` 외에 메시지 본문이 DB·파일에 안 남음
- [ ] 별칭이 로컬 SQLite 밖으로 안 나감

### 6. 코드 품질

- [ ] serve 프로세스 크래시/EOF 시 `Disconnected` 이벤트 → 워커 종료 → 상태줄 안내 (무한 루프·패닉 없음)
- [ ] `next_event` 타임아웃이 UI 렌더 페이싱을 막지 않음
- [ ] AX 락(macOS `Serve.axLock`) / 워커 단일 스레드로 폴러·요청 동시 실행 방지

## 스크립트 대조 (general-purpose 타입 활용)

- serve 프레이밍: `printf '요청들\n' | kakao-<os>-bridge serve` 로 실제 응답 라인 수집 → 계약 §5 와 대조
- `status\s*[:=]` 전수 grep → 상태 머신과 대조
- 로그/print 호출부에서 메시지 본문 변수가 인자로 들어가는지 grep
- `cargo test --workspace` (계약 `serve_framing`, `core_behaviour`, `tui_smoke`, windows `parser_parity`)
- `swift run kakao-macos-bridge --self-test` (Contract.swift 미러 파리티)

## 실행 시점: 점진적

- 공통부 모듈 완성 → 즉시 계약 정합성
- 브리지 serve 디스패치 완성 → 즉시 공통부 `ServeAdapter` 와 교차
- 양 브리지의 serve 모드 완성 → 즉시 프로토콜 패리티
- 이유: 초기 경계면 불일치가 후속 모듈로 전파되기 전에 잡는다.

## 리포트 형식

`_workspace/qa/qa-report-{모듈}.md`:

```
## {모듈} — {날짜}
### 통과
- [항목] — [방법]
### 실패
- [파일:라인] 기대: X / 실제: Y
  재현: [조건]  ·  수정 방향: [구체적으로]  ·  통지: [에이전트(경계면이면 양쪽)]
### 미검증
- [항목] — 사유: 실제 카카오톡/OS/접근성 권한 필요
```

발견은 리포트에만 쓰지 말고 **해당 에이전트에게 즉시 SendMessage**. 계약 모호성이면 cli-architect 에게.

## 환경 제약

실제 카카오톡 앱/권한이 없어 동적 검증이 불가능하면(예: macOS 라이브 watch 캐던스, Windows 라이브
UIA), 정적 교차 비교로 가능한 만큼 하고 나머지는 **"미검증 — 환경 필요"** 로 명시한다. 검증 못 한
것을 "통과"로 적지 않는다.

## 이전 리포트가 있을 때

`_workspace/qa/`에 리포트가 있으면 이전 실패 항목의 수정 여부를 먼저 확인(회귀 검증)한 뒤 새
모듈로. 계약 버전이 올라갔으면 새 버전 기준으로 재검증.
