# Windows 어댑터 — UI Automation (UIA)

카카오톡 Windows 앱을 Windows UI Automation으로 조작하는 브리지. 언어는 **Rust(windows-rs)** 로 확정 — 공통부와 툴체인·타입 공유, 런타임 없는 단일 exe. `docs/adapter-contract.md`의 5개 함수를 구현하고 **macOS 어댑터와 바이트 단위로 같은 출력·에러 코드**를 낸다.

## 목차

1. windows-rs로 UIA 다루기
2. 권한·접근
3. 앱·창 찾기
4. UIA 트리 탐색
5. 읽기: 방 목록, 최근 메시지
6. 쓰기: 방 열기, 텍스트 전송, 전송 검증
7. healthCheck
8. 빌드·배포
9. fixture 테스트

---

## 1. windows-rs로 UIA 다루기

- 크레이트: `windows` (features 로 `Win32_UI_Accessibility`, `Win32_System_Com` 등 활성화).
- 진입점: `CoInitializeEx` → `CUIAutomation` COM 객체 생성 → `IUIAutomation`.
- 핵심 인터페이스: `IUIAutomationElement`, `IUIAutomationTreeWalker`, `IUIAutomationCondition`, 그리고 패턴별 `IUIAutomation*Pattern` (`InvokePattern`, `ValuePattern`, `TextPattern`, `SelectionItemPattern`, `LegacyIAccessiblePattern`).
- COM 호출이 장황하므로 **얇은 헬퍼 레이어**를 만든다: `find_first(root, &condition) -> Option<Element>`, `get_name(el) -> String`, `set_value(el, text)`, `invoke(el)` 등. 이 레이어 위에서 5개 함수를 구현.
- 공통부와 **JSON 타입·에러 enum을 같은 크레이트/모듈로 공유**한다 (serde 직렬화). 서브프로세스든 crate 링크든 반환 shape은 계약과 동일.
- 조사 도구: Inspect.exe / Accessibility Insights for Windows 로 카카오톡 UIA 트리를 수동 확인.

## 2. 권한·접근

- UIA는 보통 별도 사용자 권한 부여가 필요 없지만, **대상 앱이 관리자 권한으로 실행 중이면** 비관리자 프로세스에서 접근 불가. `healthCheck`가 이 경우를 감지해 `ACCESSIBILITY_PERMISSION_DENIED` + 안내.
- UIPI(User Interface Privilege Isolation) 무결성 수준 불일치도 같은 증상. 브리지와 카카오톡의 무결성 수준을 확인.

## 3. 앱·창 찾기

- 프로세스: `CreateToolhelp32Snapshot`/`Process32First` 또는 `sysinfo` 크레이트로 KakaoTalk 프로세스명(실측) 확인. 없으면 `KAKAO_NOT_RUNNING`, **임의 실행 금지.**
- `IUIAutomation::GetRootElement` → `CreateAndCondition`/`CreatePropertyCondition`(`UIA_ClassNamePropertyId` + `UIA_NamePropertyId`)로 `FindFirst(TreeScope_Children, ...)`. 카카오톡 Windows는 창 `ClassName`이 고유한 경우가 많다 (실측).

## 4. UIA 트리 탐색

- `IUIAutomation::ControlViewWalker`(또는 RawViewWalker) 로 순회. ControlView가 노이즈가 적다. `FindAll`/`FindFirst` + Condition 조합도 활용.
- 요소 식별 우선순위: `AutomationId` > `Name` + `ControlType` > 위치. `AutomationId`가 비어 있으면 `Name`·`ControlType`·부모 컨텍스트 조합.
- 패턴 인터페이스 (`GetCurrentPattern` + 캐스팅): `IUIAutomationInvokePattern`(버튼 실행), `IUIAutomationValuePattern`(텍스트 설정), `IUIAutomationTextPattern`(텍스트 읽기), `IUIAutomationSelectionItemPattern`(리스트 항목 선택), `IUIAutomationLegacyIAccessiblePattern`(구형 요소 fallback).
- 카카오톡이 커스텀 렌더링(Chromium/자체 UI)을 쓰면 트리가 얕고 `Name`만 노출될 수 있다 — 그 경우 `Name` 텍스트 매칭 + 좌표 기반 `InvokePattern` 조합이 불가피할 수 있으나, 반드시 주석으로 이유 명시.

개발용: 전체 UIA 트리를 JSON으로 덤프하는 서브커맨드 (fixture 생성·버전 비교용). Inspect.exe / Accessibility Insights 로 수동 조사 병행.

## 5. 읽기

### listRooms

- 채팅 목록 리스트(`ControlType.List` 또는 `Tree`)를 찾아 각 항목(`ListItem`)에서: 제목, 안읽음 배지(`Text` 자식), 마지막 메시지 미리보기, 시각.
- 인원수: macOS와 동일 정책 — 목록에서 못 얻으면 `null` 허용 (계약 합의). **macOS가 `null`을 주는 상황에서 Windows가 숫자를 주면 패리티 위반.**
- `roomId`: `AutomationId`가 있으면 그것, 없으면 `(제목 + 인덱스)` 해시. macOS와 **다른 내부 표현이어도 되지만** 둘 다 `string`·불투명.

### readRecent

- 메시지 영역(`ControlType.List`/`Pane` 내부)에서 최근 `limit`개 항목. 발신자·본문·시각·`outgoing` 판정.
- 사진/파일/이모티콘: 계약대로 `kind: "unsupported"` 또는 스킵. macOS와 **같은 선택**.

## 6. 쓰기

### openRoom(roomId)

- `roomId`로 리스트 항목을 다시 찾아 `SelectionItemPattern.Select()` 또는 `InvokePattern.Invoke()`. 더블클릭 좌표 합성 회피.
- 창 전면화 없이 방 전환 확인. 안 되면 계약에 명시 요청 (macOS와 같은 결론이어야 함).

### sendText(roomId, text)

1. `openRoom` 으로 대상 방 확보.
2. 입력 필드(`ControlType.Edit` 또는 `Document`)를 찾는다.
3. 텍스트 설정: `IUIAutomationValuePattern::SetValue(text)`. 줄바꿈 포함 본문이 그대로 들어가 전송 키와 충돌하지 않음. 안 먹으면 클립보드 fallback (`OpenClipboard`/`SetClipboardData` 또는 `clipboard-win` 크레이트 → Ctrl+V 를 `SendInput` 으로 → 원래 클립보드 복원).
4. 전송: 전송 버튼 `InvokePattern.Invoke()`. 카카오톡 "Enter로 전송" 설정에 의존하지 않도록 버튼 우선. 버튼이 없으면 입력 필드 포커스 + `{ENTER}` (SendKeys/SendInput).
5. **검증**: 메시지 영역 폴링(100ms × 최대 3s)으로 방금 텍스트의 `outgoing` 항목 확인.
   - 확인 → `{ status: "sent", at: <ISO8601 UTC> }`
   - 타임아웃 → `{ status: "unknown", error: "SEND_VERIFY_TIMEOUT" }`
   - 필드/버튼 실패 → `{ status: "failed", error: "SEND_INPUT_FAILED" }`

폴링 간격·타임아웃 수치는 macOS 어댑터와 맞춘다.

## 7. healthCheck

```
kakaoRunning: 프로세스 조회
accessibilityGranted: UIA로 메인 창 요소를 실제로 얻을 수 있는가 (관리자 권한/무결성 수준 문제 감지)
appVersion: 카카오톡 실행 파일의 버전 리소스 (GetFileVersionInfo / VerQueryValue, 또는 windows-rs 래핑; 경로 실측)
issues: 실패 항목 + 셀렉터 맵에 없는 버전이면 APP_VERSION_UNSUPPORTED
```

## 8. 빌드·배포

- ADR 결정에 따라: (a) 서브프로세스면 별도 실행 파일 (`adapters/windows/kakao-windows-bridge.exe`), (b) crate 링크면 공통부 Windows 빌드에 포함되는 모듈. 어느 쪽이든 계약이 정한 요청/응답 shape을 지킨다.
- (a) 통신: argv 요청, stdout 한 줄 JSON 응답, stderr 진단 로그 (메시지 본문 금지) — macOS 브리지와 동일 프로토콜.
- Rust이므로 런타임 없는 단일 exe. `cargo build --release`, MSVC 툴체인.
- 배포는 **Scoop bucket** (GitHub 레포의 JSON manifest 하나, 비용 0). Scoop이 받아 shim으로 실행하면 SmartScreen 마찰이 적다. 서명·winget 미사용. 상세는 `docs/packaging.md`.

## 9. fixture 테스트

- UIA 트리 덤프 서브커맨드로 실제 트리를 `adapters/windows/fixtures/kakaotalk-<version>.json` 저장.
- 파싱·탐색 함수를 fixture로 단위 테스트.
- **macOS 어댑터와 같은 시나리오 명세**를 공유하여 두 어댑터가 같은 상황에서 같은 JSON을 내는지 대조 (qa-inspector가 diff).
