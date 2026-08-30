---
name: kakao-cli-orchestrator
description: "kakao-cli(카카오톡 데스크톱 앱을 접근성 API로 자동화하는 대화형 터미널 채팅 클라이언트, macOS·Windows) 개발 에이전트 팀을 조율한다. kakao-cli 기능 구현, 채팅 TUI 작업(방 이동·수신 표시·전송·슬래시 명령 /switch·/rooms·/alias), doctor 명령, macOS/Windows 어댑터 작업(serve 모드·watch 폴링), 어댑터 계약 변경, QA·통합 검증, 문서·릴리스 작업 요청 시 사용. 후속 작업: kakao-cli 결과 수정, 부분 재실행, 다시 실행, 업데이트, 보완, 이전 결과 기반 개선, 채팅 경험 개선, 특정 모듈만 다시 작업 요청 시에도 반드시 이 스킬을 사용."
---

# kakao-cli Orchestrator

kakao-cli 개발 에이전트 팀을 조율하여 카카오톡 데스크톱 앱 자동화 CLI를 설계·구현·검증·문서화한다.

kakao-cli는 카카오톡 네트워크 프로토콜을 재현하지 않는다. **`kakao-cli` 를 실행하면 터미널 안에
채팅 화면(TUI)이 열리는 대화형 클라이언트**다. 각 OS의 카카오톡 앱을 접근성 API로 조작하는 개인용
로컬 도구다. 설계 정본은 `docs/kakao-cli-design.md`, 계약은 `docs/adapter-contract.md`(v2.0.0),
전환 근거는 `docs/adr/0003-interactive-tui.md`.

**현재 표면:** `kakao-cli`(→ 채팅 TUI) + `kakao-cli doctor`. 그 밖의 서브명령은 없다.
방 이동·목록·별칭은 TUI 안의 슬래시 명령(`/switch`, `/rooms`, `/alias`).

## 실행 모드: 하이브리드

| Phase | 모드 | 이유 |
|-------|------|------|
| Phase 2 (설계) | 서브 에이전트 | cli-architect 단독 작업. 계약·스펙·ADR 산출 |
| Phase 3 (구현+QA) | 에이전트 팀 | 아키텍트·공통부·양 어댑터·QA가 계약을 실시간 조율. 경계면 불일치를 SendMessage로 즉시 해소 |
| Phase 5 (문서) | 서브 에이전트 | docs-writer 단독. 격리된 후속 작업 |

## 에이전트 구성

| 팀원 | 에이전트 타입 | 역할 | 스킬 | 출력 |
|------|-------------|------|------|------|
| cli-architect | cli-architect (커스텀) | 어댑터 계약(serve + one-shot)·명령/TUI 스펙·send 상태 머신·DB 스키마·ADR | adapter-contract | `docs/adapter-contract.md`, `docs/command-spec.md`, `docs/adr/`, `docs/db-schema.sql` |
| core-engineer | core-engineer (커스텀) | 공통부 구현 (채팅 TUI·워커 스레드·serve 클라이언트·방 해석·별칭·SQLite·send 정책) | cli-core-implementation | 레포 루트 공통부 소스 + 테스트 |
| macos-adapter-engineer | macos-adapter-engineer (커스텀) | macOS 어댑터 (Swift·Accessibility API·serve 모드·watch 폴러) | ui-automation-adapter | `adapters/macos/` |
| windows-adapter-engineer | windows-adapter-engineer (커스텀) | Windows 어댑터 (UIA·serve 모드), macOS와 패리티 | ui-automation-adapter | `adapters/windows/` |
| qa-inspector | qa-inspector (커스텀) | 계약 정합성(serve 프레이밍 포함)·플랫폼 패리티·send/watch 안전 불변식 교차 검증 | integration-coherence-qa | `_workspace/qa/qa-report-*.md` |
| docs-writer | docs-writer (커스텀) | 도움말·오류 카피·README·CHANGELOG·패키징 | kakao-cli-docs | `README.md`, `CHANGELOG.md`, `docs/errors.md`, `docs/packaging.md` |

모든 Agent / TeamCreate 호출에 `model: "opus"` 를 명시한다.

## 워크플로우

### Phase 0: 컨텍스트 확인

1. `_workspace/` 및 `docs/adapter-contract.md` 존재 여부 확인 (`docs/` 는 `.gitignore` 대상이라
   워킹카피에 없을 수 있음 — 그러면 소스 주석·ADR·CHANGELOG에서 재생성)
2. 실행 모드 결정:
   - **`docs/adapter-contract.md` 미존재 + 소스도 미완** → 초기 구축. Phase 1부터 전체 실행
   - **계약·소스 존재 + 사용자가 부분 수정 요청** (특정 모듈, 특정 어댑터, 특정 버그) → **부분 재실행**.
     해당 에이전트만 팀에 포함하고, 관련 산출물만 수정. Phase 2를 건너뛰고 Phase 3부터
   - **계약·소스 존재 + 아키텍처/계약 대변경 요청** → 기존 `_workspace/`를
     `_workspace_{YYYYMMDD_HHMMSS}/`로 이동 후 Phase 1부터. 기존 소스는 보존하고 diff 기반 수정
3. 부분 재실행 시: 이전 산출물 경로를 각 에이전트 프롬프트에 넣어 "기존 결과를 읽고 이 피드백만 반영"하도록 지시

### Phase 1: 준비

1. 사용자 요청 분석 — 어떤 모듈/어댑터/계층에 대한 작업인지 파악
2. `docs/kakao-cli-design.md`, `docs/adr/0003-interactive-tui.md` 를 읽어 범위·안전 정책·폴링 모델 확인
3. `_workspace/` 생성. `.gitignore` 에 `docs/`·`_workspace/` 포함 확인 — **로컬에만 유지**
4. `_workspace/00_input/request.md` 에 요청 요약 저장

### Phase 2: 설계 (서브 에이전트)

**초기 구축 또는 계약 대변경 시에만.** 부분 재실행은 건너뛴다.

1. `Agent(subagent_type: "cli-architect", model: "opus", prompt: "...")` 단독 호출:
   - `docs/kakao-cli-design.md` 와 `adapter-contract` 스킬을 근거로 다음을 산출/갱신:
     - `docs/adapter-contract.md` (v2.0.0) — 인터페이스 함수의 입출력 shape, 필드명 규칙(JSON=camelCase),
       `roomId` 불투명 문자열, 에러 코드 enum, **serve 프레이밍(§5): 요청/응답(id 상관)/이벤트
       (message·roomClosed·error), watch/dedup 의미**, one-shot 프레이밍(§1, doctor·self-test 전용),
       send 상태 머신(불변)
     - `docs/command-spec.md` — `kakao-cli`(→TUI: 키·슬래시 명령·상태줄·방 선택 오버레이) + `doctor`.
       종료 코드
     - `docs/adr/0001-tech-stack.md` — **스택 확정** (공통부 Rust / macOS Swift / Windows Rust /
       툴체인 2개·런타임 0). **IPC = 서브프로세스로 확정** (계약 §6). 언어·IPC 재평가 안 함
     - `docs/db-schema.sql` — 캐시·별칭·전송 로그 DDL. **FTS5 없음** (search 명령 제거, DB v2)
     - `docs/adr/0002-distribution.md` — Homebrew tap(소스 빌드, ad-hoc 서명) + Scoop. 공증·유료 계정 미사용
     - `docs/adr/0003-interactive-tui.md` — 대화형 전환 + serve 모드 브리지 근거 + 폴링·포커스 탈취 트레이드오프
2. 스택·배포·IPC가 확정되어 있으므로 별도 승인 게이트 없이 Phase 3로 진행한다.

### Phase 3: 구현 + 점진적 QA (에이전트 팀)

1. 팀 생성:
   ```
   TeamCreate(
     team_name: "kakao-cli-build",
     members: [
       { name: "cli-architect", agent_type: "cli-architect", model: "opus",
         prompt: "docs/adapter-contract.md(v2.0.0) 의 소유자. serve 프레이밍·one-shot·send 상태 머신을 관리한다. 어댑터 엔지니어의 현실 제약 피드백을 받아 계약을 수정하고 양쪽에 브로드캐스트한다." },
       { name: "core-engineer", agent_type: "core-engineer", model: "opus",
         prompt: "계약·command-spec 기반으로 공통부(채팅 TUI + 워커 스레드 + serve 클라이언트)를 구현한다. MockStreamAdapter 로 카카오톡 없이 병렬 개발. 완료 모듈마다 qa-inspector에게 알림." },
       { name: "macos-adapter-engineer", agent_type: "macos-adapter-engineer", model: "opus",
         prompt: "계약대로 macOS 어댑터를 Swift/Accessibility API로. serve 모드(watch 1.5초 폴러, AX 단일 락 직렬화)를 구현. 출력 예시 JSON을 windows-adapter-engineer와 공유해 패리티 유지." },
       { name: "windows-adapter-engineer", agent_type: "windows-adapter-engineer", model: "opus",
         prompt: "계약대로 Windows 어댑터를 UIA로. serve 프로토콜을 macOS와 바이트 단위로 동일하게. 라이브 UIA는 후속이면 프로토콜 파리티·스텁까지. 계약 해석은 macos-adapter-engineer와 실시간 동기화." },
       { name: "qa-inspector", agent_type: "qa-inspector", model: "opus",
         prompt: "각 모듈 완성 직후 계약 정합성(serve 프레이밍 포함)·플랫폼 패리티·send/watch 안전 불변식을 양쪽 코드를 동시에 읽고 교차 검증. 발견은 파일:라인 + 기대 vs 실제로 즉시 통지." }
     ]
   )
   ```
   > 부분 재실행 시: 관련 에이전트만 포함.

2. 작업 등록 (`TaskCreate`), 의존성 포함 (초기 구축 기준 — 부분 재실행은 범위에 맞게):
   ```
   - "계약 확정/갱신 (serve 프레이밍 포함)" (cli-architect)
   - "공통부: CLI 표면 (인자 없음 → TUI / doctor)" (core-engineer)
   - "공통부: serve 클라이언트 + StreamAdapter + MockStreamAdapter" (core-engineer)
   - "공통부: 어댑터 워커 스레드 (Job/UiEvent, AX I/O 직렬화)" (core-engineer)
   - "공통부: 채팅 TUI (app·ui·input, 슬래시 명령, 방 선택 오버레이)" (core-engineer)
   - "공통부: send 통합 (send_in_room, Enter=확인) + 상태 머신 + send_log" (core-engineer)
   - "공통부: 방 해석 (resolve_in_list, 동명 방 자동 선택 없음) + 별칭" (core-engineer)
   - "공통부: SQLite 스크롤백 캐시 (DB v2, FTS 없음)" (core-engineer)
   - "공통부: 첫 실행 온보딩 — 권한 미부여/브리지 미발견 시 doctor 수준 안내" (core-engineer)
   - "배포: Homebrew formula + Scoop manifest, 브리지 경로 해결 (serve 동일 바이너리)" (core-engineer + 어댑터)
   - "macOS 어댑터: one-shot 5함수 + serve 모드 (watch 폴러, message/roomClosed/error 이벤트)" (macos-adapter-engineer)
   - "macOS 어댑터: 임베디드 대화 pane 하드닝 (라이브 AX 덤프 필요)" (macos-adapter-engineer)
   - "Windows 어댑터: serve 프로토콜 파리티 + one-shot, 라이브 UIA (범위에 따라)" (windows-adapter-engineer)
   - "QA: 계약 정합성 (serve 프레이밍 셰이프, 모듈별)" (qa-inspector)
   - "QA: macOS↔Windows serve 프로토콜 패리티" (qa-inspector)
   - "QA: send/watch 안전 불변식 전수" (qa-inspector)
   ```
   > 구현 순서: 공통부 TUI(mock 구동) → macOS serve → macOS 하드닝 → Windows.

3. **실행 방식 — 팀원 자체 조율:** cli-architect 계약 공유 → 어댑터 피드백 → 계약 갱신 브로드캐스트.
   core-engineer는 MockStreamAdapter 로 TUI 병렬 개발. 두 어댑터 엔지니어는 serve 요청/응답/이벤트
   예시 JSON을 SendMessage로 맞춘다. 모듈 완성 시 qa-inspector 즉시 교차 검증.

4. **리더 모니터링:** 팀원 유휴 알림 → 다음 작업. qa-inspector 리포트 수집. 계약 교착 시 cli-architect 주재.

### Phase 4: 통합 검증

1. 모든 작업 완료 확인
2. qa-inspector 최종 전수 검증:
   - `docs/adapter-contract.md` §5 serve 프레이밍 ↔ 공통부 `ServeAdapter`/워커 ↔ 양 브리지 serve 디스패치
   - macOS ↔ Windows serve 요청/응답/이벤트 셰이프 대조 (동일 크레이트 타입)
   - send 안전 불변식: **동명 방 자동 선택 없음** (오버레이 기본 선택 없음) / **Enter = 명시 확인** /
     `unknown` 은 표시만·재전송 없음 / watch dedup은 브리지 소유(core는 append만)
   - 메시지 본문이 stderr·로그·send_log 외로 새지 않음
3. `_workspace/qa/qa-final.md` 생성 (통과/실패/미검증 구분)
4. 미해결 실패는 1회 수정 요청 후 재검증. 재실패 시 보고서에 명시

### Phase 5: 문서 (서브 에이전트)

1. 팀 정리 (`TeamDelete`)
2. `Agent(subagent_type: "docs-writer", model: "opus", prompt: "...")`:
   - `docs/errors.md` — 에러 코드별 사용자 메시지 + 복구. TUI 맥락 문구 포함
   - `README.md` — 대화형 채팅 클라이언트 정의·범위·`brew`/`scoop` 설치·실행법·키/슬래시 명령·개인정보·비공식 명시
   - `CHANGELOG.md` — 이번 변경분
   - `docs/help/` — `--help` 텍스트
   - `docs/packaging.md` — Homebrew formula 초안(소스 빌드 + ad-hoc 서명 + 브리지 `libexec/`, serve 동일 바이너리), Scoop 초안. **공증·유료 계정 언급 금지**
3. 문서의 예시가 `docs/command-spec.md` 와 일치하는지 확인

### Phase 6: 정리 및 보고

1. `_workspace/` 보존
2. 사용자에게 요약: 무엇을 구현/수정했나, QA 통과/미해결, 다음 권장 단계
3. **CLAUDE.md 변경 이력 갱신**
4. 사용자에게 피드백 요청

## 데이터 흐름

```
[리더] → Agent(cli-architect) → docs/adapter-contract.md(v2.0.0 serve+one-shot) + command-spec + ADR 0003
                                        │
         → TeamCreate(kakao-cli-build)
              cli-architect ←SendMessage→ macos-adapter-engineer ←SendMessage→ windows-adapter-engineer
                    │                            │                        │
              (계약 갱신)          adapters/macos/ (serve)      adapters/windows/ (serve)
                    │                            │                        │
              core-engineer ── MockStreamAdapter ┴── serve 예시 JSON ──────┘
              (tui/ + worker + ServeAdapter)      │
                    │                            │
              공통부 소스 ←──── qa-inspector (양쪽 동시 읽기, incremental) ────→ _workspace/qa/
                    │
         → TeamDelete → Agent(docs-writer) → README / errors.md / CHANGELOG
                    │
              [리더: 보고 + CLAUDE.md 이력 갱신]
```

## 파일 경로 규칙

- 중간 산출물: `_workspace/{phase}_{agent}_{artifact}.{ext}`
- QA 리포트: `_workspace/qa/qa-report-{모듈}.md`
- 프로젝트 정본 문서: `docs/` — **`.gitignore` 대상. 로컬에만 유지**
- 최종 산출물(커밋 대상): 공통부 소스, `adapters/`, `README.md`, `CHANGELOG.md`, `.gitignore`, `CLAUDE.md`, `.claude/`

## 에러 핸들링

| 상황 | 전략 |
|------|------|
| cli-architect가 ADR 못 정함 | 2~3안을 사용자에게 제시하고 선택받은 뒤 진행 |
| 팀원 1명 실패/중지 | 리더가 유휴 알림 감지 → SendMessage로 상태 확인 → 재시작 또는 작업 재할당 |
| 어댑터 엔지니어 간 계약 해석 충돌 | cli-architect가 3자 SendMessage로 주재, 결론을 계약 파일에 반영 |
| 한쪽 OS 환경/권한 부재로 동적 검증 불가 | 정적 교차 비교로 가능한 만큼, 나머지는 "미검증 — 환경 필요"로 명시 (통과 처리 금지) |
| QA 이슈 2회 미반영 | 리더가 개입, 최종 보고서에 미해결로 명시 |

## 테스트 시나리오

### 정상 흐름 (대화형 전환 후 기능 추가)
1. 사용자: "채팅 화면에서 메시지에 시간 옆에 요일도 보이게 해줘"
2. Phase 0: 계약·소스 존재 + 부분 수정 → 부분 재실행. core-engineer + qa-inspector만
3. Phase 3: core-engineer가 `tui/ui.rs` 렌더만 수정 → `MockStreamAdapter` + `tui_smoke` 로 확인
4. qa-inspector: 상태줄·트랜스크립트 렌더 회귀, send 불변식 무영향 확인
5. Phase 6: CLAUDE.md 이력에 기록

### 에러 흐름 (계약 대변경 중 환경 제약)
1. 사용자: "수신을 폴링 말고 더 빠르게 바꿔줘"
2. Phase 0: 아키텍처 대변경 → `_workspace/` 백업 후 Phase 1
3. Phase 2: cli-architect가 serve 이벤트 캐던스/방식 재설계, ADR 0003 갱신
4. Phase 3: 양 어댑터가 watch 폴러 수정. macOS는 실기기 권한 없어 라이브 검증 불가
5. Phase 4: qa-inspector가 프로토콜 셰이프는 검증, 라이브 캐던스는 "미검증 — 권한 필요"로 명시
6. Phase 6: 미검증 항목을 보고서·CLAUDE.md에 명시하고 진행
