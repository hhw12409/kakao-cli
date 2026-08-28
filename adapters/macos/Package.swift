// swift-tools-version:5.9
import PackageDescription

// Note on tests: the parser regression suite lives in
// `Tests/kakao-macos-bridgeTests/` and uses swift-testing. Xcode's toolchain
// runs it with `swift test`; the bare Command Line Tools toolchain does not
// ship the Testing module search path, so a toolchain-independent equivalent
// is built into the executable as `kakao-macos-bridge --self-test`.
let package = Package(
    name: "kakao-macos-bridge",
    platforms: [.macOS(.v12)],
    targets: [
        .executableTarget(
            name: "kakao-macos-bridge",
            path: "Sources/kakao-macos-bridge",
            resources: [.copy("Fixtures")]
        )
    ]
)
