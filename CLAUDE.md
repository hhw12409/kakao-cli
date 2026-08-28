# kakao-cli

카카오톡 데스크톱 앱을 OS 접근성 API로 자동화하는 macOS·Windows CLI. 설계 정본: `docs/kakao-cli-design.md`.

## 하네스: kakao-cli 개발

**목표:** 공통부(OS 독립)와 macOS/Windows 어댑터를 계약 기반으로 설계·구현·검증하여, 두 플랫폼에서 동일하게 동작하는 CLI를 만든다.

**기술 스택 (확정):** 공통부 = Rust, macOS 브리지 = Swift, Windows 브리지 = Rust(windows-rs). 툴체인 2개, 런타임 0. 근거는 `docs/adr/0001-tech-stack.md`(첫 실행 시 생성).

**배포 (확정):** 비용 0 우선 → brew 수준 편의 우선. macOS = Homebrew tap(소스 빌드, Gatekeeper 없음, ad-hoc 서명), Windows = Scoop bucket. 공증·유료 계정 미사용. 남는 수동 단계는 접근성 권한 부여 하나(`kakao-cli doctor`가 안내). 근거는 `docs/adr/0002-distribution.md`.

**트리거:** kakao-cli 관련 작업 — 기능 구현, 명령어 추가/수정, 어댑터 작업, 계약 변경, QA·통합 검증, 문서·릴리스, 그리고 이들의 부분 재실행/수정/보완 — 요청 시 `kakao-cli-orchestrator` 스킬을 사용하라. 단순 질문은 직접 응답 가능.

**핵심 리스크:** 경계면 불일치(공통부↔어댑터 계약 드리프트)와 플랫폼 갈라짐(macOS↔Windows 동작 차이). QA는 각 모듈 완성 직후 점진적으로 교차 검증한다.

**변경 이력:**
| 날짜 | 변경 내용 | 대상 | 사유 |
|------|----------|------|------|
| 2026-08-28 | 초기 구성 (에이전트 6, 스킬 6, 오케스트레이터) | 전체 | - |
| 2026-08-28 | 기술 스택 확정 (공통부 Rust / macOS Swift / Windows Rust) | orchestrator, cli-architect, core-engineer, windows-adapter-engineer, adapter-contract, ui-automation-adapter | 사용자 결정 — 안전성·단일 바이너리 우선안 채택 |
| 2026-08-28 | `.gitignore` 추가 — `docs/`·`_workspace/` 커밋 제외 | .gitignore, orchestrator | 사용자 결정 — 문서(설계 문서 포함)·중간 산출물은 로컬 유지, 커밋 내역에서 제외 |
| 2026-08-28 | 배포 방식 확정 (Homebrew tap + Scoop, 비용 0, 공증 미사용) | orchestrator, cli-architect, docs-writer, kakao-cli-docs, ui-automation-adapter, cli-core-implementation | 사용자 결정 — 1순위 비용 0, 2순위 brew 수준 편의 |
| 2026-08-29 | macOS 환경 초기 구현 (계약 v1.0.0, 공통부 Rust 골격, Swift 브리지, ADR 0001/0002, 문서) | crates/kakao-core, crates/kakao-contract, adapters/macos, docs/ | 사용자 요청 "macOS 환경 먼저 구성" — 설계+공통부 골격+macOS 어댑터 범위. git init + origin(github.com/hhw12409/kakao-cli). 팀 툴 부재로 오케스트레이터 계획을 단일 세션에서 직접 수행 |
