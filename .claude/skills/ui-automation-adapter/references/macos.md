# macOS 어댑터 — Accessibility API

카카오톡 Mac 앱을 macOS Accessibility API로 조작하는 Swift 브리지. `docs/adapter-contract.md`의 5개 함수를 구현한다.

## 목차

1. 권한 (TCC)
2. 앱·창 찾기
3. 접근성 트리 탐색
4. 읽기: 방 목록, 최근 메시지
5. 쓰기: 방 열기, 텍스트 전송, 전송 검증
6. healthCheck
7. 빌드·배포
8. fixture 테스트

---

## 1. 권한 (TCC)

Accessibility API는 사용자가 **시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용**에서 이 도구(또는 이 도구를 실행하는 터미널/바이너리)에 권한을 줘야 동작한다.

- `AXIsProcessTrusted()` 로 권한 보유 여부 확인.
- `AXIsProcessTrustedWithOptions([kAXTrustedCheckOptionPrompt: true])` 로 시스템 프롬프트 유도 (healthCheck에서 한 번만, 조용한 확인은 옵션 false).
- 권한 없으면 다른 함수는 `ACCESSIBILITY_PERMISSION_DENIED` 로 즉시 실패. `healthCheck`의 `recovery`에 설정 경로 안내 (구체 문구는 `docs/errors.md`).
- TCC 권한은 바이너리의 코드 서명(또는 미서명 시 경로+cdhash)에 묶인다. 빌드 단계에서 **ad-hoc 서명**(`codesign --force --sign - --identifier <stable-id>`, 무료)을 하면 안정적 식별자가 생겨 `brew upgrade` 후에도 권한이 덜 초기화된다. Developer ID·공증은 쓰지 않는다(비용). 자세한 배포는 docs-writer의 `docs/packaging.md`.

## 2. 앱·창 찾기

- `NSRunningApplication.runningApplications(withBundleIdentifier:)` 로 카카오톡 프로세스 확인. 번들 ID는 실제 설치본에서 확인 (예: `com.kakao.KakaoTalkMac` 계열 — 반드시 실측).
- 미실행이면 `KAKAO_NOT_RUNNING`. **`NSWorkspace.launchApplication` 등으로 임의 실행하지 않는다.**
- `AXUIElementCreateApplication(pid)` 로 앱 요소를 얻고, `kAXWindowsAttribute` 로 창 목록. 메인 채팅 창을 역할·제목·크기로 식별.

## 3. 접근성 트리 탐색

- `AXUIElementCopyAttributeValue` 로 `kAXChildrenAttribute`, `kAXRoleAttribute`, `kAXTitleAttribute`, `kAXValueAttribute`, `kAXDescriptionAttribute`, `kAXIdentifierAttribute` 를 읽는다.
- `kAXIdentifier`(= AXDOMIdentifier)가 있으면 최우선으로 쓴다 — 가장 안정적. 카카오톡이 제공하는 식별자를 트리 덤프로 조사.
- 리스트/테이블은 `kAXRowsAttribute`, 텍스트 영역은 `AXTextArea`/`AXStaticText`.
- 좌표가 꼭 필요하면 `kAXPositionAttribute`/`kAXSizeAttribute` 로 요소 위치를 얻어 `AXUIElementPostKeyboardEvent`/합성 클릭 대신 `kAXPressAction` 등 **액션**을 우선 시도.

트리 구조 조사용으로, 앱 실행 중 전체 트리를 JSON으로 덤프하는 개발용 서브커맨드를 만들어 두면 fixture 생성과 버전 비교에 쓸 수 있다.

## 4. 읽기

### listRooms

- 채팅 목록(친구/채팅 탭 중 "채팅") 리스트 요소를 찾아 각 행에서: 제목(`AXTitle`/자식 `AXStaticText`), 안읽음 배지(별도 `AXStaticText`/`AXDescription`), 마지막 메시지 미리보기, 시각.
- 인원수는 목록에 없을 수 있다 — 방을 열지 않고 얻을 수 없으면 계약 논의 (열 때만 채우거나 `null` 허용). 부작용 없는 읽기를 지키려면 인원수를 옵셔널로 하는 게 낫다.
- `roomId`: 행의 `kAXIdentifier`가 있으면 그것, 없으면 `(제목 + 목록 인덱스)` 해시.

### readRecent

- 대상 방의 메시지 영역(`AXScrollArea` 내부 리스트/그룹)을 찾아 최근 `limit`개 말풍선 요소를 읽는다.
- 각 말풍선: 발신자(`AXStaticText` 또는 부모 그룹의 `AXDescription`), 본문, 시각, `outgoing` 판정(정렬/스타일 role 차이, 또는 발신자 == 내 이름).
- 사진/파일/이모티콘 말풍선: 계약이 정한 대로 `kind: "unsupported"` 표시 또는 스킵. **본문 텍스트만 지원.**

## 5. 쓰기

### openRoom(roomId)

- `roomId`로 목록에서 행을 다시 찾아 `kAXPressAction` 수행 (더블클릭 좌표 합성 대신).
- 창을 전면화하지 않고 방 전환이 되는지 확인. 안 되면 계약에 "openRoom은 창 활성화가 필요" 명시를 요청.

### sendText(roomId, text)

1. `openRoom` 으로 대상 방 확보.
2. 입력 필드(`AXTextArea`, 보통 `kAXFocused` 가능한 하단 요소)를 찾는다.
3. 텍스트 설정: `AXUIElementSetAttributeValue(field, kAXValueAttribute, text)`. 이 방식이 줄바꿈을 값에 그대로 넣어 전송 단축키(Enter)와 충돌하지 않아 안전. 안 먹으면 클립보드 붙여넣기 fallback (`NSPasteboard` 저장 → Cmd+V 합성 → 클립보드 복원).
4. 전송: 전송 버튼 요소에 `kAXPressAction`, 또는 입력 필드 포커스 상태에서 Return 키 이벤트. 카카오톡 설정의 "Enter로 전송" 여부에 의존하지 않도록 버튼 액션을 우선.
5. **검증**: 메시지 영역을 폴링(예: 100ms × 최대 3s)하여 방금 텍스트와 일치하는 `outgoing` 말풍선이 생겼는지 확인.
   - 확인 → `{ status: "sent", at: <ISO8601 UTC now> }`
   - 타임아웃 → `{ status: "unknown", error: "SEND_VERIFY_TIMEOUT" }`
   - 필드/버튼 못 찾음 → `{ status: "failed", error: "SEND_INPUT_FAILED" }`

## 6. healthCheck

```
kakaoRunning: NSRunningApplication 조회
accessibilityGranted: AXIsProcessTrusted()
appVersion: 카카오톡 번들의 CFBundleShortVersionString (경로: /Applications/KakaoTalk.app 등, 실측)
issues: 위 셋 중 실패한 것 + 셀렉터 맵에 없는 버전이면 APP_VERSION_UNSUPPORTED
```

## 7. 빌드·배포

- Swift Package(`swift build -c release`, Xcode CLT로 충분). 산출물은 CLI가 서브프로세스로 실행할 실행 파일 (`adapters/macos/kakao-macos-bridge`).
- 계약이 정한 통신: argv로 요청(JSON 또는 `subcommand + args`), stdout에 한 줄 JSON 응답, stderr에 진단 로그.
- 배포는 **Homebrew tap에서 소스 빌드** — 로컬 빌드 바이너리라 Gatekeeper·quarantine 없음. 빌드 후 **ad-hoc 서명**(무료)으로 TCC 식별자 안정화. Developer ID·공증 미사용. 상세는 docs-writer의 `docs/packaging.md`.
- Apple Silicon + Intel 유니버설 바이너리 권장 (`--arch arm64 --arch x86_64`).

## 8. fixture 테스트

- 개발용 트리 덤프 서브커맨드로 실제 카카오톡의 접근성 트리를 JSON으로 저장 → `adapters/macos/fixtures/kakaotalk-<version>.json`.
- 파싱·탐색 함수(listRooms 파서 등)를 fixture 입력으로 단위 테스트. 실제 앱 없이 CI에서 회귀 검출.
- Windows 어댑터와 **같은 시나리오**의 fixture를 쓰도록 시나리오 명세를 공유 (방 3개, 안읽음 분포, 1:1 + 그룹 혼합 등).
