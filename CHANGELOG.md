# Changelog

[Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식. 구현 순서:
macOS PoC → `send` 완성 → Windows 어댑터 → inbox/검색.

## [Unreleased]

### Changed — BREAKING: 대화형 채팅 클라이언트로 전환 (계약 v2.0.0)

kakao-cli 는 이제 one-shot 서브명령 집합이 아니라 **대화형 터미널 채팅 클라이언트**다.
`kakao-cli` 를 인자 없이 실행하면 채팅 TUI가 열린다. 근거: `docs/adr/0003-interactive-tui.md`.

- **제거된 서브명령**: `send` · `rooms` · `open` · `inbox` · `search` · `alias` · `cache`.
  방 이동·별칭은 채팅 화면 안의 슬래시 명령(`/switch`, `/rooms`, `/alias`)으로.
  `doctor` 만 서브명령으로 남는다.
- **새 전송로 — serve 모드**: `kakao-<os>-bridge serve` 장수 프로세스 + 줄 단위 JSON-RPC
  + `message` 이벤트 스트림 (계약 §5). one-shot 프레이밍은 `doctor` 와 셀프테스트용으로 유지.
- **수신 = 폴링**: 브리지가 watch 중인 방의 접근성 트리를 1.5초 간격으로 읽어 새 메시지를
  이벤트로 밀어 준다. de-dup은 브리지가 소유. 체감 지연 1~2초.
- **계약 v2.0.0** (`crates/kakao-contract`): `ServeRequest` / `ServeResponse` / `ServeEvent`
  추가, `Method` 에 `Watch`/`Unwatch`/`Shutdown`. `Message`/`Room`/`SendResult`/`Health`/
  `ErrorCode` 와이어 타입과 send 상태 머신은 불변.
- **공통부**: `tui/` 모듈(ratatui + crossterm), 어댑터 워커 스레드, `StreamAdapter` 트레잇
  + `ServeAdapter` + `MockStreamAdapter`(`KAKAO_CLI_STREAM_MOCK`). `send::send_in_room`,
  `resolve::resolve_in_list`(동명 방은 `Resolution::Many` 로 반환, 자동 선택 없음).
- **DB v2**: FTS5 검색 객체 제거 (`search` 명령이 없어짐). 마이그레이션이 `messages_fts` 를 DROP.
- **macOS 브리지**: `serve` 서브명령 + `Serve.swift`(watch 폴러, AX 접근 단일 락 직렬화).
  임베디드 대화 pane 하드닝은 라이브 AX 덤프 필요 — 후속. 현재 분리 대화 창 경로로 동작.
- **Windows 브리지**: `serve` 프로토콜 파리티 + 스텁(`serve.rs`). 라이브 UIA watch 폴러는 후속.
- **검증**: `cargo test --workspace` 그린 (`tests/tui_smoke.rs` 포함 — `/rooms` → `/switch`
  → 수신 → 전송 헤드리스 구동, 동명 방 자동 선택 안 함). macOS serve framing 실기기 확인
  (권한 게이트까지). 실기기 메시지 흐름은 접근성 권한 부여 후 검증 필요.

**마이그레이션**: 0.1.0 을 스크립트에서 쓰던 경우 대체재가 없다 — kakao-cli 는 대화형 도구다.

### Added — 카카오톡 창을 kakao-cli 가 관리

사용자가 카카오톡 창을 직접 띄워 두고 최소화하지 않아야 했던 제약을 없앤다.

- **macOS serve 브리지**: 시작 시 카카오톡 상태를 스냅샷 → 요청 전 `WindowControl.ensureAwake`
  (미실행이면 `NSWorkspace.openApplication`, 숨김/최소화면 복원, 창 없으면 재오픈) →
  `shutdown`/EOF 시 스냅샷대로 복원(재최소화, serve 가 실행했으면 종료 — `KAKAO_CLI_KEEP_KAKAO`
  로 무력화). one-shot(`doctor`)은 상태를 보고만 한다.
- **오프라인(캐시) 모드**: 카카오톡을 끝내 띄우지 못하면 TUI 가 종료하지 않고 마지막
  동기화분(방 목록·대화)을 읽기 전용으로 표시(`◐ · 캐시`). 전송 거부. `/rooms`·`/switch`
  가 라이브 재시도, 성공 시 자동 복귀. `db::cached_rooms` + 워커 폴백 + `UiEvent::Offline/Online`.
- **첫 실행 게이트 완화**: 카카오톡 미실행은 더 이상 TUI 를 막지 않는다(브리지가 실행/캐시로
  처리). 접근성 권한·앱 버전만 하드 게이트.
- 브리지: `AX.bool` 헬퍼(`CFBoolean` → Swift `Bool` 는 `as?` 로 안 됨), `KakaoApp.launch` /
  `waitUntilWindowReady`, `--wake` 디버그 커맨드.
- **넘을 수 없는 선**: 카카오톡이 설치·1회 로그인돼 있어야 한다(프로세스 없으면 AX 트리 없음).

### Changed — 채팅 TUI 탐색 방식

- **방 목록이 첫 화면.** `kakao-cli` 실행 시 곧바로 방 목록을 불러와 보여 준다.
  `↑`·`↓` 로 방을 고르고 `Enter` 로 연다. 글자를 입력하면 이름으로 필터.
  더 이상 빈 화면에서 `/switch` 를 쳐서 시작하지 않아도 된다.
- **`Esc` = 한 단계 뒤로.** 채팅 화면에서 `Esc`(입력이 비어 있을 때) → 방 목록.
  방 목록에서 `Esc`(필터가 비어 있을 때) → 종료.
- `/rooms` 는 이제 방 목록 화면으로 전환한다(트랜스크립트에 목록을 찍지 않음).
  `/switch` 슬래시 명령과 동명 방 번호 선택 오버레이는 그대로.
- 공통부: `App` 에 `Screen{Rooms,Chat}` + 방 목록 상태(선택·필터), 이벤트 루프는
  변경이 있을 때만 다시 그린다(유휴 채팅이 프레임을 흘리지 않음 — SSH·pty 버퍼 대비).
- DB: `busy_timeout=5s` — `doctor` 나 두 번째 세션이 쓰기 락을 잠깐 쥐어도 실패 대신 대기.

### Fixed

- **접근성 권한 온보딩**: `healthCheck` 가 권한 미부여 시 `AXIsProcessTrustedWithOptions(prompt:)`
  로 시스템 권한 요청 창을 띄운다 → 브리지 바이너리가 손쉬운 사용 목록에 자동 등록되어 사용자는
  토글만 켜면 된다 (`AX.requestTrust()` 가 정의만 돼 있고 호출되지 않던 것을 연결). `doctor` 와
  첫 실행 TUI 게이트 둘 다에서 발생. 복구 문구(`render.rs`, `docs/errors.md`, README)에 터미널 앱에
  직접 권한 주는 대안(brew 업그레이드에도 유지)과 "왜 두 경로가 다 되는지" 설명 추가.

### Added (이전)

- **Windows 어댑터 골격** (`adapters/windows`, Rust + windows-rs 0.58):
  - `kakao-contract` 크레이트를 직접 공유 — 타입·에러 코드 재구현 없음 (macOS는
    Swift라 재구현했지만 Windows는 Rust).
  - UIA COM 헬퍼(`uia.rs`): `IUIAutomation`, `RawViewWalker`, `ValuePattern`,
    `InvokePattern`, VARIANT/BSTR 변환, 서브트리 → `FixtureNode` 스냅샷.
  - 파서(`parsers.rs`) + 한국어 시각(`korean_time.rs`)은 플랫폼 독립 — macOS와
    **동일 fixture 시나리오로 파리티 테스트** (`cargo test`, 그린).
  - 5개 계약 함수, `--dump-tree` / `--self-test` 디버그 커맨드.
  - `cargo check --target x86_64-pc-windows-msvc` 통과 (macOS에서 타입 체크).
  - `kakao_app.rs`: `CreateToolhelp32Snapshot` 프로세스 탐색 + `GetFileVersionInfo`
    버전.
- `scripts/build-release.sh` 에 Windows 브랜치 추가.

### 알려진 제한 (Windows)

- **실제 KakaoTalk Windows에서 빌드·실행·검증 안 됨.** windows-rs API 타입만 확인.
- UIA 셀렉터(`selectors.rs`)는 전부 플레이스홀더 — 실제 `--dump-tree` 필요.
- 방 열기가 `Invoke` 로 되는지, 전송 버튼 동작, 메시지 목록 가상화 여부 미확인.
- Scoop manifest 는 릴리스 zip 대기 (초안).

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
