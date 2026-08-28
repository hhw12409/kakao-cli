import Foundation
import AppKit

/// Locating the running KakaoTalk Mac app. We never launch it ourselves
/// (contract: `KAKAO_NOT_RUNNING` + guidance instead).
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
}
