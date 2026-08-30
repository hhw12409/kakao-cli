# kakao-cli

**카카오톡을 터미널에서 쓰는 대화형 채팅 클라이언트** (macOS · Windows).

`kakao-cli` 를 실행하면 터미널 안에 채팅 화면이 열린다. 방을 고르고, 상대의 메시지를
받아 보고, 입력해서 보낸다. 카카오톡 네트워크 프로토콜을 재현하지 않는다 — 각 OS에
설치된 카카오톡 데스크톱 앱을 접근성 API로 조작하는 개인용 로컬 도구다.

> **상태: 개발 중.** 공통부 TUI + serve 모드 브리지는 구현·검증됨(목 어댑터). macOS
> 실기기 메시지 흐름 검증과 Windows 라이브 구현은 진행 중. 로드맵 참조.

## 빠른 시작

1. 카카오톡 데스크톱 앱을 실행하고 로그인한다. **창을 최소화하지 않는다.**
2. `kakao-cli` 를 설치한다 (아래 [설치](#설치)).
3. `kakao-cli doctor` 로 권한을 확인하고, 안내대로 접근성(자동화) 권한을 준다.
4. `kakao-cli` 를 실행한다 → 채팅 화면이 열린다.
5. `/switch 가족` 처럼 방을 고르고, 메시지를 입력해 Enter.

## 쓰는 법

```bash
kakao-cli            # 채팅 화면을 연다
kakao-cli doctor     # 카카오톡 실행 상태와 자동화 권한 진단
```

채팅 화면 안에서:

```
<메시지> Enter              현재 방으로 전송
/switch <이름|@별칭>        방 이동 (동명이면 번호로 선택)
/rooms                     방 목록
/alias add dev 개발팀       별칭 추가   ·   /alias list   ·   /alias rm dev
/help                      명령 목록
/quit                      종료
PgUp / PgDn                스크롤       ·   Ctrl-C  종료
```

```
 ● kakao-cli — 가족
────────────────────────────────────────────
 [18:00] 엄마   저녁 뭐 먹지
 [18:01] 나     치킨이요
────────────────────────────────────────────
 ✓ 전송됨 18:02
┌ 입력 ─────────────────────────────────────┐
│ > 배고파                                  │
└───────────────────────────────────────────┘
```

## 어떻게 동작하나

카카오톡은 새 메시지 이벤트를 노출하지 않는다. 그래서 브리지가 현재 방의 접근성
트리를 **1.5초 간격으로 폴링**해 새 메시지를 화면에 밀어 준다 (체감 지연 1~2초).

- 카카오톡 창을 **최소화하지 말 것** — 폴링이 트리를 읽어야 한다.
- 정상 상태 폴링은 포커스를 뺏지 않는다. 단 `/switch` 와 **매 전송**은 카카오톡 창을
  잠깐 앞으로 올린다 (접근성 방식의 본질적 제약).

## 오배송 방지

- `/switch` 가 **여러 방**과 일치하면 번호 선택을 요구한다. 기본 선택값이 없다.
- 전송 확인은 **Enter** — 활성 방이 정해져 있고 직접 입력했으므로.
- 전송 결과가 불확실하면 `? 전송 확인 불가` 로 표시하고 **다시 보내지 않는다.**

## 범위

**제공**: 대화형 채팅(단일 활성 방, 수신 표시, 전송), 방 이동·목록·별칭, 환경 진단.

**제외**: 여러 방 동시 표시, 사진·파일·이모티콘(수신 시 "표시 불가"로만), 메시지 검색,
inbox 요약, 읽음 처리, 알림, 그룹/프로필/결제, 카카오 계정·세션·네트워크 프로토콜.

## 설치

### macOS — Homebrew tap

```bash
brew install --HEAD hhw12409/tap/kakao-cli   # 릴리스 태그 전까지
kakao-cli doctor
```

formula 가 소스에서 빌드한다 (`cargo` + `swift`). 로컬 빌드라 Gatekeeper 차단창이
없다. (`packaging/homebrew/kakao-cli.rb`)

### Windows — Scoop (진행 중)

```powershell
scoop bucket add hhw12409 https://github.com/hhw12409/scoop-bucket
scoop install kakao-cli
```

### 설치 후: 접근성 권한 (유일한 수동 단계)

`kakao-cli doctor` 가 안내한다. macOS: 시스템 설정 → 개인정보 보호 및 보안 → 손쉬운
사용에서 `kakao-cli` 를 켠다.

### 소스에서

필요: Rust(`rustup`), Swift(Xcode Command Line Tools).

```bash
git clone https://github.com/hhw12409/kakao-cli && cd kakao-cli
./scripts/build-release.sh dist     # cargo + swift 빌드 + ad-hoc 서명 + 레이아웃
dist/bin/kakao-cli doctor
dist/bin/kakao-cli                  # 채팅 화면
```

개발 빌드 (브리지가 keg에 없으므로 경로를 알려준다):

```bash
cargo build
swift build --package-path adapters/macos
KAKAO_CLI_BRIDGE_PATH=adapters/macos/.build/debug/kakao-macos-bridge \
  ./target/debug/kakao-cli

# 카카오톡 없이 UI만 확인 (목 어댑터):
KAKAO_CLI_STREAM_MOCK=crates/kakao-core/tests/fixtures/chat.json \
  ./target/debug/kakao-cli

cargo test --workspace                       # 전체 테스트
swift run --package-path adapters/macos kakao-macos-bridge --self-test
```

## 개인정보

- 방 이름 · 최근 메시지 · 별칭 · 전송 기록은 **로컬 SQLite** 에만 저장한다
  (macOS `~/Library/Application Support/kakao-cli/`, Windows `%APPDATA%\kakao-cli\`).
- 텔레메트리 · 로그 · 오류 리포트에 **메시지 본문을 넣지 않는다.** (전송 로그의 본문은
  로컬 감사용으로 그 DB에만 있고 전송되지 않는다.)
- 어떤 서버로도 데이터를 전송하지 않는다. AI 요약 · 자동 답장은 범위 밖이다.

## 성격

개인용 로컬 도구. 카카오톡 데스크톱 앱을 OS 접근성 API(macOS Accessibility API /
Windows UI Automation)로 조작하는 **비공식** 자동화다. 카카오와 무관하다.

## 로드맵

- macOS 임베디드 대화 pane 하드닝 (라이브 AX 덤프 필요)
- macOS 실기기 메시지 흐름 검증 (수신·전송)
- Windows serve 모드 라이브 UIA 구현
- 의도한 방에 30회 연속 정확 전송 (완료 기준)

## 라이선스

MIT.
