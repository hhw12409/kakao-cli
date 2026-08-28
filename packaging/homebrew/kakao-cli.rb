# Homebrew formula for kakao-cli.
#
# This file belongs in a tap repo: github.com/hhw12409/homebrew-tap
# at Formula/kakao-cli.rb. Kept here in the main repo as the source of truth.
#
# Until a release is tagged, install from git:
#   brew install --HEAD hhw12409/tap/kakao-cli
#
# After tagging v0.1.0, fill in `url` + `sha256` (from `brew fetch`) and drop
# `--HEAD`.
#
# Builds from source on the user's machine (docs/adr/0002-distribution.md):
# zero cost, no Gatekeeper prompt (local build => no quarantine), no
# notarization. A free ad-hoc signature gives a stable TCC identity.

class KakaoCli < Formula
  desc "카카오톡 텍스트 채팅을 터미널에서 처리하는 CLI"
  homepage "https://github.com/hhw12409/kakao-cli"
  license "MIT"
  head "https://github.com/hhw12409/kakao-cli.git", branch: "main"

  # stable do
  #   url "https://github.com/hhw12409/kakao-cli/archive/refs/tags/v0.1.0.tar.gz"
  #   sha256 "..."
  #   version "0.1.0"
  # end

  depends_on "rust" => :build
  depends_on :macos
  depends_on xcode: :build

  def install
    system "cargo", "build", "--release", "--locked", "-p", "kakao-core"

    # Universal build needs full Xcode; fall back to native arch otherwise.
    args = %w[build -c release --package-path adapters/macos]
    universal = %w[--arch arm64 --arch x86_64]
    if quiet_system("swift", *args, *universal)
      system "swift", *args, *universal
      bin_dir = Utils.safe_popen_read("swift", *args, *universal, "--show-bin-path").strip
    else
      system "swift", *args
      bin_dir = Utils.safe_popen_read("swift", *args, "--show-bin-path").strip
    end

    bin.install "target/release/kakao-cli"
    (libexec/"kakao-cli").install "#{bin_dir}/kakao-macos-bridge"

    # Free ad-hoc signature with a stable identifier — keeps the macOS
    # accessibility (TCC) grant across `brew upgrade`.
    system "codesign", "--force", "--sign", "-",
           "--identifier", "com.hhw12409.kakao-cli", bin/"kakao-cli"
    system "codesign", "--force", "--sign", "-",
           "--identifier", "com.hhw12409.kakao-cli.macos-bridge",
           libexec/"kakao-cli/kakao-macos-bridge"
  end

  def caveats
    <<~EOS
      kakao-cli needs macOS Accessibility permission to drive the KakaoTalk app.
      Run this and follow the guidance:

        kakao-cli doctor

      Then enable kakao-cli in:
        System Settings → Privacy & Security → Accessibility
    EOS
  end

  test do
    assert_match "kakao-cli", shell_output("#{bin}/kakao-cli --help")
    # doctor exits non-zero when KakaoTalk isn't running / permission absent;
    # just check it produces the checklist rather than crashing.
    output = shell_output("#{bin}/kakao-cli doctor 2>&1", 4)
    assert_match(/카카오톡 실행|접근성 권한/, output)
  end
end
