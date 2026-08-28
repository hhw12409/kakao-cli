import Foundation
import Testing
@testable import kakao_macos_bridge

/// Parser tests run against a serialized accessibility tree, so they need
/// neither KakaoTalk nor accessibility permission. Add one fixture per known
/// KakaoTalk version and re-run these to catch UI-update regressions.

private func loadFixture(_ name: String) throws -> FixtureNode {
    let url = try #require(
        Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures")
    )
    let data = try Data(contentsOf: url)
    return try JSONDecoder().decode(FixtureNode.self, from: data)
}

private var selectors: SelectorMap { SelectorMap.known["*"]! }

// MARK: listRooms

@Test func roomsParsedFromFixture() throws {
    let rooms = Parsers.rooms(in: try loadFixture("scenario-basic"), selectors: selectors)

    #expect(rooms.count == 3)
    #expect(rooms.map(\.title) == ["개발팀", "엄마", "개발 공지"])
    #expect(rooms.map(\.roomId) == ["row:0", "row:1", "row:2"])
}

@Test func unreadBadgeParsedAsCount() throws {
    let rooms = Parsers.rooms(in: try loadFixture("scenario-basic"), selectors: selectors)
    #expect(rooms[0].unreadCount == 2)
    #expect(rooms[1].unreadCount == 0)
    #expect(rooms[2].unreadCount == 0)
}

@Test func memberCountIsNilFromListView() throws {
    let rooms = Parsers.rooms(in: try loadFixture("scenario-basic"), selectors: selectors)
    // Parity rule: the list view cannot read member count -> null.
    #expect(rooms.allSatisfy { $0.memberCount == nil })
}

@Test func lastMessagePreview() throws {
    let rooms = Parsers.rooms(in: try loadFixture("scenario-basic"), selectors: selectors)
    #expect(rooms[0].lastMessage?.text == "배포 끝났어요?")
    #expect(rooms[2].lastMessage == nil) // image preview -> no readable text
}

// MARK: readRecent

@Test func messagesParsedWithOutgoingAndKind() throws {
    let messages = Parsers.messages(
        in: try loadFixture("scenario-basic"),
        selectors: selectors,
        myName: nil
    )

    #expect(messages.count == 3)

    #expect(messages[0].sender == "민수")
    #expect(messages[0].text == "배포 끝났어요?")
    #expect(messages[0].outgoing == false)
    #expect(messages[0].kind == .text)

    #expect(messages[1].outgoing == true)          // subrole AXOutgoing
    #expect(messages[1].text == "확인 중입니다")

    #expect(messages[2].kind == .unsupported)      // image bubble, no text
    #expect(messages[2].text == "")
}

// MARK: contract shape

@Test func sendResultEncodingMatchesContract() throws {
    let result = SendResult(status: .unknown, at: nil, error: .sendVerifyTimeout)
    let json = String(data: try JSONEncoder().encode(result), encoding: .utf8)!
    #expect(json.contains("\"status\":\"unknown\""))
    #expect(json.contains("\"error\":\"SEND_VERIFY_TIMEOUT\""))
}

@Test func errorCodeRawValuesMatchContract() {
    #expect(ErrorCode.accessibilityPermissionDenied.rawValue == "ACCESSIBILITY_PERMISSION_DENIED")
    #expect(ErrorCode.sendVerifyTimeout.rawValue == "SEND_VERIFY_TIMEOUT")
    #expect(ErrorCode.roomNotFound.rawValue == "ROOM_NOT_FOUND")
}
