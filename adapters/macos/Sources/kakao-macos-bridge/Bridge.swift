import Foundation
import ApplicationServices
import CoreGraphics

/// The five contract functions, live against KakaoTalk. Tree parsing is
/// delegated to `Parsers` (fixture-testable); this file owns app lookup,
/// permission gating, writes, and send verification.
enum Bridge {

    struct Context {
        let appElement: AXElement
        let selectors: SelectorMap
        let running: KakaoApp.Running
        let appVersion: String?
    }

    /// Common preflight for every function except `healthCheck`.
    static func context() throws -> Context {
        guard let running = KakaoApp.running() else {
            throw BridgeError(.kakaoNotRunning)
        }
        guard AX.isTrusted() else {
            throw BridgeError(.accessibilityPermissionDenied)
        }
        let version = KakaoApp.version(of: running)
        guard let selectors = SelectorMap.forVersion(version) else {
            throw BridgeError(.appVersionUnsupported, "no selector map for \(version ?? "unknown")")
        }
        let appRaw = AX.app(pid: running.pid)
        AX.setMessagingTimeout(appRaw, seconds: 1.0)
        let appElement = AXElement(raw: appRaw)
        return Context(
            appElement: appElement,
            selectors: selectors,
            running: running,
            appVersion: version
        )
    }

    // MARK: listRooms

    static func listRooms() throws -> ListRoomsData {
        let ctx = try context()
        let rooms = Parsers.rooms(in: ctx.appElement, selectors: ctx.selectors)
        return ListRoomsData(rooms: rooms)
    }

    // MARK: openRoom

    static func openRoom(roomId: String) throws {
        let ctx = try context()
        _ = try selectRoomRow(roomId: roomId, ctx: ctx)
    }

    /// Finds the chat-list row for `roomId` and activates it WITHOUT bringing
    /// the window to the front. Returns the row element.
    @discardableResult
    static func selectRoomRow(roomId: String, ctx: Context) throws -> AXElement {
        guard let list = ctx.appElement.firstDescendant(where: {
            $0.role == ctx.selectors.roomListRole
        }) else {
            throw BridgeError(.uiElementNotFound, "room list (\(ctx.selectors.roomListRole))")
        }
        let rows = list.children.filter { $0.role == ctx.selectors.roomRowRole }

        let target: UINode?
        if let idx = rowIndex(from: roomId) {
            target = idx < rows.count ? rows[idx] : nil
        } else {
            target = rows.first { $0.identifier == roomId }
        }
        guard let row = target as? AXElement else {
            throw BridgeError(.roomNotFound, "roomId \(roomId)")
        }
        if !AX.perform(row.raw, kAXPressAction as String) {
            throw BridgeError(.uiElementNotFound, "press action on room row")
        }
        return row
    }

    private static func rowIndex(from roomId: String) -> Int? {
        guard roomId.hasPrefix("row:") else { return nil }
        return Int(roomId.dropFirst(4))
    }

    // MARK: readRecent

    static func readRecent(roomId: String, limit: Int) throws -> ReadRecentData {
        let ctx = try context()
        try selectRoomRow(roomId: roomId, ctx: ctx)

        let myName = AX.string(ctx.appElement.raw, kAXTitleAttribute as String) // placeholder; PoC resolves real display name
        let all = Parsers.messages(in: ctx.appElement, selectors: ctx.selectors, myName: myName)
        let trimmed = limit > 0 && all.count > limit ? Array(all.suffix(limit)) : all
        return ReadRecentData(messages: trimmed)
    }

    // MARK: sendText

    static func sendText(roomId: String, text: String) throws -> SendResult {
        if text.isEmpty { return SendResult(status: .failed, at: nil, error: .emptyMessage) }

        let ctx: Context
        do {
            ctx = try context()
        } catch let e as BridgeError {
            return SendResult(status: .failed, at: nil, error: e.code)
        }

        do {
            try selectRoomRow(roomId: roomId, ctx: ctx)
        } catch let e as BridgeError {
            return SendResult(status: .failed, at: nil, error: e.code)
        }

        guard let field = ctx.appElement.firstDescendant(where: {
            $0.role == ctx.selectors.composeFieldRole
        }) as? AXElement else {
            return SendResult(status: .failed, at: nil, error: .sendInputFailed)
        }

        // Set the whole body as the field value: newlines land literally and do
        // not collide with the Enter-to-send shortcut.
        if !AX.setValue(field.raw, kAXValueAttribute as String, text as CFTypeRef) {
            // TODO(PoC): clipboard-paste fallback (NSPasteboard + Cmd-V synth).
            return SendResult(status: .failed, at: nil, error: .sendInputFailed)
        }

        // Prefer the send button over a synthesised Return so we do not depend
        // on the "Enter to send" preference.
        let pressed = pressSendButton(ctx: ctx) || pressReturn(in: field)
        if !pressed {
            return SendResult(status: .failed, at: nil, error: .sendInputFailed)
        }

        return verifySend(text: text, ctx: ctx)
    }

    private static func pressSendButton(ctx: Context) -> Bool {
        let label = ctx.selectors.sendButtonLabel
        guard let button = ctx.appElement.firstDescendant(where: { node in
            node.role == "AXButton"
                && (node.title == label || node.descriptionText == label)
        }) as? AXElement else { return false }
        return AX.perform(button.raw, kAXPressAction as String)
    }

    private static func pressReturn(in field: AXElement) -> Bool {
        // Best-effort: confirm the field is focusable, then post a Return.
        AX.setValue(field.raw, kAXFocusedAttribute as String, kCFBooleanTrue as CFTypeRef)
        let src = CGEventSource(stateID: .hidSystemState)
        guard let down = CGEvent(keyboardEventSource: src, virtualKey: 0x24, keyDown: true),
              let up = CGEvent(keyboardEventSource: src, virtualKey: 0x24, keyDown: false)
        else { return false }
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
        return true
    }

    /// Poll the message area (100ms x up to 3s) for our own outgoing bubble.
    private static func verifySend(text: String, ctx: Context) -> SendResult {
        let deadline = Date().addingTimeInterval(3.0)
        let needle = text.trimmingCharacters(in: .whitespacesAndNewlines)
        while Date() < deadline {
            let msgs = Parsers.messages(in: ctx.appElement, selectors: ctx.selectors, myName: nil)
            if msgs.contains(where: { $0.outgoing && $0.text.contains(needle) }) {
                return SendResult(status: .sent, at: ISO8601.now(), error: nil)
            }
            Thread.sleep(forTimeInterval: 0.1)
        }
        return SendResult(status: .unknown, at: nil, error: .sendVerifyTimeout)
    }

    // MARK: healthCheck

    static func healthCheck() -> Health {
        guard let running = KakaoApp.running() else {
            return Health(
                kakaoRunning: false,
                accessibilityGranted: AX.isTrusted(),
                appVersion: nil,
                issues: [Issue(code: .kakaoNotRunning, recovery: "카카오톡 데스크톱 앱을 실행하세요.")]
            )
        }
        let trusted = AX.isTrusted()
        let version = KakaoApp.version(of: running)
        var issues: [Issue] = []
        if !trusted {
            issues.append(Issue(
                code: .accessibilityPermissionDenied,
                recovery: "시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용에서 kakao-cli 를 허용하세요."
            ))
        }
        if trusted && SelectorMap.forVersion(version) == nil {
            issues.append(Issue(
                code: .appVersionUnsupported,
                recovery: "지원 버전이 아닙니다. 이슈로 버전을 알려주세요."
            ))
        }
        return Health(
            kakaoRunning: true,
            accessibilityGranted: trusted,
            appVersion: version,
            issues: issues
        )
    }
}
