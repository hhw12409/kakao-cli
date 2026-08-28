import Foundation
import ApplicationServices
import CoreGraphics

/// The five contract functions, live against KakaoTalk.
///
/// Window model (KakaoTalk 26.6.1): the main window ("카카오톡") holds the chat
/// list in `AXScrollArea(_NS:101)` and an embedded conversation pane in
/// `AXScrollArea(_NS:28)`. A chat can ALSO be popped out into a separate window
/// titled with the room name. `readRecent`/`sendText` currently only handle the
/// separate-window case — the embedded pane's selectors still need a dump with
/// a chat open (TODO / see `_workspace/ax-dumps/`).
///
/// `kAXWindowsAttribute` returns nothing when windows are minimized or the app
/// is not frontmost — that surfaces as `KAKAO_WINDOW_NOT_VISIBLE` with a "click
/// the Dock icon" recovery hint.
enum Bridge {

    struct Context {
        let appRaw: AXUIElement
        let selectors: SelectorMap
        let running: KakaoApp.Running
        let appVersion: String?
    }

    static func context() throws -> Context {
        guard let running = KakaoApp.running() else { throw BridgeError(.kakaoNotRunning) }
        guard AX.isTrusted() else { throw BridgeError(.accessibilityPermissionDenied) }
        let version = KakaoApp.version(of: running)
        guard let selectors = SelectorMap.forVersion(version) else {
            throw BridgeError(.appVersionUnsupported, "no selector map for \(version ?? "unknown")")
        }
        let appRaw = AX.app(pid: running.pid)
        AX.setMessagingTimeout(appRaw, seconds: 2.0)
        return Context(appRaw: appRaw, selectors: selectors, running: running, appVersion: version)
    }

    // MARK: window lookup

    static func windows(_ ctx: Context) -> [AXElement] {
        var ws = AX.elements(ctx.appRaw, kAXWindowsAttribute as String)
        if ws.isEmpty {
            ws = AX.elements(ctx.appRaw, kAXChildrenAttribute as String)
                .filter { AX.string($0, kAXRoleAttribute as String) == "AXWindow" }
        }
        return ws.map { AXElement(raw: $0) }
    }

    static func mainWindow(_ ctx: Context) throws -> AXElement {
        let all = windows(ctx)
        if all.isEmpty {
            throw BridgeError(.kakaoWindowNotVisible, "no KakaoTalk windows (minimized?)")
        }
        if let byId = all.first(where: { $0.identifier == ctx.selectors.mainWindowIdentifier }) {
            return byId
        }
        if let byTitle = all.first(where: { $0.title == ctx.selectors.mainWindowTitle }) {
            return byTitle
        }
        throw BridgeError(.uiElementNotFound, "main window not among \(all.count) window(s)")
    }

    static func conversationWindow(_ ctx: Context, title: String) -> AXElement? {
        windows(ctx).first { $0.title == title && $0.identifier != ctx.selectors.mainWindowIdentifier }
    }

    /// How many chat rows / messages to pull. KakaoTalk's per-AX-call latency is
    /// high; a full 100+ row list read takes ~20s. `inbox`/`rooms` only need the
    /// most recent rooms (the list is already in recency order).
    static let rowScanLimit = 40

    /// Direct navigation to an `AXTable` by identifier under `window`, without a
    /// recursive `firstDescendant` walk (which would cost hundreds of AX calls).
    static func findTable(_ window: AXElement, identifier: String) -> AXElement? {
        for child in AX.elements(window.raw, kAXChildrenAttribute as String) {
            let v = AX.multi(child, [kAXRoleAttribute as String, kAXIdentifierAttribute as String])
            let role = v[0] as? String
            if v[1] as? String == identifier, role == "AXTable" {
                return AXElement(raw: child)
            }
            if role == "AXScrollArea" || role == "AXGroup" {
                for gc in AX.elements(child, kAXChildrenAttribute as String) {
                    let gv = AX.multi(gc, [kAXRoleAttribute as String, kAXIdentifierAttribute as String])
                    if gv[0] as? String == "AXTable",
                       gv[1] as? String == identifier {
                        return AXElement(raw: gc)
                    }
                }
            }
        }
        return nil
    }

    // MARK: roomId <-> row

    /// Resolves an opaque `roomId` ("row:N") to the row element + its title,
    /// reading the main window's chat table.
    static func resolveRoom(_ ctx: Context, roomId: String) throws -> (row: AXElement, title: String) {
        guard let idx = rowIndex(from: roomId) else {
            throw BridgeError(.roomNotFound, "unrecognised roomId \(roomId)")
        }
        let main = try mainWindow(ctx)
        guard let table = findTable(main, identifier: ctx.selectors.roomTableIdentifier) else {
            throw BridgeError(.uiElementNotFound, "chat table \(ctx.selectors.roomTableIdentifier)")
        }
        let rowEls = AX.elements(table.raw, kAXChildrenAttribute as String)
        guard idx < rowEls.count else {
            throw BridgeError(.roomNotFound, "row \(idx) of \(rowEls.count)")
        }
        let row = AXElement(raw: rowEls[idx])
        // Row index includes spacer rows; parse the same way listRooms does so
        // the caller's "row:N" lines up with what they saw.
        let snap = row.snapshot(maxDepth: 3)
        guard snap.role == "AXRow" else {
            throw BridgeError(.roomNotFound, "element at index \(idx) is \(snap.role), not a row")
        }
        let title = Parsers.fieldValue(
            snap.firstDescendant(where: { $0.role == "AXCell" }) ?? snap,
            id: ctx.selectors.roomTitleIdentifier
        ) ?? ""
        return (row, title)
    }

    private static func rowIndex(from roomId: String) -> Int? {
        roomId.hasPrefix("row:") ? Int(roomId.dropFirst(4)) : nil
    }

    /// Activate a chat row without raising the app: press the row, then its cell.
    static func openRow(_ row: AXElement) -> Bool {
        if AX.perform(row.raw, kAXPressAction as String) { return true }
        AX.setValue(row.raw, kAXSelectedAttribute as String, kCFBooleanTrue as CFTypeRef)
        if let cell = row.firstDescendant(where: { $0.role == "AXCell" }) as? AXElement {
            return AX.perform(cell.raw, kAXPressAction as String)
        }
        return false
    }

    // MARK: listRooms

    static func listRooms() throws -> ListRoomsData {
        let ctx = try context()
        let main = try mainWindow(ctx)
        guard let table = findTable(main, identifier: ctx.selectors.roomTableIdentifier) else {
            throw BridgeError(.uiElementNotFound, "chat table \(ctx.selectors.roomTableIdentifier)")
        }
        // Snapshot the first N rows in batched reads, then parse in memory.
        let rowEls = AX.elements(table.raw, kAXChildrenAttribute as String).prefix(rowScanLimit)
        let rows = rowEls.map { AXElement(raw: $0).snapshot(maxDepth: 3) }
        let synthetic = FixtureNode(
            role: "AXWindow",
            children: [FixtureNode(role: "AXTable", identifier: ctx.selectors.roomTableIdentifier, children: rows)]
        )
        return ListRoomsData(rooms: Parsers.rooms(in: synthetic, selectors: ctx.selectors))
    }

    // MARK: openRoom

    static func openRoom(roomId: String) throws {
        let ctx = try context()
        let (row, _) = try resolveRoom(ctx, roomId: roomId)
        if !openRow(row) {
            throw BridgeError(.uiElementNotFound, "could not activate chat row")
        }
    }

    // MARK: readRecent

    static func readRecent(roomId: String, limit: Int) throws -> ReadRecentData {
        let ctx = try context()
        let (row, title) = try resolveRoom(ctx, roomId: roomId)

        var win = conversationWindow(ctx, title: title)
        if win == nil {
            _ = openRow(row)
            win = pollForWindow(ctx, title: title, timeout: 2.0)
        }
        guard let conversation = win else {
            throw BridgeError(.uiElementNotFound, "conversation window for \(title)")
        }
        guard let messages = readMessages(ctx, conversation: conversation, want: max(limit, 1)) else {
            throw BridgeError(.uiElementNotFound, "message table \(ctx.selectors.messageTableIdentifier)")
        }
        return ReadRecentData(messages: messages)
    }

    /// Snapshot the tail of the message table in batched reads and parse it.
    /// Returns `nil` only when the table can't be found.
    static func readMessages(_ ctx: Context, conversation: AXElement, want: Int) -> [Message]? {
        guard let table = findTable(conversation, identifier: ctx.selectors.messageTableIdentifier)
        else { return nil }

        let rowEls = AX.elements(table.raw, kAXChildrenAttribute as String)
        // Messages are oldest->newest; take the tail plus slack for spacer rows
        // and the trailing AXColumn.
        let tail = rowEls.suffix(want + 8)
        let rows = tail.map { AXElement(raw: $0).snapshot(maxDepth: 4) }
        let synthetic = FixtureNode(
            role: "AXWindow",
            children: [FixtureNode(
                role: "AXTable",
                identifier: ctx.selectors.messageTableIdentifier,
                children: rows
            )]
        )
        let myName = AX.string(ctx.appRaw, kAXTitleAttribute as String)
        let all = Parsers.messages(in: synthetic, selectors: ctx.selectors, myName: myName)
        return all.count > want ? Array(all.suffix(want)) : all
    }

    private static func pollForWindow(_ ctx: Context, title: String, timeout: TimeInterval) -> AXElement? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let w = conversationWindow(ctx, title: title) { return w }
            Thread.sleep(forTimeInterval: 0.1)
        }
        return nil
    }

    // MARK: sendText

    static func sendText(roomId: String, text: String) throws -> SendResult {
        if text.isEmpty { return SendResult(status: .failed, at: nil, error: .emptyMessage) }

        let ctx: Context
        let row: AXElement
        let title: String
        do {
            ctx = try context()
            (row, title) = try resolveRoom(ctx, roomId: roomId)
        } catch let e as BridgeError {
            return SendResult(status: .failed, at: nil, error: e.code)
        }

        var win = conversationWindow(ctx, title: title)
        if win == nil {
            _ = openRow(row)
            win = pollForWindow(ctx, title: title, timeout: 2.0)
        }
        guard let conversation = win else {
            return SendResult(status: .failed, at: nil, error: .uiElementNotFound)
        }

        guard let field = findComposeField(conversation, selectors: ctx.selectors) else {
            return SendResult(status: .failed, at: nil, error: .sendInputFailed)
        }

        // Whole body as the field value: newlines land literally, no collision
        // with the Enter-to-send shortcut.
        if !AX.setValue(field.raw, kAXValueAttribute as String, text as CFTypeRef) {
            // TODO(PoC): NSPasteboard + Cmd-V synth fallback.
            return SendResult(status: .failed, at: nil, error: .sendInputFailed)
        }

        let pressed = pressSendButton(in: conversation, selectors: ctx.selectors)
            || pressReturn(in: field)
        if !pressed {
            return SendResult(status: .failed, at: nil, error: .sendInputFailed)
        }

        return verifySend(ctx, text: text, conversation: conversation)
    }

    private static func pressSendButton(in conversation: AXElement, selectors: SelectorMap) -> Bool {
        // The send button is a direct child of the conversation window.
        for child in AX.elements(conversation.raw, kAXChildrenAttribute as String) {
            let v = AX.multi(child, [kAXRoleAttribute as String, kAXTitleAttribute as String])
            if v[0] as? String == "AXButton", v[1] as? String == selectors.sendButtonTitle {
                return AX.perform(child, kAXPressAction as String)
            }
        }
        return false
    }

    /// The compose field is `AXScrollArea(_NS:47) > AXTextArea(_NS:51)`, a
    /// direct child of the window — NOT inside the message table. Navigate
    /// child-by-child so we never walk the 100+ message rows.
    private static func findComposeField(_ window: AXElement, selectors: SelectorMap) -> AXElement? {
        for child in AX.elements(window.raw, kAXChildrenAttribute as String) {
            guard AX.string(child, kAXRoleAttribute as String) == "AXScrollArea" else { continue }
            for gc in AX.elements(child, kAXChildrenAttribute as String) {
                let v = AX.multi(gc, [
                    kAXRoleAttribute as String,
                    kAXIdentifierAttribute as String,
                    kAXDescriptionAttribute as String,
                ])
                guard v[0] as? String == "AXTextArea" else { continue }
                if v[1] as? String == selectors.composeFieldIdentifier
                    || v[2] as? String == selectors.composeFieldDescription {
                    return AXElement(raw: gc)
                }
            }
        }
        return nil
    }

    private static func pressReturn(in field: AXElement) -> Bool {
        AX.setValue(field.raw, kAXFocusedAttribute as String, kCFBooleanTrue as CFTypeRef)
        let src = CGEventSource(stateID: .hidSystemState)
        guard let down = CGEvent(keyboardEventSource: src, virtualKey: 0x24, keyDown: true),
              let up = CGEvent(keyboardEventSource: src, virtualKey: 0x24, keyDown: false)
        else { return false }
        down.post(tap: .cghidEventTap)
        up.post(tap: .cghidEventTap)
        return true
    }

    /// Poll the message list (~200ms x up to 3s) for our own text.
    private static func verifySend(
        _ ctx: Context,
        text: String,
        conversation: AXElement
    ) -> SendResult {
        let deadline = Date().addingTimeInterval(3.0)
        let needle = text.trimmingCharacters(in: .whitespacesAndNewlines)
        while Date() < deadline {
            if let msgs = readMessages(ctx, conversation: conversation, want: 6),
               msgs.contains(where: { $0.text.contains(needle) }) {
                return SendResult(status: .sent, at: ISO8601.now(), error: nil)
            }
            Thread.sleep(forTimeInterval: 0.2)
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
