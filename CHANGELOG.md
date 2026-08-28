# Changelog

[Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식. 구현 순서:
macOS PoC → `send` 완성 → Windows 어댑터 → inbox/검색.

## [Unreleased]

### Added

- 어댑터 계약 v1.1.0 (`docs/adapter-contract.md`): 5개 인터페이스 함수, IPC 봉투,
  에러 코드 enum(10개), send 상태 머신. v1.1.0에서 `KAKAO_WINDOW_NOT_VISIBLE`
  추가, 목록 시각이 상대 라벨일 때 `at: ""` 허용.
- macOS 어댑터 셀렉터를 실제 KakaoTalk 26.6.1 접근성 트리 기준으로 확정
  (`Selectors.swift`): 방 목록은 메인 창 `AXTable(_NS:63)`, 대화는 방 이름
  제목의 별도 창 `AXTable(_NS:33)`, 입력창 `AXTextArea desc="메시지 입력"`,
  전송 `AXButton title="전송"`. 한국어 시각 라벨(`"오전 11:17"`, `"어제"`) 파싱.
  self-test 26개 그린.
- 공통부(Rust, `crates/kakao-core`):
  - `inbox` · `rooms` · `open` · `send` · `search` · `doctor` · `alias` · `cache` 명령.
  - 방 이름 해석 — 정확히 하나면 진행, 여러 개면 기본값 없는 번호 선택,
    비대화형/`--yes` 에서는 종료 코드 5.
  - `send` 안전 정책 — `--stdin`(줄바꿈 보존) · 편집기 모드 · `--dry-run` ·
    `--yes` · `--exact` · `--max-chars`, `@별칭`.
  - send 상태 머신 `pending → sent | failed | unknown`. `unknown` 자동 재시도 없음.
    `--dry-run` 은 `pending` 에 진입하지 않음.
  - SQLite 캐시(rusqlite 번들) + FTS5 `search`.
  - 어댑터 서브프로세스 디스패치 + 응답 런타임 검증.
  - 첫 실행 온보딩 — 브리지 미발견 · 카카오톡 미실행 · 권한 없음 시 스택트레이스
    대신 `doctor` 수준 안내.
- 공통 타입 크레이트(`crates/kakao-contract`) — 공통부와 Windows 브리지 공유.
- macOS 어댑터(Swift, `adapters/macos`):
  - `kakao-macos-bridge` 실행 파일, 계약 IPC 구현.
  - 접근성 트리 파서(방 목록 · 최근 메시지) — `UINode` 추상화로 fixture 테스트 가능.
  - 버전별 셀렉터 맵, `--dump-tree` 개발 서브커맨드, `--self-test` 회귀 검증.
  - `healthCheck` — 실행 여부 · TCC 권한 · 앱 버전.
  - `AXUIElementSetMessagingTimeout` 으로 개별 접근성 호출 상한.
- ADR: `0001-tech-stack`(공통부 Rust / macOS Swift / Windows Rust, 양 어댑터
  서브프로세스), `0002-distribution`(Homebrew tap 소스 빌드 + Scoop, 비용 0).
- 문서: `README.md`, `docs/errors.md`, `docs/command-spec.md`, `docs/db-schema.sql`,
  `docs/packaging.md`, `docs/help/`.

### 알려진 제한

- `listRooms` 등은 **카카오톡 창이 보일 때만** 동작한다. 창 최소화 시
  `kAXWindowsAttribute` 가 비어 `KAKAO_WINDOW_NOT_VISIBLE`(exit 4)로 안내한다
  (0.1초 내). 창이 열려 있는 상태의 실측 파싱은 아직 미검증.
- `readRecent` 의 `outgoing`/`sender` 판정 미구현 (self-chat 덤프만 있어 발신자
  신호를 못 찾음). 현재 `sender: ""`, `outgoing: false`. 그룹 채팅 덤프 필요.
- `roomId` 는 세션 한정 `"row:N"` (목록 행 인덱스). 세션 간 안정성 없음 (계약 허용).
- Windows 어댑터 미구현.
- 빌드에 ad-hoc `codesign` 단계 미포함 (packaging 단계에서 추가 예정).
