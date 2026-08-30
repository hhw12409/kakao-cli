---
name: kakao-cli-docs
description: "kakao-cli의 사용자 대면 텍스트와 릴리스 문서를 작성한다. CLI 도움말(--help), '다음 행동이 보이는' 오류 메시지 카피(에러 코드 → 사용자 메시지 + 복구 명령, TUI 맥락 문구 포함), README(대화형 채팅 클라이언트 정의·범위·설치·실행법·키/슬래시 명령·개인정보·비공식 명시), CHANGELOG, macOS/Windows 패키징·배포 노트를 만들거나 갱신할 때 사용."
---

# kakao-cli 문서·릴리스

kakao-cli는 **대화형 터미널 채팅 클라이언트**다 — `kakao-cli` 를 실행하면 채팅 화면(TUI)이 열린다.
사용성 원칙 중 하나는 **"다음 행동이 보이는 오류를 쓴다"**. 이 카피가 그 원칙을 구현한다.
문서는 AI가 아니라 최종 사용자를 위한 것이다.

정본: `docs/kakao-cli-design.md`(제품 의도), `docs/adr/0003-interactive-tui.md`(전환 근거),
`docs/adapter-contract.md`(에러 코드), `docs/command-spec.md`(TUI·doctor 동작).

## 오류 메시지 카피 (`docs/errors.md`)

계약의 에러 코드 enum마다 표 한 줄: **무엇이 잘못됐나 / 왜 / 지금 뭘 하면 되나(실행할 명령)**.
세 번째가 빠지면 원칙 위반.

예시:

```
ACCESSIBILITY_PERMISSION_DENIED
  kakao-cli 에 접근성 권한이 없습니다.
  → 시스템 설정 > 개인정보 보호 및 보안 > 손쉬운 사용에서 kakao-cli 를 켜세요.
    open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
    그다음: kakao-cli doctor

SEND_VERIFY_TIMEOUT (전송 결과 unknown)
  전송 여부를 확인할 수 없습니다.
  → 카카오톡에서 직접 확인하세요. 재전송하지 않았습니다.

ROOM_NOT_FOUND
  "개발팁"과 일치하는 방이 없습니다.
  → 채팅 화면에서 /rooms 로 방 목록을 확인하세요.
```

TUI 맥락 문구도 정리한다 (정본은 `tui/app.rs`):
- 방 선택 전 전송: `먼저 /switch <방> 으로 방을 선택하세요.`
- 동명 방: `여러 방이 일치합니다. 번호로 선택하세요 (Esc 취소):`
- watch 대상 대화가 닫힘: `카카오톡에서 대화가 닫혔습니다. /switch 로 다시 여세요.`
- 브리지 연결 끊김: `어댑터 연결이 끊겼습니다: <사유>` + `/quit 로 종료 후 다시 실행하세요.`

일반화: 카피는 방 이름/버전에 하드코딩하지 말고 플레이스홀더로. 공통부가 값을 채운다.

## CLI 도움말 (`docs/help/`)

표면이 둘뿐이라 짧다:

```
kakao-cli            채팅 화면을 연다
kakao-cli doctor     카카오톡 실행 상태와 자동화 권한을 진단한다
```

채팅 화면 안 키·슬래시 명령을 함께 안내: `<메시지> Enter` 전송 · `/switch` `/rooms` `/alias`
`/help` `/quit` · PgUp/PgDn 스크롤 · Ctrl-C 종료. 튜토리얼이 아니라 리마인더.
`docs/command-spec.md` 와 대조하여 실제와 맞는지 확인. 원본은 `docs/help/` 에 두고 core-engineer 가
소스에 통합.

## README

섹션 순서:

1. **한 줄 정의** — "카카오톡을 터미널에서 쓰는 대화형 채팅 클라이언트 (macOS · Windows)"
2. **쓰는 법** — `kakao-cli`(→ 채팅 화면) / `kakao-cli doctor`. 채팅 화면 안 키·슬래시 명령 + 레이아웃 그림
3. **어떻게 동작하나** — 카카오톡은 새 메시지 이벤트가 없어 브리지가 1.5초 폴링. **창을 최소화하지 말 것.**
   정상 폴링은 포커스 안 뺏음, 단 `/switch` 와 매 전송은 카카오톡 창을 잠깐 앞으로
4. **오배송 방지** — `/switch` 동명 방은 번호 선택(기본값 없음) / 확인은 Enter / `unknown` 은 재전송 안 함
5. **범위** — 제공(대화형 채팅·방 이동·별칭·진단) / 제외(여러 방 동시, 사진·파일·이모티콘, 검색, inbox, 읽음 처리, 알림, 계정/프로토콜). 정직하게
6. **설치** — macOS `brew install --HEAD hhw12409/tap/kakao-cli`, Windows `scoop install`. 설치 후 `kakao-cli doctor` 로 접근성 권한 (유일한 수동 단계). `KAKAO_CLI_STREAM_MOCK` 로 카카오톡 없이 UI 확인
7. **개인정보** — 방 이름·메시지·별칭·전송 로그는 로컬 SQLite에만. 텔레메트리에 메시지 본문 없음. 어떤 서버로도 전송 안 함. AI 요약·자동 답장 없음
8. **성격** — 개인용 로컬 도구. 접근성 API로 카카오톡 앱을 조작하는 **비공식** 자동화. 카카오와 무관
9. **로드맵** — 아직 없는 기능은 여기 (macOS 실기기 검증, 임베디드 pane 하드닝, Windows 라이브 UIA). 현재 기능과 섞지 않는다

## CHANGELOG

- Keep a Changelog 형식 (`Added`/`Changed`/`Fixed`/`Removed`).
- 0.2.0의 대화형 전환은 **BREAKING** — 제거된 서브명령, serve 모드, 계약 v2.0.0, DB v2를 명시하고
  0.1.0 스크립트 사용자를 위한 마이그레이션 노트(대체재 없음, 대화형 도구임) 포함.
- 새 버전 항목을 위에 추가하고 기존 항목은 보존.

## 패키징·배포 노트 (`docs/packaging.md`)

**제약: 유료 인증서·계정 없이, 그래도 `brew` 수준으로 편하게.** `docs/adr/0002-distribution.md` 참조.

- **macOS: Homebrew tap** — formula 가 **소스에서 빌드**(`cargo` + `swift build -c release`). 로컬
  빌드라 Gatekeeper·quarantine 없음. `depends_on "rust" => :build`. SwiftPM 샌드박스 충돌 →
  `--disable-sandbox` + 빌드트리 경로. **ad-hoc 서명**(무료, 안정적 식별자)으로 `brew upgrade` 후
  접근성 권한 유지. 브리지를 `libexec/kakao-cli/` 에.
- **Windows: Scoop bucket** — manifest JSON 하나. 현재 **DRAFT**: Windows serve 모드는 프로토콜
  파리티까지, 라이브 UIA 후속 → 릴리스 zip 링크 미정.
- **serve 모드의 영향: 없음.** 동일 브리지 바이너리의 `serve` 서브명령일 뿐. formula/manifest 변경 불필요.
- **카카오톡 창을 최소화하지 말 것** 안내를 caveats/notes 에 (watch 폴링이 접근성 트리를 읽어야 함).
- **공증·유료 계정·Developer ID·winget 언급 금지.**

## 이전 문서가 있을 때

`README.md`/`CHANGELOG.md` 가 있으면 전면 재작성하지 않는다. 이번 변경분 섹션만 갱신.
CHANGELOG 는 새 버전 항목 추가, 기존 보존.

## 하지 않을 것

- 아직 구현 안 된 기능(임베디드 pane 하드닝, Windows 라이브)을 현재 기능처럼 → 로드맵으로
- 코드와 다른 명령/키 예시를 추정으로 작성 → 확인 후
- 개발 과정 메타 정보(팀 구성, 반복 이력)를 사용자 문서에
- 제거된 서브명령(`send`/`rooms`/`open`/`inbox`/`search`/`alias`/`cache`)을 문서에 남기기
