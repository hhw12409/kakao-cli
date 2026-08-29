# Homebrew formula for kakao-cli.
#
# This file belongs in a tap repo: github.com/hhw12409/homebrew-tap
# at Formula/kakao-cli.rb. Kept here in the main repo as the source of truth.
#
# Install:
#   brew install hhw12409/tap/kakao-cli          # tagged release
#   brew install --HEAD hhw12409/tap/kakao-cli   # latest main
#
# Builds from source on the user's machine (docs/adr/0002-distribution.md):
# zero cost, no Gatekeeper prompt (local build => no quarantine), no
# notarization. A free ad-hoc signature gives a stable TCC identity.

class KakaoCli < Formula
  desc "카카오톡 텍스트 채팅을 터미널에서 처리하는 CLI"
  homepage "https://github.com/hhw12409/kakao-cli"
  url "https://github.com/hhw12409/kakao-cli/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "SHA256_PLACEHOLDER"
  license "MIT"
  head "https://github.com/hhw12409/kakao-cli.git", branch: "main"

  depends_on "rust" => :build
  depends_on :macos
  # Swift ships with the Command Line Tools, which Homebrew already requires —
  # no full Xcode dependency. (A universal bridge would need Xcode's xcbuild;
  # without it the build falls back to a native-arch binary, which is fine.)

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/kakao-core")

    # SwiftPM's own sandbox + manifest compilation clashes with Homebrew's
    # build sandbox, and it wants a writable ~/Library. Point every path at the
    # build tree and disable SwiftPM's sandbox.
    swift_home = buildpath/"swiftpm"
    ENV["HOME"] = swift_home
    args = %W[
      build -c release --package-path adapters/macos
      --disable-sandbox
      --scratch-path #{buildpath}/adapters/macos/.build
      --cache-path #{swift_home}/cache
      --config-path #{swift_home}/config
    ]
    # Universal build needs full Xcode; fall back to native arch otherwise.
    universal = %w[--arch arm64 --arch x86_64]
    if quiet_system("swift", *args, *universal)
      system "swift", *args, *universal
      bin_dir = Utils.safe_popen_read("swift", *args, *universal, "--show-bin-path").strip
    else
      system "swift", *args
      bin_dir = Utils.safe_popen_read("swift", *args, "--show-bin-path").strip
    end

    # `cargo install` already placed kakao-cli in bin/.
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
