import Foundation

/// Serve mode — `kakao-macos-bridge serve`. A long-lived process the core's TUI
/// worker drives over newline-delimited JSON (contract §5):
///
///   in:   {"id":1,"method":"listRooms","params":{}}
///   out:  {"id":1,"ok":true,"data":{...}}   (correlated by id)
///   out:  {"event":"message","roomId":"row:3","message":{...}}   (unsolicited)
///
/// `watch` starts a background poller that emits `message` events for rows that
/// appear after the watch began. All accessibility access — the poller and the
/// request handlers alike — is serialized on `axLock`.
enum Serve {
    static let out = LineWriter()
    static let axLock = NSLock()
    static var watcher: Watcher?

    static func run() -> Never {
        // Serve mode manages the KakaoTalk window for the user: launch it,
        // un-minimise / reopen, and put it back on exit.
        Bridge.autoManage = true
        Bridge.priorState = WindowControl.capture()

        // stdin is line-delimited requests; EOF (core exited) ends the process.
        while let line = readLine(strippingNewline: true) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty { continue }
            handle(trimmed)
        }
        shutdown()
    }

    /// Stop the watcher, restore KakaoTalk's window state, exit.
    private static func shutdown() -> Never {
        watcher?.stop()
        if let prior = Bridge.priorState {
            axLock.lock()
            WindowControl.restore(prior)
            axLock.unlock()
        }
        exit(0)
    }

    private static func handle(_ line: String) {
        guard
            let data = line.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let method = obj["method"] as? String
        else {
            FileHandle.standardError.write(Data("serve: unparseable request\n".utf8))
            return
        }
        let id = (obj["id"] as? NSNumber)?.uint64Value ?? 0
        let params = obj["params"] as? [String: Any] ?? [:]

        switch method {
        case "listRooms":
            locked { do { out.send(ServeOk(id: id, data: try Bridge.listRooms())) }
                     catch let e as BridgeError { out.send(ServeErr(id: id, error: e.code)) }
                     catch { out.send(ServeErr(id: id, error: .uiElementNotFound)) } }

        case "openRoom":
            guard let roomId = params["roomId"] as? String else {
                return out.send(ServeErr(id: id, error: .roomNotFound))
            }
            locked { do { try Bridge.openRoom(roomId: roomId)
                          out.send(ServeOk(id: id, data: EmptyData())) }
                     catch let e as BridgeError { out.send(ServeErr(id: id, error: e.code)) }
                     catch { out.send(ServeErr(id: id, error: .uiElementNotFound)) } }

        case "readRecent":
            guard let roomId = params["roomId"] as? String else {
                return out.send(ServeErr(id: id, error: .roomNotFound))
            }
            let limit = (params["limit"] as? NSNumber)?.intValue ?? 40
            locked { do { out.send(ServeOk(id: id, data: try Bridge.readRecent(roomId: roomId, limit: limit))) }
                     catch let e as BridgeError { out.send(ServeErr(id: id, error: e.code)) }
                     catch { out.send(ServeErr(id: id, error: .uiElementNotFound)) } }

        case "sendText":
            guard let roomId = params["roomId"] as? String,
                  let text = params["text"] as? String else {
                return out.send(ServeErr(id: id, error: .sendInputFailed))
            }
            // sendText returns a SendResult even for handled failures.
            locked { do { out.send(ServeOk(id: id, data: try Bridge.sendText(roomId: roomId, text: text))) }
                     catch { out.send(ServeErr(id: id, error: .sendInputFailed)) } }

        case "healthCheck":
            locked { out.send(ServeOk(id: id, data: Bridge.healthCheck())) }

        case "watch":
            guard let roomId = params["roomId"] as? String else {
                return out.send(ServeErr(id: id, error: .roomNotFound))
            }
            watcher?.stop()
            let w = Watcher(roomId: roomId)
            watcher = w
            // Seed the baseline synchronously so the ack means "watching".
            locked {
                do {
                    w.seed(try Bridge.readMessagesForWatch(roomId: roomId))
                    out.send(ServeOk(id: id, data: EmptyData()))
                } catch let e as BridgeError {
                    out.send(ServeErr(id: id, error: e.code))
                } catch {
                    out.send(ServeErr(id: id, error: .uiElementNotFound))
                }
            }
            w.start()

        case "unwatch":
            watcher?.stop()
            watcher = nil
            out.send(ServeOk(id: id, data: EmptyData()))

        case "shutdown":
            shutdown()

        default:
            out.send(ServeErr(id: id, error: .uiElementNotFound))
        }
    }

    /// Run `body` holding the AX lock.
    private static func locked(_ body: () -> Void) {
        axLock.lock()
        defer { axLock.unlock() }
        body()
    }
}

/// Thread-safe single-line JSON writer for stdout.
final class LineWriter {
    private let lock = NSLock()
    private let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.outputFormatting = [.withoutEscapingSlashes]
        return e
    }()

    func send<T: Encodable>(_ value: T) {
        guard let data = try? encoder.encode(value) else { return }
        lock.lock()
        defer { lock.unlock() }
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data("\n".utf8))
    }
}

/// Polls one room's message tail and emits `message` events for new rows.
final class Watcher {
    let roomId: String
    private var lastKeys: Set<String> = []
    private var seeded = false
    private var running = false
    private var thread: Thread?
    private var missCount = 0

    init(roomId: String) { self.roomId = roomId }

    func seed(_ messages: [Message]) {
        lastKeys = Set(messages.map(Watcher.key))
        seeded = true
    }

    func start() {
        running = true
        let t = Thread { [weak self] in self?.loop() }
        t.stackSize = 1 << 20
        thread = t
        t.start()
    }

    func stop() {
        running = false
    }

    private func loop() {
        while running {
            Thread.sleep(forTimeInterval: 1.5)
            guard running else { return }

            Serve.axLock.lock()
            let result: Result<[Message], BridgeError>
            do {
                result = .success(try Bridge.readMessagesForWatch(roomId: roomId))
            } catch let e as BridgeError {
                result = .failure(e)
            } catch {
                result = .failure(BridgeError(.uiElementNotFound))
            }
            Serve.axLock.unlock()

            switch result {
            case .success(let messages):
                missCount = 0
                if !seeded {
                    seed(messages)
                    continue
                }
                for m in messages where !lastKeys.contains(Watcher.key(m)) {
                    Serve.out.send(MessageEvent(roomId: roomId, message: m))
                }
                lastKeys = Set(messages.map(Watcher.key))
            case .failure(let e):
                missCount += 1
                if e.code == .uiElementNotFound && missCount >= 2 {
                    Serve.out.send(RoomClosedEvent(roomId: roomId))
                } else {
                    Serve.out.send(ErrorEvent(code: e.code))
                }
            }
        }
    }

    private static func key(_ m: Message) -> String {
        "\(m.at)\u{1}\(m.sender)\u{1}\(m.outgoing ? "1" : "0")\u{1}\(m.text)"
    }
}
