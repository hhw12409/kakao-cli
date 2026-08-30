import Foundation
import ApplicationServices
import CoreGraphics
import AppKit

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

    /// The element under which the open conversation for `title` lives: a
    /// popped-out window titled with the room name, or (fallback) the main
    /// window, where the conversation renders in an embedded pane.
    static func conversationContainer(_ ctx: Context, title: String) -> AXElement? {
        if let separate = conversationWindow(ctx, title: title) { return separate }
        return try? mainWindow(ctx)
    }

    /// Find the message table in either container shape.
    static func conversationTable(_ ctx: Context, container: AXElement) -> AXElement? {
        findTable(container, identifier: ctx.selectors.messageTableIdentifier)
    }

    /// KakaoTalk's message list is a virtualised NSTableView: only on-screen
    /// rows exist in the AX tree. Scroll the message area to the bottom so the
    /// newest messages are actually present before we read them.
    static func scrollMessagesToBottom(_ ctx: Context, container: AXElement) {
        for child in AX.elements(container.raw, kAXChildrenAttribute as String) {
            let v = AX.multi(child, [kAXRoleAttribute as String, kAXIdentifierAttribute as String])
            guard v[0] as? String == "AXScrollArea",
                  v[1] as? String == ctx.selectors.messageScrollAreaIdentifier
            else { continue }
            for _ in 0..<6 { AX.perform(child, "AXScrollDownByPage") }
            return
        }
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

    /// Activate a chat row so its conversation opens.
    ///
    /// KakaoTalk's list rows advertise NO accessibility action (`row actions:
    /// []`, `cell actions: ["AXShowMenu"]`) — verified with `--actions`. There
    /// is no AX way to open a chat, so this is the documented last resort: a
    /// synthesised mouse click at the row's accessibility frame. `AXPosition`
    /// is in screen points, so this is independent of resolution / DPI / window
    /// placement (unlike a hard-coded pixel offset).
    @discardableResult
    static func openRow(_ ctx: Context, _ row: AXElement) -> Bool {
        let target = (row.firstDescendant(where: { $0.role == "AXCell" }) as? AXElement) ?? row
        guard let f = AX.frame(target.raw) else { return false }
        let center = CGPoint(x: f.midX, y: f.midY)

        // A synthesised click only reaches a control if KakaoTalk's window is
        // the front window; otherwise macOS consumes the first click just to
        // activate it. `openRoom`/`sendText` are the state-changing calls, so a
        // brief raise here is an accepted trade-off (contract §2).
        if let main = try? mainWindow(ctx) {
            AX.perform(main.raw, "AXRaise")
        }
        AX.setValue(ctx.appRaw, "AXFrontmost", kCFBooleanTrue as CFTypeRef)
        Thread.sleep(forTimeInterval: 0.15)

        let src = CGEventSource(stateID: .hidSystemState)
        // Double click: KakaoTalk opens a chat on double-click of a list row.
        for click in 1...2 {
            for down in [true, false] {
                guard let ev = CGEvent(
                    mouseEventSource: src,
                    mouseType: down ? .leftMouseDown : .leftMouseUp,
                    mouseCursorPosition: center,
                    mouseButton: .left
                ) else { return false }
                if click == 2 { ev.setIntegerValueField(.mouseEventClickState, value: 2) }
                ev.post(tap: .cghidEventTap)
                Thread.sleep(forTimeInterval: 0.03)
            }
        }
        return true
    }

    /// Actions an element advertises (for the `--actions` debug command).
    static func actions(_ element: AXUIElement) -> [String] {
        var names: CFArray?
        guard AXUIElementCopyActionNames(element, &names) == .success,
              let arr = names as? [String] else { return [] }
        return arr
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
        if !openRow(ctx, row) {
            throw BridgeError(.uiElementNotFound, "could not activate chat row")
        }
    }

    // MARK: readRecent

    static func readRecent(roomId: String, limit: Int) throws -> ReadRecentData {
        let ctx = try context()
        let (row, title) = try resolveRoom(ctx, roomId: roomId)
        let container = try openConversation(ctx, row: row, title: title)
        scrollMessagesToBottom(ctx, container: container)
        Thread.sleep(forTimeInterval: 0.2)
        guard let table = conversationTable(ctx, container: container) else {
            throw BridgeError(.uiElementNotFound, "message table \(ctx.selectors.messageTableIdentifier)")
        }
        let winX = AX.frame(container.raw).map { Double($0.minX) }
        return ReadRecentData(messages: readMessages(ctx, table: table, want: max(limit, 1), windowMinX: winX))
    }

    /// Read the message tail for a **watch** poll. Unlike `readRecent` this
    /// never synthesises a click: if the conversation is not already open in
    /// KakaoTalk it throws `UI_ELEMENT_NOT_FOUND`, which the poller turns into a
    /// `roomClosed` event after a couple of misses. Keeps steady-state polling
    /// free of focus stealing.
    static func readMessagesForWatch(roomId: String, limit: Int = 12) throws -> [Message] {
        let ctx = try context()
        let (_, title) = try resolveRoom(ctx, roomId: roomId)
        guard let container = conversationContainer(ctx, title: title),
              let table = conversationTable(ctx, container: container) else {
            throw BridgeError(.uiElementNotFound, "conversation for \(title) not open")
        }
        scrollMessagesToBottom(ctx, container: container)
        let winX = AX.frame(container.raw).map { Double($0.minX) }
        return readMessages(ctx, table: table, want: max(limit, 1), windowMinX: winX)
    }

    /// Ensure the conversation for `title` is open and return its container
    /// (a popped-out window, or the main window for the embedded pane). Tries
    /// already-open first, then a synthesised click on the list row.
    static func openConversation(_ ctx: Context, row: AXElement, title: String) throws -> AXElement {
        if let c = conversationContainer(ctx, title: title),
           conversationTable(ctx, container: c) != nil {
            return c
        }
        openRow(ctx, row)
        let deadline = Date().addingTimeInterval(3.0)
        while Date() < deadline {
            if let c = conversationContainer(ctx, title: title),
               conversationTable(ctx, container: c) != nil {
                return c
            }
            Thread.sleep(forTimeInterval: 0.1)
        }
        throw BridgeError(.uiElementNotFound, "conversation for \(title) did not open")
    }

    /// Snapshot the tail of the message table in batched reads and parse it.
    static func readMessages(_ ctx: Context, table: AXElement, want: Int, windowMinX: Double? = nil) -> [Message] {
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
        let all = Parsers.messages(
            in: synthetic,
            selectors: ctx.selectors,
            myName: myName,
            windowMinX: windowMinX
        )
        return all.count > want ? Array(all.suffix(want)) : all
    }

    // MARK: sendText

    static func sendText(roomId: String, text: String) throws -> SendResult {
        if text.isEmpty { return SendResult(status: .failed, at: nil, error: .emptyMessage) }

        let ctx: Context
        let row: AXElement
        let title: String
        let container: AXElement
        do {
            ctx = try context()
            (row, title) = try resolveRoom(ctx, roomId: roomId)
            container = try openConversation(ctx, row: row, title: title)
        } catch let e as BridgeError {
            return SendResult(status: .failed, at: nil, error: e.code)
        }

        guard let field = findComposeField(container, selectors: ctx.selectors) else {
            return SendResult(status: .failed, at: nil, error: .sendInputFailed)
        }

        if !enterText(text, into: field) {
            return SendResult(status: .failed, at: nil, error: .sendInputFailed)
        }

        // The 전송 button advertises no AX action, so trigger the send with a
        // real click on its frame, then fall back to Enter in the (focused,
        // raised) compose field. Both need the window frontmost.
        AX.setValue(ctx.appRaw, "AXFrontmost", kCFBooleanTrue as CFTypeRef)
        AX.setValue(field.raw, kAXFocusedAttribute as String, kCFBooleanTrue as CFTypeRef)
        Thread.sleep(forTimeInterval: 0.1)

        if !clickSendButton(in: container, selectors: ctx.selectors) {
            _ = pressReturn(in: field)
        }

        return verifySend(ctx, text: text, container: container)
    }

    /// Put `text` into the compose field. KakaoTalk's compose area does not
    /// accept `AXUIElementSetAttributeValue(kAXValue)` reliably, so we focus it
    /// and paste from the clipboard (restoring the previous clipboard after).
    /// Newlines survive because paste inserts them literally rather than firing
    /// the Enter-to-send shortcut.
    private static func enterText(_ text: String, into field: AXElement) -> Bool {
        // Try the direct route first; harmless if it silently no-ops.
        AX.setValue(field.raw, kAXValueAttribute as String, text as CFTypeRef)
        if AX.string(field.raw, kAXValueAttribute as String) == text { return true }

        // Clipboard paste fallback.
        let pb = NSPasteboard.general
        let saved = pb.pasteboardItems?.compactMap { item -> (NSPasteboard.PasteboardType, Data)? in
            item.types.first.flatMap { t in item.data(forType: t).map { (t, $0) } }
        }

        pb.clearContents()
        pb.setString(text, forType: .string)

        AX.setValue(field.raw, kAXFocusedAttribute as String, kCFBooleanTrue as CFTypeRef)
        Thread.sleep(forTimeInterval: 0.05)
        pressCmdV()
        Thread.sleep(forTimeInterval: 0.15)

        let ok = (AX.string(field.raw, kAXValueAttribute as String) ?? "").contains(
            text.trimmingCharacters(in: .whitespacesAndNewlines)
        )

        // Restore the previous clipboard.
        pb.clearContents()
        if let saved, !saved.isEmpty {
            for (type, data) in saved { pb.setData(data, forType: type) }
        }
        return ok
    }

    private static func pressCmdV() {
        let src = CGEventSource(stateID: .hidSystemState)
        let vKey: CGKeyCode = 0x09
        for down in [true, false] {
            guard let ev = CGEvent(keyboardEventSource: src, virtualKey: vKey, keyDown: down)
            else { return }
            ev.flags = .maskCommand
            ev.post(tap: .cghidEventTap)
            Thread.sleep(forTimeInterval: 0.02)
        }
    }

    /// Click the 전송 button at its accessibility frame. `AXPress` on it is a
    /// no-op (the button advertises no actions), so a synthesised click is the
    /// only way. Returns false if the button can't be located.
    private static func clickSendButton(in container: AXElement, selectors: SelectorMap) -> Bool {
        for child in AX.elements(container.raw, kAXChildrenAttribute as String) {
            let v = AX.multi(child, [kAXRoleAttribute as String, kAXTitleAttribute as String])
            guard v[0] as? String == "AXButton", v[1] as? String == selectors.sendButtonTitle
            else { continue }
            _ = AX.perform(child, kAXPressAction as String)   // harmless if unsupported
            guard let f = AX.frame(child) else { return false }
            clickAt(CGPoint(x: f.midX, y: f.midY))
            return true
        }
        return false
    }

    private static func clickAt(_ p: CGPoint) {
        let src = CGEventSource(stateID: .hidSystemState)
        for down in [true, false] {
            guard let ev = CGEvent(
                mouseEventSource: src,
                mouseType: down ? .leftMouseDown : .leftMouseUp,
                mouseCursorPosition: p,
                mouseButton: .left
            ) else { return }
            ev.post(tap: .cghidEventTap)
            Thread.sleep(forTimeInterval: 0.03)
        }
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
        container: AXElement
    ) -> SendResult {
        let deadline = Date().addingTimeInterval(3.0)
        let needle = text.trimmingCharacters(in: .whitespacesAndNewlines)
        while Date() < deadline {
            scrollMessagesToBottom(ctx, container: container)
            if let table = conversationTable(ctx, container: container),
               readMessages(ctx, table: table, want: 6).contains(where: { $0.text.contains(needle) }) {
                return SendResult(status: .sent, at: ISO8601.now(), error: nil)
            }
            Thread.sleep(forTimeInterval: 0.25)
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
