# kakao-cli

카카오톡 텍스트 채팅을 터미널에서 처리하는 macOS·Windows CLI.

카카오톡 데스크톱 앱에 이미 로그인한 상태에서, 채팅방을 찾고 · 새 텍스트 메시지를
확인하고 · 텍스트를 보내는 흐름을 터미널에서 끝낸다. 카카오톡 네트워크 프로토콜을
재현하지 않는다 — 각 OS에 설치된 카카오톡 앱을 접근성 API로 조작하는 개인용 로컬
도구다.

> **상태: 개발 초기.** macOS 골격(공통부 + Swift 브리지)까지 구현됨. 실제 카카오톡
> UI 연동(접근성 셀렉터)은 검증 진행 중이고, Windows 어댑터는 아직 없다. 로드맵 참조.

## 무엇을 하나

```bash
kakao-cli inbox                          # 안 읽은 방 우선으로 목록
kakao-cli rooms "개발"                    # 방 이름 검색
kakao-cli open "개발팀"                   # 최근 텍스트 대화
kakao-cli send "개발팀" "배포 확인했습니다"  # 텍스트 한 줄 전송
kakao-cli search "배포"                   # 로컬 캐시에서 메시지 검색
kakao-cli doctor                         # 실행 상태·권한 진단
```

`send` 가 중심이다. 가장 흔한 행동을 한 줄로, 대신 오발송은 구조적으로 막는다:

```bash
kakao-cli send "엄마" "곧 도착해요"
cat release-notes.txt | kakao-cli send "개발팀" --stdin
kakao-cli alias add dev "개발팀" && kakao-cli send @dev "CI 통과" --yes
kakao-cli send "개발팀" "배포 완료" --dry-run
```

- **정확히 하나** 일치하는 방이면 즉시 전송.
- **여러 방** 이 일치하면 번호 선택을 요구한다. 기본 선택값이 없고, 자동 전송하지
  않는다. `--exact` 로 제목을 특정.
- **스크립트**: `--yes` 로 대화형 확인을 건너뛴다 (단 동명 방이 실제로 여럿이면
  `--yes` 여도 전송하지 않는다).
- **전송 결과 불명확**: `unknown` 을 반환하고 **다시 보내지 않는다.**

## 범위

### 제공

- 채팅방 목록과 안 읽은 메시지 수 (`inbox`, `rooms`)
- 방 이름 검색·열기, 최근 텍스트 메시지 조회 (`open`)
- 텍스트 메시지 전송 (`send` — `--stdin` / 편집기 모드 / `@별칭` / `--dry-run` / `--exact` / `--yes` / `--max-chars`)
- 로컬 캐시 기반 메시지 검색 (`search`, SQLite FTS5)
- 1:1 및 그룹 채팅
- macOS·Windows 동일 명령·출력 (Windows는 로드맵)

### 제외

사진 · 파일 · 동영상 · 음성 메시지 · 이모티콘 · 답장 · 멘션 · 투표 · 공지 · 캘린더
등 특수 메시지 · 보이스톡 · 페이스톡 · 친구/프로필/결제 · 카카오 계정/세션/네트워크
프로토콜.

## 설치

### macOS — Homebrew tap

```bash
brew install hhw12409/tap/kakao-cli
kakao-cli doctor
```

formula 가 소스에서 빌드한다 (`cargo` + `swift`). 최초 설치는 1~3분 걸린다. 로컬
빌드 바이너리라 Gatekeeper 차단창이 없다.

### Windows — Scoop (로드맵)

```powershell
scoop bucket add hhw12409 https://github.com/hhw12409/scoop-bucket
scoop install kakao-cli
```

### 설치 후: 접근성 권한 (macOS, 유일한 수동 단계)

`kakao-cli doctor` 가 안내한다. 시스템 설정 → 개인정보 보호 및 보안 → 손쉬운
사용에서 `kakao-cli` 를 켜면 된다.

### 소스에서 직접

```bash
git clone https://github.com/hhw12409/kakao-cli
cd kakao-cli
cargo build --release                                   # 공통부 -> target/release/kakao-cli
swift build -c release --package-path adapters/macos     # macOS 브리지
```

개발 실행 시 브리지 경로를 알려준다:
`KAKAO_CLI_BRIDGE_PATH=adapters/macos/.build/release/kakao-macos-bridge kakao-cli doctor`

## 개인정보

- 방 이름 · 최근 메시지 · 별칭 · 전송 기록은 **로컬 SQLite** 에만 저장한다
  (`~/Library/Application Support/kakao-cli/`).
- 텔레메트리 · 로그 · 오류 리포트에 **메시지 본문을 넣지 않는다.**
- 별칭은 어떤 서버로도 전송하지 않는다.
- AI 요약 · 자동 답장은 범위 밖이다. 구현하지 않는다.
- 캐시 삭제: `kakao-cli cache clear` (별칭 · 전송 기록은 유지).

## 성격

개인용 로컬 도구다. 카카오톡 데스크톱 앱을 OS 접근성 API(macOS Accessibility API /
Windows UI Automation)로 조작한다. 카카오 계정 정보 · 세션 · 네트워크 프로토콜을
직접 다루지 않는 **비공식** 자동화다.

## 로드맵

- 실제 카카오톡 접근성 트리에 맞춘 셀렉터 + 버전별 fixture (진행 중)
- 의도한 방에 30회 연속 정확 전송 (design 완료 기준)
- Windows 어댑터 (UI Automation, windows-rs)
- `inbox` 증분 동기화

## 라이선스

MIT.
