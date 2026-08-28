import Foundation

/// `kakao-macos-bridge --self-test` — runs the parser regression checks against
/// the bundled accessibility-tree fixtures. Toolchain-independent stand-in for
/// the swift-testing suite in `Tests/` (which needs the Xcode toolchain).
///
/// Exit 0 = all passed, 1 = a check failed. Wire this into CI and run it after
/// adding a new per-version fixture.
enum SelfTest {
    private static var failures = 0

    private static func check(_ condition: Bool, _ label: String) {
        if condition {
            print("  ok   \(label)")
        } else {
            print("  FAIL \(label)")
            failures += 1
        }
    }

    static func run() -> Never {
        let selectors = SelectorMap.known["*"]!

        guard let url = Bundle.module.url(
            forResource: "scenario-basic",
            withExtension: "json",
            subdirectory: "Fixtures"
        ),
        let data = try? Data(contentsOf: url),
        let tree = try? JSONDecoder().decode(FixtureNode.self, from: data)
        else {
            FileHandle.standardError.write(Data("could not load scenario-basic fixture\n".utf8))
            exit(1)
        }

        print("scenario-basic — listRooms")
        let rooms = Parsers.rooms(in: tree, selectors: selectors)
        check(rooms.count == 3, "3 rooms parsed")
        check(rooms.map(\.title) == ["개발팀", "엄마", "개발 공지"], "titles in list order")
        check(rooms.map(\.roomId) == ["row:0", "row:1", "row:2"], "opaque roomIds")
        check(rooms[0].unreadCount == 2, "unread badge -> 2")
        check(rooms[1].unreadCount == 0, "no badge -> 0")
        check(rooms.allSatisfy { $0.memberCount == nil }, "memberCount null from list view (parity)")
        check(rooms[0].lastMessage?.text == "배포 끝났어요?", "last message preview")
        check(rooms[2].lastMessage == nil, "image-only preview -> nil")

        print("scenario-basic — readRecent")
        let messages = Parsers.messages(in: tree, selectors: selectors, myName: nil)
        check(messages.count == 3, "3 messages parsed")
        check(messages[0].sender == "민수" && !messages[0].outgoing, "incoming sender")
        check(messages[0].kind == .text, "text kind")
        check(messages[1].outgoing, "AXOutgoing subrole -> outgoing")
        check(messages[2].kind == .unsupported && messages[2].text == "", "image bubble -> unsupported")

        print("contract shape")
        let sr = SendResult(status: .unknown, at: nil, error: .sendVerifyTimeout)
        let json = (try? JSONEncoder().encode(sr)).flatMap { String(data: $0, encoding: .utf8) } ?? ""
        check(json.contains("\"status\":\"unknown\""), "SendResult status wire value")
        check(json.contains("\"error\":\"SEND_VERIFY_TIMEOUT\""), "ErrorCode wire value")
        check(
            ErrorCode.accessibilityPermissionDenied.rawValue == "ACCESSIBILITY_PERMISSION_DENIED",
            "ErrorCode raw values match contract"
        )

        print("")
        if failures == 0 {
            print("all checks passed")
            exit(0)
        } else {
            print("\(failures) check(s) failed")
            exit(1)
        }
    }
}
