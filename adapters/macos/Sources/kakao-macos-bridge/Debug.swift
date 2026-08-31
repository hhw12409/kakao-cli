import Foundation
import ApplicationServices

/// `--windows` / `--actions` dev helpers. Not part of the contract. Output
/// goes to stderr so it never contaminates a JSON response.
enum Debug {
    /// `--wake` — exercise the serve-mode window wake path with tracing.
    static func wake() {
        Bridge.autoManage = true
        func err(_ s: String) { FileHandle.standardError.write(Data((s + "\n").utf8)) }

        guard let running = KakaoApp.running() else { err("not running"); return }
        let appRaw = AX.app(pid: running.pid)
        let ws = AX.elements(appRaw, kAXWindowsAttribute as String)
        err("windows=\(ws.count)")
        for (i, w) in ws.enumerated() {
            err("  [\(i)] title=\(AX.string(w, kAXTitleAttribute as String) ?? "-") "
                + "minimized=\(String(describing: AX.bool(w, kAXMinimizedAttribute as String))) "
                + "hidden(app)=\(String(describing: AX.bool(appRaw, kAXHiddenAttribute as String)))")
        }
        do {
            try WindowControl.ensureAwake()
            err("ensureAwake OK")
        } catch let e as BridgeError {
            err("ensureAwake threw \(e.code.rawValue) \(e.diagnostic ?? "")")
        } catch { err("ensureAwake threw \(error)") }

        let after = AX.elements(appRaw, kAXWindowsAttribute as String)
        for (i, w) in after.enumerated() {
            err("  after [\(i)] minimized=\(String(describing: AX.bool(w, kAXMinimizedAttribute as String)))"
                + " children=\(AX.elements(w, kAXChildrenAttribute as String).count)")
        }
    }

    /// `--probe-rooms` — find the chat-list table and print its identifier +
    /// the field identifiers of the first few rows. Fast, targeted (no full
    /// tree walk). For building a version selector map.
    static func probeRooms() {
        Bridge.autoManage = true
        func p(_ s: String) { FileHandle.standardError.write(Data((s + "\n").utf8)) }
        try? WindowControl.ensureAwake()

        guard let running = KakaoApp.running() else { p("not running"); return }
        let appRaw = AX.app(pid: running.pid)
        AX.setMessagingTimeout(appRaw, seconds: 2.0)
        let windows = AX.elements(appRaw, kAXWindowsAttribute as String)
        guard let mainRaw = windows.first(where: {
            AX.string($0, kAXTitleAttribute as String) == "카카오톡"
        }) ?? windows.first else { p("no window"); return }
        let main = AXElement(raw: mainRaw)

        // Try to select the "chatrooms" tab so the list is present.
        if let tab = main.firstDescendant(where: {
            $0.identifier == "chatrooms" && $0.role == "AXButton"
        }) as? AXElement {
            AX.perform(tab.raw, "AXPress")
            Thread.sleep(forTimeInterval: 0.6)
        }

        // Every AXTable under the main window, deepest-first-ish.
        var tables: [(node: AXElement, depth: Int)] = []
        func walk(_ n: AXElement, _ d: Int) {
            if d > 30 { return }
            if n.role == "AXTable" || n.role == "AXOutline" {
                tables.append((n, d))
            }
            for c in AX.elements(n.raw, kAXChildrenAttribute as String) {
                walk(AXElement(raw: c), d + 1)
            }
        }
        walk(main, 0)

        p("=== \(tables.count) table/outline(s) under main window ===")
        for (t, d) in tables {
            let rows = AX.elements(t.raw, kAXChildrenAttribute as String)
            let rowRoles = rows.prefix(3).map { AX.string($0, kAXRoleAttribute as String) ?? "-" }
            p("depth=\(d) id=\(t.identifier ?? "-") role=\(t.role) rows=\(rows.count) firstRoles=\(rowRoles)")
        }

        // Dump the first non-empty rows of the widest table.
        guard let best = tables.max(by: {
            AX.elements($0.node.raw, kAXChildrenAttribute as String).count
                < AX.elements($1.node.raw, kAXChildrenAttribute as String).count
        })?.node else { p("no table"); return }

        p("\n=== rows of table id=\(best.identifier ?? "-") ===")
        let rows = AX.elements(best.raw, kAXChildrenAttribute as String)
        var shown = 0
        for r in rows {
            guard shown < 4 else { break }
            let snap = AXElement(raw: r).snapshot(maxDepth: 6)
            let flat = flatten(snap)
            // skip obvious spacer rows (no text anywhere)
            if flat.allSatisfy({ $0.2 == nil || $0.2!.isEmpty }) { continue }
            shown += 1
            p("--- row \(shown) ---")
            for (role, id, text) in flat {
                p("  \(role)  id=\(id ?? "-")  text=\(text.map { String($0.prefix(40)) } ?? "-")")
            }
        }
    }

    private static func flatten(_ n: UINode, _ acc: inout [(String, String?, String?)]) {
        let text = n.value ?? n.title ?? n.descriptionText
        acc.append((n.role, n.identifier, text))
        for c in n.children { flatten(c, &acc) }
    }
    private static func flatten(_ n: UINode) -> [(String, String?, String?)] {
        var acc: [(String, String?, String?)] = []
        flatten(n, &acc)
        return acc
    }

    /// `--probe-convo` — with a conversation already open, map the message
    /// table / compose field / send button in whatever window holds it.
    static func probeConvo() {
        Bridge.autoManage = true
        func p(_ s: String) { FileHandle.standardError.write(Data((s + "\n").utf8)) }
        try? WindowControl.ensureAwake()
        guard let running = KakaoApp.running() else { p("not running"); return }
        let appRaw = AX.app(pid: running.pid)
        AX.setMessagingTimeout(appRaw, seconds: 2.0)

        for w in AX.elements(appRaw, kAXWindowsAttribute as String) {
            let win = AXElement(raw: w)
            p("\n=== window title=\(win.title ?? "-") id=\(win.identifier ?? "-") ===")
            func walk(_ n: AXElement, _ d: Int) {
                if d > 16 { return }
                let kids = AX.elements(n.raw, kAXChildrenAttribute as String)
                let v = AX.multi(n.raw, [
                    kAXRoleAttribute as String, kAXIdentifierAttribute as String,
                    kAXDescriptionAttribute as String, kAXTitleAttribute as String,
                    kAXValueAttribute as String,
                ])
                let role = v[0] as? String ?? "-"
                if ["AXTable", "AXTextArea", "AXTextField", "AXButton", "AXScrollArea", "AXStaticText"].contains(role) {
                    let val = (v[4] as? String).map { String($0.prefix(24)) } ?? "-"
                    p("  d=\(d) \(role) id=\(v[1] as? String ?? "-") desc=\(v[2] as? String ?? "-") title=\(v[3] as? String ?? "-") val=\(val) kids=\(kids.count)")
                }
                // Don't descend into big lists (the 682-row chat table).
                if kids.count > 40 { p("  d=\(d) [\(role) \(kids.count) kids — not descending]"); return }
                for c in kids { walk(AXElement(raw: c), d + 1) }
            }
            walk(win, 0)
        }
    }

    /// `--press <identifier>` — AXPress the first button with that identifier.
    static func press(_ identifier: String) {
        Bridge.autoManage = true
        guard let ctx = try? Bridge.context(),
              let main = try? Bridge.mainWindow(ctx) else {
            FileHandle.standardError.write(Data("no context\n".utf8)); return
        }
        if let b = Bridge.findByIdentifier(main, identifier, maxDepth: 10) {
            let ok = AX.perform(b.raw, "AXPress")
            FileHandle.standardError.write(Data("pressed \(identifier): \(ok)\n".utf8))
        } else {
            FileHandle.standardError.write(Data("not found: \(identifier)\n".utf8))
        }
    }

    static func windows() {
        guard let ctx = try? Bridge.context() else {
            FileHandle.standardError.write(Data("cannot get context\n".utf8))
            return
        }
        let ws = Bridge.windows(ctx)
        var s = "\(ws.count) window(s):\n"
        for (i, w) in ws.enumerated() {
            s += "  [\(i)] id=\(w.identifier ?? "-")  subrole=\(w.subrole ?? "-")  title=\(w.title ?? "-")\n"
        }
        FileHandle.standardError.write(Data(s.utf8))
    }

    /// `--probe-messages row:N` — for each message row, print the body,
    /// whether it has a profile button, and the textarea's screen X (to see if
    /// outgoing messages are right-aligned).
    static func probeMessages(roomId: String) {
        guard let ctx = try? Bridge.context(),
              let (row, title) = try? Bridge.resolveRoom(ctx, roomId: roomId)
        else {
            FileHandle.standardError.write(Data("cannot resolve \(roomId)\n".utf8))
            return
        }
        guard let container = try? Bridge.openConversation(ctx, row: row, title: title),
              let table = Bridge.conversationTable(ctx, container: container)
        else {
            FileHandle.standardError.write(Data("no conversation table\n".utf8))
            return
        }
        Bridge.scrollMessagesToBottom(ctx, container: container)
        Thread.sleep(forTimeInterval: 0.3)

        let winFrame = AX.frame(container.raw)
        var s = "title=\(title)  window=\(winFrame.map { "\($0)" } ?? "?")\n"
        for rowEl in AX.elements(table.raw, kAXChildrenAttribute as String).suffix(14) {
            guard AX.string(rowEl, kAXRoleAttribute as String) == "AXRow" else { continue }
            let cell = AX.elements(rowEl, kAXChildrenAttribute as String).first ?? rowEl
            let kids = AX.elements(cell, kAXChildrenAttribute as String).map { AXElement(raw: $0) }
            let hasProfile = kids.contains { $0.role == "AXButton" && $0.descriptionText == "프로필" }
            let ta = kids.first { $0.role == "AXTextArea" }
            let body = ta?.value ?? "(no textarea)"
            let x = ta.flatMap { AX.frame($0.raw) }?.minX
            let cellX = AX.frame(cell)?.minX
            s += "  profile=\(hasProfile ? "Y" : "n")  taX=\(x.map { String(format: "%.0f", $0) } ?? "-")  cellX=\(cellX.map { String(format: "%.0f", $0) } ?? "-")  body=\(body.prefix(24))\n"
        }
        FileHandle.standardError.write(Data(s.utf8))
    }

    static func actions(roomId: String) {
        guard let ctx = try? Bridge.context() else {
            FileHandle.standardError.write(Data("cannot get context\n".utf8))
            return
        }
        guard let (row, title) = try? Bridge.resolveRoom(ctx, roomId: roomId) else {
            FileHandle.standardError.write(Data("cannot resolve \(roomId)\n".utf8))
            return
        }
        var s = "\(roomId) -> title=\(title)\n"
        s += "  row actions:  \(Bridge.actions(row.raw))\n"
        if let cell = row.firstDescendant(where: { $0.role == "AXCell" }) as? AXElement {
            s += "  cell actions: \(Bridge.actions(cell.raw))\n"
            for (i, child) in cell.children.enumerated() {
                if let el = child as? AXElement {
                    s += "    child[\(i)] \(el.role) actions: \(Bridge.actions(el.raw))\n"
                }
            }
        }
        FileHandle.standardError.write(Data(s.utf8))
    }
}
