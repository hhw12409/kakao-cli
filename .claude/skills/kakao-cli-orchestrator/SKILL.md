---
name: kakao-cli-orchestrator
description: "kakao-cli(카카오톡 데스크톱 앱을 접근성 API로 자동화하는 macOS·Windows CLI) 개발 에이전트 팀을 조율한다. kakao-cli 기능 구현, 명령어 추가/수정(inbox·rooms·open·send·search·doctor·alias·cache), macOS/Windows 어댑터 작업, 어댑터 계약 변경, QA·통합 검증, 문서·릴리스 작업 요청 시 사용. 후속 작업: kakao-cli 결과 수정, 부분 재실행, 다시 실행, 업데이트, 보완, 이전 결과 기반 개선, send 경험 개선, 특정 명령만 다시 작업 요청 시에도 반드시 이 스킬을 사용."
---

# kakao-cli Orchestrator

kakao-cli 개발 에이전트 팀을 조율하여 카카오톡 데스크톱 앱 자동화 CLI를 설계·구현·검증·문서화한다.

kakao-cli는 카카오톡 네트워크 프로토콜을 재현하지 않는다. 각 OS의 카카오톡 앱을 접근성 API로 조작하는 개인용 로컬 도구다. 설계 정본은 `docs/kakao-cli-design.md`.

## 실행 모드: 하이브리드

| Phase | 모드 | 이유 |
|-------|------|------|
| Phase 2 (설계) | 서브 에이전트 | cli-architect 단독 작업. 계약·스펙·ADR 산출 |
| Phase 3 (구현+QA) | 에이전트 팀 | 아키텍트·공통부·양 어댑터·QA가 계약을 실시간 조율. 경계면 불일치를 SendMessage로 즉시 해소 |
| Phase 5 (문서) | 서브 에이전트 | docs-writer 단독. 격리된 후속 작업 |

## 에이전트 구성

| 팀원 | 에이전트 타입 | 역할 | 스킬 | 출력 |
|------|-------------|------|------|------|
| cli-architect | cli-architect (커스텀) | 어댑터 계약·명령어 스펙·상태 머신·DB 스키마·기술 스택 ADR | adapter-contract | `docs/adapter-contract.md`, `docs/command-spec.md`, `docs/adr/`, `docs/db-schema.sql` |
| core-engineer | core-engineer (커스텀) | 공통부 구현 (CLI·출력·방 해석·별칭·SQLite·send 정책) | cli-core-implementation | 레포 루트 공통부 소스 + 테스트 |
| macos-adapter-engineer | macos-adapter-engineer (커스텀) | macOS 어댑터 (Swift·Accessibility API) | ui-automation-adapter | `adapters/macos/` |
| windows-adapter-engineer | windows-adapter-engineer (커스텀) | Windows 어댑터 (UIA), macOS와 패리티 | ui-automation-adapter | `adapters/windows/` |
| qa-inspector | qa-inspector (커스텀) | 계약 정합성·플랫폼 패리티·send 안전 불변식 교차 검증 | integration-coherence-qa | `_workspace/qa/qa-report-*.md` |
| docs-writer | docs-writer (커스텀) | 도움말·오류 카피·README·CHANGELOG·패키징 | kakao-cli-docs | `README.md`, `CHANGELOG.md`, `docs/errors.md`, `docs/packaging.md` |

모든 Agent / TeamCreate 호출에 `model: "opus"` 를 명시한다.

## 워크플로우

### Phase 0: 컨텍스트 확인

1. `_workspace/` 및 `docs/adapter-contract.md` 존재 여부 확인
2. 실행 모드 결정:
   - **`docs/adapter-contract.md` 미존재** → 초기 구축. Phase 1부터 전체 실행
   - **계약 존재 + 사용자가 부분 수정 요청** (특정 명령, 특정 어댑터, 특정 버그) → **부분 재실행**. 해당 에이전트만 팀에 포함하고, 관련 산출물만 수정. Phase 2를 건너뛰고 Phase 3부터, 필요 시 cli-architect만 계약 해당 섹션 갱신
   - **계약 존재 + 아키텍처/계약 대변경 요청** → 기존 `_workspace/`를 `_workspace_{YYYYMMDD_HHMMSS}/`로 이동 후 Phase 1부터. 기존 소스는 보존하고 diff 기반 수정
3. 부분 재실행 시: 이전 산출물 경로를 각 에이전트 프롬프트에 넣어 "기존 결과를 읽고 이 피드백만 반영"하도록 지시

### Phase 1: 준비

1. 사용자 요청 분석 — 어떤 명령/어댑터/계층에 대한 작업인지, 구현 순서(design 4단계) 중 어디인지 파악
2. `docs/kakao-cli-design.md` 를 읽어 범위·안전 정책·구현 순서를 확인
3. `_workspace/` 생성. 레포가 git이 아니면 `git init` (사용자 확인 후). `.gitignore`가 없으면 생성하고, 있으면 `_workspace/`·`docs/`가 포함되어 있는지 확인 — **`docs/`(계약·스펙·ADR·스키마)와 `_workspace/`는 커밋 내역에서 제외한다.** 로컬에만 유지되며 에이전트는 로컬 파일로 읽는다
4. `_workspace/00_input/request.md` 에 요청 요약 저장

### Phase 2: 설계 (서브 에이전트)

**초기 구축 또는 계약 대변경 시에만.** 부분 재실행은 건너뛴다.

1. `Agent(subagent_type: "cli-architect", model: "opus", prompt: "...")` 단독 호출:
   - `docs/kakao-cli-design.md` 와 `adapter-contract` 스킬을 근거로 다음을 산출:
     - `docs/adapter-contract.md` — 5개 인터페이스 함수의 입출력 shape, 필드명 규칙(JSON=camelCase), `roomId` 불투명 문자열 규칙, 에러 코드 enum, 프로세스 간 통신 형식, send 상태 머신
     - `docs/command-spec.md` — 명령별 인자·플래그·출력 형식·종료 코드
     - `docs/adr/0001-tech-stack.md` — **스택은 확정됨** (공통부 Rust / macOS Swift / Windows Rust(windows-rs) / 툴체인 2개·런타임 0). cli-architect는 이 결정을 ADR로 기록하고, 남은 세부(공통부↔Windows 브리지를 서브프로세스로 둘지 crate로 링크할지 — 계약의 IPC 추상화는 어느 쪽이든 유지)를 결정한다. 언어를 다시 평가하지 않는다
     - `docs/db-schema.sql` — 캐시·별칭·전송 로그·FTS5 DDL
     - `docs/adr/0002-distribution.md` — 배포 방식. **확정 제약: 유료 인증서·계정 없이(1순위), brew 수준으로 편하게(2순위).** macOS = Homebrew tap(소스 빌드 → Gatekeeper 없음, ad-hoc 서명), Windows = Scoop bucket. 공증·Developer ID·winget 미사용. cli-architect는 이 방향을 ADR로 기록하고 formula/manifest가 참조할 빌드 명령·설치 레이아웃(브리지 바이너리 위치)을 확정
2. 스택·배포가 확정되어 있으므로 별도 승인 게이트 없이 Phase 3로 진행한다. 단 cli-architect가 서브프로세스 vs crate 링크 등 아키텍처 세부에서 사용자 판단이 필요하다고 보면 그 지점만 확인받는다.

### Phase 3: 구현 + 점진적 QA (에이전트 팀)

1. 팀 생성:
   ```
   TeamCreate(
     team_name: "kakao-cli-build",
     members: [
       { name: "cli-architect", agent_type: "cli-architect", model: "opus",
         prompt: "docs/adapter-contract.md 의 소유자. 어댑터 엔지니어의 현실 제약 피드백을 받아 계약을 수정하고 양쪽에 브로드캐스트한다. 계약 해석 분쟁을 직접 주재한다." },
       { name: "core-engineer", agent_type: "core-engineer", model: "opus",
         prompt: "계약과 command-spec 기반으로 공통부를 구현한다. 어댑터 완성 전 목 응답으로 병렬 개발. 완료 모듈마다 qa-inspector에게 알림." },
       { name: "macos-adapter-engineer", agent_type: "macos-adapter-engineer", model: "opus",
         prompt: "계약대로 macOS 어댑터를 Swift/Accessibility API로 구현. 출력 예시 JSON을 windows-adapter-engineer와 공유하여 패리티 유지." },
       { name: "windows-adapter-engineer", agent_type: "windows-adapter-engineer", model: "opus",
         prompt: "계약대로 Windows 어댑터를 UIA로 구현. macOS 어댑터와 바이트 단위로 같은 출력·에러 코드. 계약 해석은 macos-adapter-engineer와 실시간 동기화." },
       { name: "qa-inspector", agent_type: "qa-inspector", model: "opus",
         prompt: "각 모듈 완성 직후 계약 정합성·플랫폼 패리티·send 안전 불변식을 양쪽 코드를 동시에 읽고 교차 검증. 발견은 파일:라인 + 기대 vs 실제로 해당 에이전트(들)에게 즉시 통지." }
     ]
   )
   ```
   > 부분 재실행 시: 관련 에이전트만 포함 (예: send 버그 → core-engineer + 해당 어댑터 + qa-inspector).

2. 작업 등록 (`TaskCreate`), 의존성 포함:
   ```
   - "계약 확정" (cli-architect) — Phase 2 산출물 팀 공유 및 피드백 반영
   - "공통부: CLI 파싱 + 디스패치" (core-engineer)
   - "공통부: 방 해석 + 별칭 + 동명 방 UX" (core-engineer)
   - "공통부: send 정책 + 상태 머신 + stdin/편집기/dry-run/yes" (core-engineer)
   - "공통부: SQLite 캐시 + FTS search + cache clear" (core-engineer)
   - "공통부: 첫 실행 온보딩 — 접근성 권한 미부여/브리지 미발견 시 스택트레이스 대신 `doctor` 수준 안내(복사할 명령 포함)로 폴백" (core-engineer)
   - "배포: Homebrew tap formula + Scoop manifest 초안, 브리지 바이너리 경로 해결" (core-engineer + 해당 어댑터, depends_on 빌드 산출물)
   - "macOS 어댑터: listRooms/openRoom/readRecent" (macos-adapter-engineer) depends_on 계약 확정
   - "macOS 어댑터: sendText + 전송 검증" (macos-adapter-engineer) depends_on 계약 확정
   - "macOS 어댑터: healthCheck + 권한 진단" (macos-adapter-engineer) depends_on 계약 확정
   - "Windows 어댑터: 동일 3작업, macOS 패리티" (windows-adapter-engineer) depends_on 계약 확정
   - "QA: 계약 정합성 (모듈별)" (qa-inspector) depends_on 각 모듈
   - "QA: macOS↔Windows 패리티" (qa-inspector) depends_on 양 어댑터 같은 함수
   - "QA: send 안전 불변식 전수" (qa-inspector) depends_on send 정책 + 양 어댑터 sendText
   ```
   > 구현 순서(design 문서 4단계)를 우선순위에 반영: macOS PoC → send 완성 → Windows → inbox/검색. 초기 구축이 아니면 요청 범위에 맞는 작업만 등록.

3. **실행 방식 — 팀원 자체 조율:**
   - cli-architect가 계약을 팀에 공유 → 두 어댑터 엔지니어가 구현 가능성 피드백 → 계약 수정 시 양쪽에 브로드캐스트
   - core-engineer는 계약 기반 목 응답으로 공통부를 병렬 개발
   - 두 어댑터 엔지니어는 출력 예시 JSON을 SendMessage로 주고받으며 필드·포맷·에러 코드 일치 확인
   - 각 모듈 완성 시 담당자가 파일 저장 + qa-inspector에게 알림 → qa-inspector가 즉시 교차 검증 (incremental QA)
   - qa-inspector 발견은 해당 에이전트(경계면 이슈는 양쪽)에게 직접. 계약 모호성이면 cli-architect에게

4. **리더 모니터링:**
   - 팀원 유휴 알림 수신 시 다음 작업 확인
   - qa-inspector 리포트를 수집하며 미해결 이슈 추적
   - 계약 해석 교착 시 cli-architect에게 주재 요청

### Phase 4: 통합 검증

1. 모든 작업 완료 확인 (`TaskGet`)
2. qa-inspector에게 최종 전수 검증 요청:
   - `docs/adapter-contract.md` ↔ 공통부 파싱부 전체 대조
   - macOS ↔ Windows 출력 diff (동일 fixture)
   - send 안전 불변식: 동명 방 자동 선택 없음 / 확인 전 재전송 없음 / 불명확 시 `unknown` / `--yes` 없이 대화형 확인 / `--dry-run` 시 `sendText` 미호출
   - 메시지 본문이 로그·텔레메트리에 없음
3. qa-inspector가 `_workspace/qa/qa-final.md` 생성 (통과/실패/미검증 구분)
4. 미해결 실패가 있으면 해당 에이전트에게 1회 수정 요청 후 재검증. 재실패 시 최종 보고서에 명시하고 진행

### Phase 5: 문서 (서브 에이전트)

1. 팀 정리 (`TeamDelete`)
2. `Agent(subagent_type: "docs-writer", model: "opus", prompt: "...")`:
   - `docs/adapter-contract.md` 의 에러 코드마다 `docs/errors.md` 에 사용자 메시지 + 복구 명령
   - `README.md` — 제품 정의·범위(제공/제외)·설치(`brew`/`scoop` 한 줄)·핵심 명령·개인정보·비공식 도구 명시
   - `CHANGELOG.md` — 이번 변경분
   - CLI 도움말 텍스트 (core-engineer가 소스 반영할 수 있도록 `docs/help/` 에)
   - `docs/packaging.md` — `docs/adr/0002-distribution.md` 기반. Homebrew tap formula 초안(소스 빌드 + ad-hoc 서명 + 브리지를 `libexec/`), Scoop manifest 초안, quarantine 안내. **공증·유료 계정 언급 금지**
3. 문서의 예시 명령이 `docs/command-spec.md` 와 일치하는지 확인

### Phase 6: 정리 및 보고

1. `_workspace/` 보존 (중간 산출물·QA 리포트는 삭제하지 않음)
2. 사용자에게 요약 보고: 무엇을 구현/수정했나, QA 통과/미해결 항목, 다음 권장 단계 (design 문서 구현 순서 기준)
3. **CLAUDE.md 변경 이력 갱신** — 날짜, 변경 내용, 대상, 사유
4. 사용자에게 피드백 요청: "팀 구성이나 워크플로우에서 바꾸고 싶은 점이 있나요?"

## 데이터 흐름

```
[리더] → Agent(cli-architect) → docs/adapter-contract.md + command-spec + ADR(확정 스택 기록)
                                        │
                         (IPC 방식 등 세부만 필요 시 확인)
                                        │
         → TeamCreate(kakao-cli-build)
              cli-architect ←SendMessage→ macos-adapter-engineer ←SendMessage→ windows-adapter-engineer
                    │                            │                        │
              (계약 갱신)                  adapters/macos/           adapters/windows/
                    │                            │                        │
              core-engineer ── 목 응답 ──────────┴── 계약 준수 샘플 ───────┘
                    │                            │
              공통부 소스 ←──── qa-inspector (양쪽 동시 읽기, incremental) ────→ _workspace/qa/
                    │
         → TeamDelete → Agent(docs-writer) → README / errors.md / CHANGELOG
                    │
              [리더: 보고 + CLAUDE.md 이력 갱신]
```

## 파일 경로 규칙

- 중간 산출물: `_workspace/{phase}_{agent}_{artifact}.{ext}` (예: `_workspace/03_qa_contract-check.md`)
- QA 리포트: `_workspace/qa/qa-report-{모듈}.md`
- 프로젝트 정본 문서: `docs/` (계약·스펙·ADR·스키마·에러·패키징) — **`.gitignore` 대상. 커밋하지 않고 로컬에만 유지**
- 최종 산출물(커밋 대상): 공통부 소스, `adapters/`, `README.md`, `CHANGELOG.md`, `.gitignore`, `CLAUDE.md`, `.claude/`
- `_workspace/` 는 사후 검증·감사 추적용으로 보존 (`.gitignore` 대상)

## 에러 핸들링

| 상황 | 전략 |
|------|------|
| cli-architect가 ADR 못 정함 | 2~3안을 사용자에게 제시하고 선택받은 뒤 진행 |
| 팀원 1명 실패/중지 | 리더가 유휴 알림 감지 → SendMessage로 상태 확인 → 재시작 또는 작업 재할당 |
| 어댑터 엔지니어 간 계약 해석 충돌 | cli-architect가 3자 SendMessage로 주재, 결론을 계약 파일에 반영 |
| 한쪽 OS 환경 부재로 동적 검증 불가 | 정적 교차 비교로 가능한 만큼 검증, 나머지는 "미검증 — 환경 필요"로 명시 (통과로 처리 금지) |
| QA 이슈 2회 미반영 | 리더가 개입, 최종 보고서에 미해결로 명시 |
| 팀원 간 데이터 충돌 | 출처 병기, 삭제하지 않음 |

## 테스트 시나리오

### 정상 흐름 (초기 구축)
1. 사용자: "kakao-cli 설계안대로 macOS PoC부터 시작해줘"
2. Phase 1: `docs/kakao-cli-design.md` 읽고 `_workspace/` + `git init`
3. Phase 2: cli-architect가 계약·스펙 산출 + 확정 스택(Rust/Swift/Rust)을 ADR로 기록
4. Phase 3: 5인 팀 구성. cli-architect 계약 공유 → 어댑터 피드백으로 `roomId`를 불투명 문자열로 확정 → core-engineer 목 개발 → macos-adapter-engineer가 listRooms 완성 → qa-inspector가 공통부 파싱부와 교차 검증, `{ rooms: [...] }` 래핑 불일치 발견 → core-engineer + cli-architect에게 통지 → 수정
5. Phase 4: 최종 전수 검증, `_workspace/qa/qa-final.md`
6. Phase 5: docs-writer가 README + errors.md
7. Phase 6: 보고 + CLAUDE.md 이력에 "초기 구성" 기록
8. 예상 결과: `docs/adapter-contract.md`, 공통부 소스, `adapters/macos/`, `adapters/windows/` 스텁, QA 리포트, README

### 에러 흐름 (부분 재실행 중 팀원 중지)
1. 사용자: "send 할 때 동명 방인데 1번이 기본 선택돼. 고쳐줘"
2. Phase 0: 계약 존재 + 부분 수정 → 부분 재실행. core-engineer + qa-inspector만 팀 구성
3. Phase 3: core-engineer가 방 해석부 수정 중 중지
4. 리더가 유휴 알림 수신 → SendMessage로 상태 확인 → 재시작
5. 재시작 후 수정 완료 → qa-inspector가 send 안전 불변식 재검증 (기본 선택값 없음 확인)
6. Phase 6: CLAUDE.md 이력에 "동명 방 기본 선택 버그 수정" 기록
7. 예상 결과: 방 해석부 패치 + 회귀 검증 통과 리포트
