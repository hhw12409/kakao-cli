import Foundation

/// kakao-macos-bridge — subprocess adapter for kakao-cli.
///
/// Invocation (contract §1):  kakao-macos-bridge <method> <argsJson>
///   methods: listRooms | openRoom | readRecent | sendText | healthCheck
///
/// Output: exactly one line of JSON on stdout:
///   {"ok":true,"data":<...>}   or   {"ok":false,"error":"<CODE>"}
/// Exit code is always 0 for handled results; non-zero means the bridge itself
/// crashed. Diagnostics go to stderr and never contain message bodies.

setbuf(stdout, nil)

let args = Array(CommandLine.arguments.dropFirst())

// Dev helper: dump the KakaoTalk accessibility tree as fixture JSON.
if args.first == "--dump-tree" {
    DumpTree.run()
    exit(0)
}

// Parser regression checks against bundled fixtures. No KakaoTalk needed.
if args.first == "--self-test" {
    SelfTest.run()
}

// Debug: list current KakaoTalk window titles.
if args.first == "--windows" {
    Debug.windows()
    exit(0)
}

// Debug: show the AXActions available on a chat-list row. `--actions row:3`
if args.first == "--actions" {
    Debug.actions(roomId: args.count > 1 ? args[1] : "row:0")
    exit(0)
}

// Debug: per-message profile/position, to find the outgoing signal.
if args.first == "--probe-messages" {
    Debug.probeMessages(roomId: args.count > 1 ? args[1] : "row:0")
    exit(0)
}

guard let method = args.first else {
    Envelope.crash("missing method argument")
}
let argsJson = args.count > 1 ? args[1] : "{}"

func decodeArgs<T: Decodable>(_ type: T.Type) -> T {
    guard let data = argsJson.data(using: .utf8),
          let value = try? JSONDecoder().decode(T.self, from: data)
    else {
        Envelope.crash("could not decode args for \(method)")
    }
    return value
}

struct RoomIdArg: Decodable { let roomId: String }
struct ReadRecentArg: Decodable { let roomId: String; let limit: Int }
struct SendTextArg: Decodable { let roomId: String; let text: String }

do {
    switch method {
    case "listRooms":
        Envelope.ok(try Bridge.listRooms())

    case "openRoom":
        let a = decodeArgs(RoomIdArg.self)
        try Bridge.openRoom(roomId: a.roomId)
        Envelope.okEmpty()

    case "readRecent":
        let a = decodeArgs(ReadRecentArg.self)
        Envelope.ok(try Bridge.readRecent(roomId: a.roomId, limit: a.limit))

    case "sendText":
        let a = decodeArgs(SendTextArg.self)
        // sendText never throws for handled failures — it returns a SendResult.
        Envelope.ok(try Bridge.sendText(roomId: a.roomId, text: a.text))

    case "healthCheck":
        Envelope.ok(Bridge.healthCheck())

    default:
        Envelope.crash("unknown method: \(method)")
    }
} catch let e as BridgeError {
    if let d = e.diagnostic { FileHandle.standardError.write(Data("\(method): \(d)\n".utf8)) }
    Envelope.error(e.code)
} catch {
    Envelope.crash("unexpected: \(error)")
}
