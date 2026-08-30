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
3. `kakao-cli doctor` 를 실행 → 뜨는 권한 요청 창에서 **[시스템 설정 열기]** → 손쉬운 사용
   목록의 `kakao-cli` 토글 ON → **터미널을 껐다 켜고** 다시 `kakao-cli doctor` (전부 `✓`).
   자세히는 [접근성 권한](#설치-후-접근성자동화-권한--유일한-수동-단계).
4. `kakao-cli` 를 실행한다 → **방 목록**이 뜬다.
5. `↑`·`↓` 로 방을 고르고 `Enter` 로 연다. 메시지를 입력해 `Enter` 로 전송,
   `Esc` 로 방 목록으로 돌아간다.

## 쓰는 법

```bash
kakao-cli            # 채팅 화면을 연다
kakao-cli doctor     # 카카오톡 실행 상태와 자동화 권한 진단
```

실행하면 **방 목록**이 먼저 뜬다:

```
 ● kakao-cli — 채팅방
────────────────────────────────────────────
   가족   (2건)   저녁 뭐 먹지
 ▶ 개발팀
   개발 잡담
────────────────────────────────────────────
 ↑↓ 이동 · Enter 열기 · 글자 입력 = 검색 · Esc 종료
```

- `↑` / `↓` — 방 이동,  글자 입력 — 이름으로 필터,  `Enter` — 열기
- `Esc` — 필터 비우기 / (비어 있으면) 종료

방을 열면 채팅 화면:

```
 ● kakao-cli — 가족
────────────────────────────────────────────
 [18:00] 엄마   저녁 뭐 먹지
 [18:01] 나     치킨이요
────────────────────────────────────────────
 ✓ 전송됨 18:02
┌ 입력 (Esc = 방 목록) ─────────────────────┐
│ > 배고파                                  │
└───────────────────────────────────────────┘
```

```
<메시지> Enter              현재 방으로 전송
Esc                        방 목록으로 돌아가기
/switch <이름|@별칭>        방 바로 이동 (동명이면 번호로 선택)
/alias add dev 개발팀       별칭 추가   ·   /alias list   ·   /alias rm dev
/help                      명령 목록          ·   /quit  종료
PgUp / PgDn                스크롤             ·   Ctrl-C  종료
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

### 설치 후: 접근성(자동화) 권한 — 유일한 수동 단계

kakao-cli 는 카카오톡 **창을 접근성 API로 읽고 조작**한다. 그래서 macOS 손쉬운 사용
목록에 등록·허용이 필요하다. (Windows 는 보통 별도 부여 불필요 — 카카오톡을 관리자
권한으로 실행했다면 kakao-cli 도 같은 권한으로.)

**권장 — 시스템이 자동으로 등록해 준다:**

```bash
kakao-cli doctor
```

실행하면 *"kakao-cli 가 이 컴퓨터를 제어하려고 합니다"* 시스템 창이 뜬다.

1. **[시스템 설정 열기]** 클릭
2. **손쉬운 사용** 목록에서 방금 추가된 **`kakao-cli`** 항목의 토글을 **켠다**
3. **터미널 앱을 완전히 종료**(`⌘Q`)했다 다시 열고 → `kakao-cli doctor` 로 `✓` 확인

**창이 안 뜨거나 항목이 없으면 — 터미널 앱에 권한을 준다** (가장 확실, `brew upgrade`
후에도 유지):

```bash
open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
```

- 목록 아래 **`+`** → `⌘⇧G` → 쓰는 터미널 경로 입력 (예: `/Applications/iTerm.app`
  또는 `/System/Applications/Utilities/Terminal.app`) → 추가하고 **토글 켜기**
- 터미널 완전 종료 후 재실행 → `kakao-cli doctor`

> **원리:** 접근성 권한은 실제로 API를 호출하는 프로세스(`kakao-cli` 가 띄우는
> `kakao-macos-bridge`) 또는 그 부모 터미널 앱에 붙는다. 시스템 창은 브리지 바이너리를
> 자동 등록하고, 터미널에 주면 자식 프로세스가 물려받는다 — 둘 중 하나면 된다.

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
