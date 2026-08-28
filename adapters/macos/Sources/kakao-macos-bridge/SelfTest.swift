import Foundation

/// `kakao-macos-bridge --self-test` — parser regression checks against the
/// bundled `scenario-basic` fixture (a hand-sanitised copy of the real
/// KakaoTalk 26.6.1 tree structure). Toolchain-independent stand-in for the
/// swift-testing suite in `Tests/`.
///
/// Exit 0 = all passed, 1 = a check failed. Run after adding a new per-version
/// fixture or touching `Parsers` / `Selectors`.
enum SelfTest {
    private static var failures = 0

    private static func check(_ condition: Bool, _ label: String) {
        print(condition ? "  ok   \(label)" : "  FAIL \(label)")
        if !condition { failures += 1 }
    }

    static func run() -> Never {
        let selectors = SelectorMap.v26_6
        // Fixed clock so "오전 11:17" / "오전 10:00" produce stable ISO strings.
        let now = ISO8601DateFormatter().date(from: "2026-08-29T00:00:00Z") ?? Date()

        guard let url = Bundle.module.url(
                forResource: "scenario-basic", withExtension: "json", subdirectory: "Fixtures"),
              let data = try? Data(contentsOf: url),
              let tree = try? JSONDecoder().decode(FixtureNode.self, from: data)
        else {
            FileHandle.standardError.write(Data("could not load scenario-basic fixture\n".utf8))
            exit(1)
        }

        let mainWindow = tree.children.first { $0.title == "카카오톡" }!
        let convWindow = tree.children.first { $0.title == "개발팀" }!

        print("scenario-basic — listRooms")
        let rooms = Parsers.rooms(in: mainWindow, selectors: selectors, now: now)
        check(rooms.count == 3, "3 rooms parsed (spacer row skipped)")
        check(rooms.map(\.title) == ["개발팀", "엄마", "개발 공지"], "titles in list order")
        check(rooms.map(\.roomId) == ["row:0", "row:1", "row:2"], "opaque roomIds are row indices")
        check(rooms[0].unreadCount == 2, "bare numeric AXStaticText -> unread 2")
        check(rooms[1].unreadCount == 0, "no unread badge -> 0")
        check(rooms[0].memberCount == 18, "Count Label -> member count 18")
        check(rooms[1].memberCount == 2, "no Count Label -> 1:1 -> member count 2")
        check(rooms[2].memberCount == 42, "group Count Label -> 42")
        check(rooms[0].lastMessage?.text == "배포 끝났어요?", "last message preview from AXTextArea")
        check(rooms[2].lastMessage == nil, "empty preview -> nil lastMessage")
        check((rooms[0].lastMessage?.at ?? "").hasSuffix("Z"), "preview timestamp parsed to ISO UTC")

        print("scenario-basic — readRecent")
        let messages = Parsers.messages(in: convWindow, selectors: selectors, myName: "수빈", now: now)
        check(messages.count == 4, "4 messages parsed (spacer skipped)")
        check(messages[0].text == "배포 끝났어요?", "first message body")
        check(messages[0].sender == "민수", "sender from AXStaticText next to profile button")
        check(messages[0].kind == .text, "text kind")
        check(!(messages[0].at).isEmpty && messages[0].at.hasSuffix("Z"), "message time parsed to ISO")
        check(messages[1].text == "확인은요?", "second message body")
        check(messages[1].sender == "민수", "sender carried forward to a continuation row")
        check(messages[1].at == messages[0].at, "time inherited when row has no timestamp label")
        check(messages[2].sender == "수빈", "new sender run picked up")
        check(messages[2].at != messages[1].at, "multiline '1\\n오전 11:30' timestamp parsed (not inherited)")
        check(messages[2].outgoing, "sender == myName -> outgoing")
        check(messages[3].kind == .unsupported && messages[3].text == "", "media cell -> unsupported")
        check(messages[3].sender == "수빈", "sender carries to media cell")

        print("Korean time parsing")
        check(KoreanTime.parseHourMinute("오전 11:17").map { $0 == (11, 17) } ?? false, "오전 11:17 -> 11:17")
        check(KoreanTime.parseHourMinute("오후 12:12").map { $0 == (12, 12) } ?? false, "오후 12:12 -> 12:12")
        check(KoreanTime.parseHourMinute("오전 12:30").map { $0 == (0, 30) } ?? false, "오전 12:30 -> 00:30")
        check(KoreanTime.toISO("어제", now: now).hasSuffix("Z"), "어제 -> ISO date")
        check(KoreanTime.toISO("", now: now) == "", "empty label -> empty ISO")

        print("contract shape")
        let sr = SendResult(status: .unknown, at: nil, error: .sendVerifyTimeout)
        let json = (try? JSONEncoder().encode(sr)).flatMap { String(data: $0, encoding: .utf8) } ?? ""
        check(json.contains("\"status\":\"unknown\""), "SendResult status wire value")
        check(json.contains("\"error\":\"SEND_VERIFY_TIMEOUT\""), "ErrorCode wire value")
        check(ErrorCode.accessibilityPermissionDenied.rawValue == "ACCESSIBILITY_PERMISSION_DENIED",
              "ErrorCode raw values match contract")

        print("")
        if failures == 0 { print("all checks passed"); exit(0) }
        print("\(failures) check(s) failed"); exit(1)
    }
}
