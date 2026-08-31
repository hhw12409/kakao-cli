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
