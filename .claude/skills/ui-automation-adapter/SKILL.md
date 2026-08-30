---
name: ui-automation-adapter
description: "kakao-cli의 OS 어댑터를 접근성 API로 구현한다. serve 모드(장수 프로세스, 줄 단위 JSON-RPC, watch 1.5초 폴러 → message/roomClosed/error 이벤트, AX 접근 단일 락 직렬화) + one-shot 프레이밍. 접근성 트리 탐색(역할·이름·값 우선, 좌표 클릭 최소화), 방 목록/최근 텍스트 읽기, 텍스트 입력·전송·전송 검증, 권한 진단, 버전별 셀렉터와 트리 fixture 테스트를 작업할 때 사용. macOS(Accessibility API/Swift) 또는 Windows(UI Automation/Rust) 어댑터 구현/수정 시 사용."
---

# UI 자동화 어댑터 구현

어댑터는 각 OS의 카카오톡 앱을 접근성 API로 읽고 조작하여 `docs/adapter-contract.md`(v2.0.0)의
인터페이스 함수를 구현한다. 카카오톡 네트워크 프로토콜은 재현하지 않는다.

**성공 기준: 두 어댑터(macOS/Windows)가 같은 요청에 같은 응답·에러 코드·이벤트를 낸다.** 어댑터별
구현 방법은 달라도, 계약이 정한 결과는 바이트 단위로 같아야 한다.

## 두 전송로 (한 바이너리)

| 서브명령 | 설명 | 용도 |
|----------|------|------|
| `serve` | 장수 프로세스. stdin 줄 단위 요청 → stdout 응답(`id` 상관) + 비동기 이벤트 (계약 §5) | 대화형 채팅 TUI (주) |
| `<method> <argsJson>` | 한 줄 응답 후 종료 (계약 §1) | `doctor`(healthCheck), `--self-test` |

serve 가 주 전송로다. one-shot 은 제거하지 않는다(테스트·doctor). 두 경로가 **같은 하부 함수**를 호출해야 한다 (macOS: `Bridge.*`, Windows: `bridge::*`).

OS별 상세는 필요할 때 로드한다:
- macOS (Accessibility API, `AXUIElement`, Swift, TCC 권한) → `references/macos.md`
- Windows (UI Automation, `ControlType`/`AutomationId`, Rust + windows-rs) → `references/windows.md`

## serve 모드

```
$ kakao-<os>-bridge serve
  in:   {"id":1,"method":"listRooms","params":{}}
  out:  {"id":1,"ok":true,"data":{...}}
  in:   {"id":5,"method":"watch","params":{"roomId":"row:3"}}
  out:  {"id":5,"ok":true,"data":{}}
  out:  {"event":"message","roomId":"row:3","message":{...}}   ← watch 폴러가 비동기 방출
  in:   {"id":0,"method":"shutdown","params":{}}               ← 응답 없이 종료
```

- **요청 루프**: stdin 을 줄 단위로 읽는다. EOF(공통부 종료) 시 프로세스 종료. `listRooms`/`openRoom`/
  `readRecent`/`sendText`/`healthCheck` 는 one-shot 과 동일한 하부 함수를 호출해 `id` 상관 응답.
- **watch**: 요청 시점의 메시지 tail 을 baseline(행 개수 + 마지막 N행 해시)으로 잡고, 백그라운드
  폴러가 **1.5초마다** 그 방의 메시지 tail 을 읽어 **새로 추가된 행만** `message` 이벤트로 방출한다.
  `watch` 는 이전 watch 를 대체. `unwatch` 로 중지.
- **de-dup 은 브리지 소유.** 공통부는 받은 `message` 이벤트를 그대로 append 한다. 브리지가 UI 구조로
  판별하는 게 정확하다. 폭주·가상화 창 밖 메시지는 일부 유실/중복 가능 — 개인 도구 수준 허용.
- **roomClosed / error 이벤트**: watch 폴링이 대화를 못 찾으면(사용자가 카카오톡에서 다른 방으로
  이동 등) N회 연속 실패 후 `roomClosed`. 창 최소화 등 일시적 조건은 `error` (advisory, 계속 재시도).
- **watch 폴링은 클릭하지 않는다.** 대화가 이미 열려 있으면 읽기만 → 포커스 탈취 없음. 대화를 못
  찾으면 강제로 열지 말고 `roomClosed`.
- **AX 접근 직렬화**: 폴러와 요청 핸들러가 동시에 접근성 트리를 만지면 안 된다. 단일 락(macOS
  `Serve.axLock`) 또는 단일 스레드로 모든 AX 호출을 감싼다.
- **stdout 쓰기 직렬화**: 폴러와 요청 핸들러가 각각 라인을 쓰므로 writer 를 락으로 보호.

## 공통 원칙 (양 어댑터 동일)

### 역할·이름·값 > 좌표

요소를 지목할 때 접근성 역할(role/ControlType), 이름, 값, 식별자(AXDOMIdentifier/AutomationId)를
쓴다. 픽셀 좌표 클릭은 최후의 수단이고, 쓸 때 이유를 주석으로 남긴다. 좌표는 해상도·DPI·창 위치·
테마·카카오톡 UI 업데이트에 모두 취약하다.

### 읽기와 쓰기 분리

`listRooms`/`readRecent`/`healthCheck` 및 **watch 폴링**은 부작용 0. `openRoom`/`sendText` 만 앱 상태를
변경한다. 읽기 경로에서 클릭·타이핑 금지.

### 포커스 탈취 최소화

- **정상 상태 watch 폴링은 포커스를 뺏지 않는다** (읽기 전용).
- `openRoom`(대화 행 클릭)과 `sendText`(입력창 붙여넣기 + 전송 클릭)는 카카오톡 창을 잠깐 앞으로
  올려야 할 수 있다 — 접근성 방식의 본질적 제약. 계약(§2)에 명시된 경우만, 끝나면 원복 시도.
- watch 시작 시 `openRoom` 을 한 번 하되, 이후 폴링은 이미 열린 대화를 읽기만 한다.

## 공통 원칙 (양 어댑터 동일)

### 역할·이름·값 > 좌표

요소를 지목할 때 접근성 역할(role/ControlType), 이름(title/Name), 값(value), 식별자(AXDOMIdentifier/AutomationId)를 쓴다. 픽셀 좌표 클릭은 최후의 수단이고, 쓸 때 이유를 주석으로 남긴다. 이유: 좌표는 해상도·DPI·창 위치·테마·카카오톡 UI 업데이트에 모두 취약하다. 접근성 속성은 훨씬 안정적이다.

### 읽기와 쓰기 분리

`listRooms`/`readRecent`/`healthCheck`는 부작용 0 — 상태를 바꾸지 않는다. `openRoom`/`sendText`만 앱 상태를 변경한다. 이 분리를 코드 구조로 지킨다 (읽기 경로에서 클릭·타이핑 금지).

### 카카오톡 창을 앞으로 가져오지 않는다

기본 동작에서 카카오톡 창의 포커스·전면 배치·크기를 건드리지 않는다. 백그라운드 창의 접근성 트리를 읽고, 필요하면 창을 활성화하지 않고 요소에 값을 설정하거나 액션을 수행한다. 불가피하게 활성화가 필요하면 계약에 명시된 경우만, 그리고 끝나면 이전 상태를 복원.

### 안정적 roomId

계약상 `roomId`는 **불투명 문자열**이고 공통부는 이를 해석하지 않는다. 세션 내에서 같은 방을 다시 지목할 수 있게 만든다 — 접근성 요소 경로의 해시, 방 목록 내 위치 + 제목 조합 등. 세션 간 안정성은 계약이 요구하지 않으면 보장하지 않아도 된다. **두 어댑터가 서로 다른 내부 표현을 써도 되지만, 둘 다 `string`이어야 하고 공통부에 불투명해야 한다.**

### 전송은 확인 전까지 성공이 아니다

`sendText` 흐름:
1. 대상 방을 연다 (또는 이미 열린 방을 확인).
2. 입력 필드를 찾아 텍스트를 설정한다. 줄바꿈이 포함된 본문은 전송 단축키와 줄바꿈 입력이 충돌하지 않게 처리 (붙여넣기 방식 등).
3. 전송을 실행한다.
4. **채팅 영역을 폴링**하여 방금 보낸 내용이 내 메시지(`outgoing`)로 나타났는지 확인한다. 타임아웃까지 확인되면 `status: "sent"` + `at`.
5. 확인 실패 → `status: "unknown"`, `error: "SEND_VERIFY_TIMEOUT"`. **`failed`가 아니다.** 재시도하지 않는다.
6. 입력 자체가 실패(필드 못 찾음 등) → `status: "failed"`, `error: "SEND_INPUT_FAILED"`.

빈 메시지(`EMPTY_MESSAGE`), 길이 초과(`MESSAGE_TOO_LONG`)는 공통부가 먼저 거르지만, 어댑터도 방어적으로 확인.

### 에러 코드는 계약 enum만

`docs/adapter-contract.md`의 에러 코드 목록 밖의 문자열을 반환하지 않는다. 새 실패 유형을 발견하면 임의 코드 대신 cli-architect에게 SendMessage로 코드 추가를 요청한다. 진단 정보는 stderr 로그에 남기되 **메시지 본문은 로그 금지.**

### 권한·앱 상태 진단

`healthCheck()`는 다른 함수가 실패하기 전에 원인을 알려준다:
- 카카오톡 실행 여부 (프로세스 존재)
- 접근성 권한 (macOS: TCC / Windows: UIA 접근 가능 여부)
- 앱 버전 (버전별 셀렉터 선택에 필요)
- 각 문제에 대해 `{ code, recovery }` — `recovery`는 사용자가 실제로 실행할 안내 (구체 문구는 docs-writer가 `docs/errors.md`에 확정, 어댑터는 코드만 정확히)

카카오톡이 안 떠 있으면 **임의로 실행하지 않는다.** 에러 코드 + 안내로 끝낸다.

### 버전별 셀렉터 + 트리 fixture 테스트

카카오톡 UI 업데이트가 이 프로젝트의 최대 리스크다. 대응:
- 셀렉터를 코드에 흩지 말고 **버전별 셀렉터 맵**으로 모은다. `healthCheck`의 `appVersion`으로 맵을 선택.
- 실제 앱 없이 회귀를 잡도록 **접근성 트리 fixture**(직렬화된 트리 스냅샷)를 저장하고, 파싱·탐색 로직을 fixture로 단위 테스트한다.
- 알려진 카카오톡 버전마다 fixture를 하나씩 확보하는 것을 목표로.

## 플랫폼 패리티 유지

- 다른 어댑터 엔지니어와 **출력 예시 JSON을 SendMessage로 공유**한다. 같은 fixture 시나리오(방 3개, 그중 1개 안읽음 2)에 대해 두 어댑터가 내는 JSON을 나란히 놓고 필드·순서·포맷·null 처리·타임스탬프 형식을 맞춘다.
- 계약이 애매한 지점을 만나면 혼자 해석하지 말고 상대에게 "너는 이걸 어떻게 했어?"를 먼저 묻는다.
- 한쪽이 계약 변경을 요청하면 다른 쪽도 영향 검토에 참여.
- 타임스탬프: 계약이 ISO 8601 UTC라고 했으면 로케일 문자열을 반환하지 않는다. 방 목록 정렬 순서, 빈 배열 표현(`[]` vs 생략)도 계약대로 통일.

## 이전 소스가 있을 때

`adapters/{os}/` 소스가 있으면 전체 재작성하지 않는다. 카카오톡 UI 변경이나 QA 지적에 해당하는 함수만 수정. 계약 버전이 올라갔으면 diff 확인 후 영향 범위만. 다른 어댑터가 먼저 수정됐으면 패리티 관점에서 따라간다.

## 산출물

- `adapters/{macos|windows}/` 하위 소스 + 빌드 스크립트
- 버전별 셀렉터 맵
- 접근성 트리 fixture + fixture 기반 테스트
- 계약 준수 출력 예시 JSON (패리티 대조용, `_workspace/`에 공유)
