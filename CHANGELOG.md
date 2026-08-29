# Changelog

[Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식. 구현 순서:
macOS PoC → `send` 완성 → Windows 어댑터 → inbox/검색.

## [0.1.0] — 2026-08-29

첫 릴리스. macOS 전용 (Windows 어댑터는 다음 단계).

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

### 라이브 검증 (KakaoTalk 26.6.1, 2026-08-29)

- **`send` 30회 연속 정확 전송 통과** (design 완료 기준). 30/30 sent, 대상 방
  순서·내용 정확, 타 방 유출 없음.
- `outgoing` 판정: 말풍선 X 좌표로 좌/우 정렬 구분 (받은 것 = 창 왼쪽+~60,
  보낸 것 = 창 왼쪽+120 이상). 내 메시지는 `sender: ""`, `open` 출력에서 `나` 로 표시.
- `doctor` / `inbox` / `rooms` / `open` / `search` / **`send`** 모두 실제 카카오톡에서 동작 확인.
- `send`: 방 열기(창 raise + 더블클릭 — 목록 행에 AX 액션 없음) → 입력창 `AXValue`
  설정 → 전송 버튼 위치 클릭 합성(버튼 `AXPress` 무반응) → 메시지 영역 맨 아래로
  스크롤 후 폴링 검증. `✓ 황현우에 전송됨  03:24` 확인. 상태 머신 정상.
- 성능: `listRooms` 배치 AX 읽기 + 40행 제한으로 23s → 2.3s.

### 알려진 제한

- 읽기는 카카오톡 창이 보일 때만 동작 (최소화 시 `KAKAO_WINDOW_NOT_VISIBLE`,
  0.1초). `openRoom`/`sendText` 는 방 열기 시 메인 창을 잠시 활성화한다.
- 카카오톡 메시지 목록은 가상화(virtualized) — 화면 밖 메시지는 AX 트리에 없음.
  `readRecent`/검증은 스크롤 후 읽지만, 매우 긴 대화는 여전히 최근 N개만.
- `roomId` 는 세션 한정 `"row:N"`.
- Windows 어댑터 미구현. 빌드에 ad-hoc `codesign` 단계 미포함.
