import Foundation
import ApplicationServices

/// `--windows` / `--actions` dev helpers. Not part of the contract. Output
/// goes to stderr so it never contaminates a JSON response.
enum Debug {
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
