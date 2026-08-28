---
name: kakao-cli-docs
description: "kakao-cli의 사용자 대면 텍스트와 릴리스 문서를 작성한다. CLI 도움말(--help), '다음 행동이 보이는' 오류 메시지 카피(에러 코드 → 사용자 메시지 + 복구 명령), README(제품 정의·범위·설치·개인정보·비공식 도구 명시), CHANGELOG, macOS/Windows 패키징·배포 노트를 만들거나 갱신할 때 사용."
---

# kakao-cli 문서·릴리스

kakao-cli의 사용성 원칙 중 하나는 **"다음 행동이 보이는 오류를 쓴다"**. 이 카피가 그 원칙을 구현한다. 문서는 AI가 아니라 최종 사용자를 위한 것이다.

정본: `docs/kakao-cli-design.md` (제품 의도), `docs/adapter-contract.md` (에러 코드), `docs/command-spec.md` (명령 동작).

## 오류 메시지 카피 (`docs/errors.md`)

계약의 에러 코드 enum마다 표 한 줄:

| 코드 | 무엇이 잘못됐나 | 왜 | 지금 뭘 하면 되나 (실행할 명령) |
|------|---------------|-----|------------------------------|

**오류 메시지는 세 부분이다.** 세 번째(복구 명령)가 빠지면 원칙 위반.

예시:

```
ACCESSIBILITY_PERMISSION_DENIED
  카카오톡 창을 읽을 권한이 없습니다.
  → 시스템 설정 > 개인정보 보호 및 보안 > 손쉬운 사용에서 kakao-cli를 켜세요.
    권한을 준 뒤: kakao-cli doctor

KAKAO_NOT_RUNNING
  카카오톡이 실행 중이 아닙니다.
  → 카카오톡을 실행하고 로그인한 뒤 다시 시도하세요.

SEND_VERIFY_TIMEOUT (전송 결과 unknown)
  메시지를 보냈지만 카카오톡 화면에서 전송을 확인하지 못했습니다.
  → 카카오톡에서 직접 확인하세요. 자동으로 다시 보내지 않습니다.

ROOM_NOT_FOUND
  "개발팁"과 일치하는 채팅방이 없습니다.
  → kakao-cli rooms 로 방 목록을 확인하세요.
```

일반화: 카피는 특정 방 이름/버전에 하드코딩하지 말고 템플릿(플레이스홀더)으로 쓴다. 공통부가 값을 채운다.

## CLI 도움말

- `kakao-cli --help`: 한 화면. 가장 흔한 사용례(`send`, `inbox`, `open`)를 맨 위에.
- 서브커맨드 도움말: 인자·플래그·1~2개 예시. 튜토리얼이 아니라 리마인더.
- 안전 플래그의 목적을 분명히: `--exact`(제목 완전 일치), `--yes`(대화형 확인 건너뜀, 스크립트용), `--dry-run`(대상·메시지만 출력, 전송 안 함).
- `docs/command-spec.md`와 대조하여 플래그·인자·종료 코드가 실제와 맞는지 확인. 안 맞으면 지어내지 말고 core-engineer/cli-architect에게 확인.
- 원본은 `docs/help/`에 두고 core-engineer가 소스에 통합.

## README

섹션 순서:

1. **한 줄 정의** — "카카오톡 텍스트 채팅을 터미널에서 처리하는 macOS·Windows CLI"
2. **무엇을 하나** — inbox / rooms / open / send / search / doctor, `send` 중심 예시
3. **범위** — 제공/제외를 정직하게. 제외: 사진·파일·동영상·음성·이모티콘·답장·멘션·투표·공지·보이스톡·페이스톡·친구/프로필/결제. 이걸 숨기면 사용자가 실망한다
4. **설치** — macOS는 `brew install <user>/tap/kakao-cli`, Windows는 `scoop install`. 설치 후 `kakao-cli doctor` 로 접근성 권한 부여 (유일한 수동 단계)
5. **핵심 명령** — design 문서의 예시 흐름 재사용
6. **개인정보** — 방 이름·메시지·별칭은 로컬 SQLite에만. 텔레메트리에 메시지 본문 없음. AI 요약·자동 답장 없음. `kakao-cli cache clear`
7. **성격** — 개인용 로컬 도구. 카카오톡을 UI 자동화로 조작하며, 카카오 계정 정보·네트워크 프로토콜을 직접 다루지 않는다. 비공식 도구임을 명시
8. **로드맵** (선택) — 아직 없는 기능은 여기. 현재 기능과 섞지 않는다

## CHANGELOG

- Keep a Changelog 형식 (`Added`/`Changed`/`Fixed`/`Removed`).
- 구현 순서(macOS PoC → send 완성 → Windows 어댑터 → inbox/검색)를 버전 흐름에 반영.
- 새 버전 항목을 위에 추가하고 기존 항목은 보존.

## 패키징·배포 노트 (`docs/packaging.md`)

**제약: 유료 인증서·계정 없이 (Apple Developer Program 미사용), 그래도 `brew` 수준으로 편하게.** `docs/adr/0002-distribution.md` 참조.

- **1순위 = 비용 0, 2순위 = 사용자 친화.** 이 둘을 동시에 만족하는 길이 패키지 매니저 배포다.
- **macOS: Homebrew tap** — `brew install <user>/tap/kakao-cli`. tap은 GitHub 레포 하나(`homebrew-tap`)에 Ruby formula 파일 하나. formula가 **소스에서 빌드**(`cargo build --release` + `swift build -c release`)하면, 결과 바이너리는 "로컬 빌드"라 **Gatekeeper 차단도 quarantine도 없다** — 서명·공증 불필요. formula에 `depends_on "rust" => :build`. Swift는 Xcode Command Line Tools로 충분(대부분 brew 사용자가 이미 보유).
  - **ad-hoc 서명**(`codesign --force --sign - --identifier <id>`)은 무료. Gatekeeper엔 도움 안 되지만 안정적 식별자를 줘서 `brew upgrade` 후 접근성 권한이 덜 초기화됨. 빌드 단계에 포함.
  - 직접 다운로드(레포 릴리스 zip)한 유저만 Gatekeeper를 만난다 → README는 brew 설치를 권장 경로로, `doctor`가 quarantine을 감지하면 `xattr -dr com.apple.quarantine <path>` 안내.
  - 소스 빌드가 느린 게 부담이면 나중에 GitHub Actions로 bottle 빌드(이것도 무료) — 지금은 안 해도 됨.
- **Windows: Scoop bucket** — `scoop bucket add <user> <repo>; scoop install kakao-cli`. manifest는 GitHub 레포의 JSON 하나. Scoop이 받아서 shim으로 실행하면 SmartScreen 마찰이 적다. winget은 서명 요구가 늘어 $0 경로로는 Scoop이 낫다.
- **레이아웃**: 메인 실행 파일 + 브리지 바이너리를 함께 설치. 브리지는 `libexec/`(Homebrew)나 앱 디렉토리에 두고, 메인이 자기 실행 파일 기준 상대경로로 찾는다. 유저가 파일 두 개를 의식하지 않게.
- 남는 필수 단계는 **접근성 권한 부여 하나뿐** — `kakao-cli doctor`가 복사할 명령·설정 경로와 함께 안내.

## 이전 문서가 있을 때

`README.md`/`CHANGELOG.md`가 있으면 전면 재작성하지 않는다. 이번 변경분에 해당하는 섹션만 갱신. CHANGELOG는 새 버전 항목 추가, 기존 보존.

## 하지 않을 것

- 아직 구현 안 된 기능을 현재 기능처럼 문서화 → 로드맵으로.
- 코드와 다른 명령 예시를 추정으로 작성 → 확인 후.
- 개발 과정 메타 정보(팀 구성, 반복 이력)를 사용자 문서에 넣기.
