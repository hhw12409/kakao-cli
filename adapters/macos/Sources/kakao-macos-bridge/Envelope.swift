import Foundation

/// Writes the single-line JSON response and exits. Contract §1.
enum Envelope {
    private struct OkWrapper<T: Encodable>: Encodable {
        let ok = true
        let data: T
    }
    private struct ErrWrapper: Encodable {
        let ok = false
        let error: ErrorCode
    }

    static func ok<T: Encodable>(_ data: T) -> Never {
        emit(OkWrapper(data: data))
        exit(0)
    }

    static func okEmpty() -> Never {
        emit(OkWrapper(data: [String: String]()))
        exit(0)
    }

    static func error(_ code: ErrorCode) -> Never {
        emit(ErrWrapper(error: code))
        exit(0)
    }

    /// The bridge itself failed (bad args, unknown method, unexpected throw).
    /// Non-zero exit so the core promotes it to an internal error.
    static func crash(_ message: String) -> Never {
        FileHandle.standardError.write(Data("bridge crash: \(message)\n".utf8))
        exit(70)
    }

    private static func emit<T: Encodable>(_ value: T) {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        guard let data = try? encoder.encode(value) else {
            FileHandle.standardError.write(Data("bridge crash: response encode failed\n".utf8))
            exit(71)
        }
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data("\n".utf8))
    }
}
