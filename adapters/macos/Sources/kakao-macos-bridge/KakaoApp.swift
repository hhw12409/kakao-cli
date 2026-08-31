import Foundation
import AppKit
import ApplicationServices

/// Locating — and, in serve mode, launching — the KakaoTalk Mac app.
/// One-shot dispatch (`doctor`) never launches; it reports `KAKAO_NOT_RUNNING`.
enum KakaoApp {
    /// Candidate bundle identifiers. MUST be verified against a real install;
    /// the first match wins. Add observed values here rather than guessing in
    /// call sites.
    static let bundleIdCandidates = [
        "com.kakao.KakaoTalkMac",
        "com.kakao.KakaoTalk",
    ]

    struct Running {
        let app: NSRunningApplication
        let pid: pid_t
        let bundleId: String
    }

    static func running() -> Running? {
        for id in bundleIdCandidates {
            if let app = NSRunningApplication
                .runningApplications(withBundleIdentifier: id)
                .first(where: { !$0.isTerminated })
            {
                return Running(app: app, pid: app.processIdentifier, bundleId: id)
            }
        }
        return nil
    }

    /// `CFBundleShortVersionString` from the on-disk bundle of a running
    /// instance. `nil` when the bundle or key is unreadable.
    static func version(of running: Running) -> String? {
        guard let url = running.app.bundleURL,
              let bundle = Bundle(url: url)
        else { return nil }
        return bundle.infoDictionary?["CFBundleShortVersionString"] as? String
    }

    /// Launch the installed KakaoTalk (or, if already running, ask it to
    /// reopen its window). `false` when no KakaoTalk bundle is installed.
    @discardableResult
    static func launch() -> Bool {
        let ws = NSWorkspace.shared
        for id in bundleIdCandidates {
            guard let url = ws.urlForApplication(withBundleIdentifier: id) else { continue }
            let cfg = NSWorkspace.OpenConfiguration()
            cfg.activates = true
            cfg.addsToRecentItems = false
            let sem = DispatchSemaphore(value: 0)
            ws.openApplication(at: url, configuration: cfg) { _, _ in sem.signal() }
            _ = sem.wait(timeout: .now() + 6)
            return true
        }
        return false
    }

    /// Poll until KakaoTalk is running and has at least one window in its AX
    /// tree, or `timeout` elapses. A window at the login screen counts — the
    /// caller surfaces "login needed" from there.
    static func waitUntilWindowReady(timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let running = running(),
               !AX.elements(AX.app(pid: running.pid), kAXWindowsAttribute as String).isEmpty {
                return true
            }
            Thread.sleep(forTimeInterval: 0.4)
        }
        return false
    }
}
