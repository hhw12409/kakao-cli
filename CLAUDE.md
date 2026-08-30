# kakao-cli

카카오톡 데스크톱 앱을 OS 접근성 API로 자동화하는 **대화형 터미널 채팅 클라이언트** (macOS·Windows). `kakao-cli` 를 실행하면 채팅 화면(TUI)이 열린다. 서브명령은 `doctor` 하나. 설계 정본: `docs/kakao-cli-design.md`, 계약: `docs/adapter-contract.md`(v2.0.0), 전환 근거: `docs/adr/0003-interactive-tui.md`.

## 하네스: kakao-cli 개발

**목표:** 공통부(OS 독립, 채팅 TUI 포함)와 macOS/Windows 어댑터(serve 모드)를 계약 기반으로 설계·구현·검증하여, 두 플랫폼에서 동일하게 동작하는 대화형 클라이언트를 만든다.

**기술 스택 (확정):** 공통부 = Rust(TUI: ratatui+crossterm), macOS 브리지 = Swift, Windows 브리지 = Rust(windows-rs). 툴체인 2개, 런타임 0. IPC = 서브프로세스(serve 모드 = 장수 프로세스 + 줄 단위 JSON-RPC + 이벤트 스트림). 근거는 `docs/adr/0001-tech-stack.md`.

**배포 (확정):** 비용 0 우선 → brew 수준 편의 우선. macOS = Homebrew tap(소스 빌드, Gatekeeper 없음, ad-hoc 서명), Windows = Scoop bucket. 공증·유료 계정 미사용. 남는 수동 단계는 접근성 권한 부여 하나(`kakao-cli doctor`가 안내). 근거는 `docs/adr/0002-distribution.md`.

**트리거:** kakao-cli 관련 작업 — 기능 구현, 채팅 TUI·슬래시 명령(`/switch`·`/rooms`·`/alias`) 작업, `doctor`, 어댑터 작업(serve 모드·watch 폴링), 계약 변경, QA·통합 검증, 문서·릴리스, 그리고 이들의 부분 재실행/수정/보완 — 요청 시 `kakao-cli-orchestrator` 스킬을 사용하라. 단순 질문은 직접 응답 가능.

**핵심 리스크:** 경계면 불일치(공통부↔어댑터 계약 드리프트, 특히 serve 프레이밍)와 플랫폼 갈라짐(macOS↔Windows 동작 차이). QA는 각 모듈 완성 직후 점진적으로 교차 검증한다.

**변경 이력:**
| 날짜 | 변경 내용 | 대상 | 사유 |
|------|----------|------|------|
| 2026-08-28 | 초기 구성 (에이전트 6, 스킬 6, 오케스트레이터) | 전체 | - |
| 2026-08-28 | 기술 스택 확정 (공통부 Rust / macOS Swift / Windows Rust) | orchestrator, cli-architect, core-engineer, windows-adapter-engineer, adapter-contract, ui-automation-adapter | 사용자 결정 — 안전성·단일 바이너리 우선안 채택 |
| 2026-08-28 | `.gitignore` 추가 — `docs/`·`_workspace/` 커밋 제외 | .gitignore, orchestrator | 사용자 결정 — 문서(설계 문서 포함)·중간 산출물은 로컬 유지, 커밋 내역에서 제외 |
| 2026-08-28 | 배포 방식 확정 (Homebrew tap + Scoop, 비용 0, 공증 미사용) | orchestrator, cli-architect, docs-writer, kakao-cli-docs, ui-automation-adapter, cli-core-implementation | 사용자 결정 — 1순위 비용 0, 2순위 brew 수준 편의 |
| 2026-08-29 | macOS 환경 초기 구현 (계약 v1.0.0, 공통부 Rust 골격, Swift 브리지, ADR 0001/0002, 문서) | crates/kakao-core, crates/kakao-contract, adapters/macos, docs/ | 사용자 요청 "macOS 환경 먼저 구성" — 설계+공통부 골격+macOS 어댑터 범위. git init + origin(github.com/hhw12409/kakao-cli). 팀 툴 부재로 오케스트레이터 계획을 단일 세션에서 직접 수행 |
| 2026-08-31 | **대화형 채팅 클라이언트로 전환 (BREAKING, 계약 v2.0.0)** — one-shot 서브명령 전부 제거(doctor만 유지), 인자 없이 실행 시 TUI. serve 모드 브리지(장수 프로세스 + 줄 단위 JSON-RPC + 이벤트 스트림), 1.5초 폴링 수신, 동명 방 오버레이. 공통부 `tui/`·워커 스레드·`StreamAdapter`, DB v2(FTS 제거), macOS `Serve.swift`, Windows serve 파리티 스텁. ADR 0003. `docs/` 재생성(로컬 유실분 복원). | crates/kakao-core, crates/kakao-contract, adapters/macos, adapters/windows, docs/, README, CHANGELOG, packaging | 사용자 요청 — "터미널에서 자유롭게 채팅하는 대화형 도구". 이번 세션 범위: core TUI + macOS 브리지(serve). Windows 라이브 UIA·macOS 임베디드 pane 하드닝·실기기 메시지 흐름 검증은 후속. 팀 툴 부재로 단일 세션 직접 수행. Rust 툴체인 없어 rustup 설치 |
| 2026-08-31 | **하네스 동기화 (대화형 전환 반영)** — 6개 에이전트 + 6개 스킬 + 오케스트레이터를 계약 v2.0.0·serve 모드·채팅 TUI·슬래시 명령 기준으로 갱신. 옛 명령어(inbox·rooms·open·send·search·alias·cache) 참조 제거, FTS5·`--dry-run`/`--yes`/`--stdin`/편집기 모드 참조 제거, IPC "서브프로세스 vs crate 링크 미결정" → 서브프로세스 확정. 역할 구조·팀 구성은 유지(콘텐츠 동기화). | .claude/agents/*, .claude/skills/* | 사용자 요청 "harness 점검" — 코드/문서와 하네스 설명의 드리프트 해소 |
| 2026-08-31 | **채팅 TUI 탐색 UX** — 실행 시 방 목록 화면 먼저(↑↓ 탐색·글자 입력 필터·Enter 열기), 채팅에서 `Esc`→방 목록, 방 목록에서 `Esc`→종료. 이벤트 루프 dirty 기반 렌더. `/rooms`=목록 화면 전환. db `busy_timeout=5s`. 팀 툴 부재로 단일 세션 직접 수행. | crates/kakao-core/src/tui/*, db.rs, docs/command-spec.md, README, CHANGELOG | 사용자 요청 — "빈 화면에서 /switch 는 불편, 목록 보여주고 방향키로 접근" |
