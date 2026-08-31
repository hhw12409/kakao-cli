import Foundation
import AppKit
import ApplicationServices

/// Bringing KakaoTalk into a state kakao-cli can read, and putting it back.
///
/// The user shouldn't have to keep KakaoTalk running with a non-minimised
/// window. In serve mode (`Bridge.autoManage`) the bridge launches KakaoTalk if
/// needed, un-hides / un-minimises / reopens its window, and on shutdown
/// restores whatever it changed (re-minimise, re-hide, or quit if we launched
/// it). One thing it can't work around: KakaoTalk must be installed and logged
/// in at least once — with no process there is no accessibility tree.
enum WindowControl {

    /// Snapshot of how KakaoTalk looked before we touched it.
    struct PriorState {
        let wasRunning: Bool
        let wasHidden: Bool
        let hadAnyWindow: Bool
        /// Titles of windows that were minimised at capture time.
        let minimizedTitles: [String]
    }

    static func capture() -> PriorState {
        guard let running = KakaoApp.running() else {
            return PriorState(wasRunning: false, wasHidden: false,
                              hadAnyWindow: false, minimizedTitles: [])
        }
        let appRaw = AX.app(pid: running.pid)
        let hidden = AX.bool(appRaw, kAXHiddenAttribute as String) ?? false
        let windows = AX.elements(appRaw, kAXWindowsAttribute as String)
        let minimized = windows
            .filter { AX.bool($0, kAXMinimizedAttribute as String) ?? false }
            .compactMap { AX.string($0, kAXTitleAttribute as String) }
        return PriorState(wasRunning: true,
                          wasHidden: hidden,
                          hadAnyWindow: !windows.isEmpty,
                          minimizedTitles: minimized)
    }

    /// A window exists that is not minimised AND has rendered its contents
    /// (a just-un-minimised window reports `minimized=false` a beat before its
    /// subtree is populated — reading then yields `UI_ELEMENT_NOT_FOUND`).
    private static func hasReadableWindow(_ appRaw: AXUIElement) -> Bool {
        AX.elements(appRaw, kAXWindowsAttribute as String).contains { w in
            !(AX.bool(w, kAXMinimizedAttribute as String) ?? false)
                && !AX.elements(w, kAXChildrenAttribute as String).isEmpty
        }
    }

    /// Poll `hasReadableWindow` for up to `timeout`.
    private static func waitReadable(_ appRaw: AXUIElement, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if hasReadableWindow(appRaw) { return true }
            Thread.sleep(forTimeInterval: 0.15)
        } while Date() < deadline
        return false
    }

    /// Ensure KakaoTalk is running with a readable window. Escalating steps,
    /// re-checking after each. Throws `.kakaoNotRunning` / `.kakaoWindowNotVisible`
    /// if it can't get there (caller falls back to the cached, read-only view).
    static func ensureAwake() throws {
        if KakaoApp.running() == nil {
            guard Bridge.autoManage else { throw BridgeError(.kakaoNotRunning) }
            guard KakaoApp.launch() else {
                throw BridgeError(.kakaoNotRunning, "KakaoTalk 앱을 찾을 수 없습니다 (설치 필요)")
            }
            guard KakaoApp.waitUntilWindowReady(timeout: 12) else {
                throw BridgeError(.kakaoNotRunning,
                                  "카카오톡을 실행했지만 준비되지 않았습니다 (로그인 필요?)")
            }
        }

        guard let running = KakaoApp.running() else { throw BridgeError(.kakaoNotRunning) }
        let appRaw = AX.app(pid: running.pid)
        if hasReadableWindow(appRaw) { return }

        // 1. un-hide (⌘H).
        AX.setValue(appRaw, kAXHiddenAttribute as String, kCFBooleanFalse as CFTypeRef)
        if waitReadable(appRaw, timeout: 0.5) { return }

        // 2. un-minimise every minimised window, then wait for it to render.
        for w in AX.elements(appRaw, kAXWindowsAttribute as String)
        where AX.bool(w, kAXMinimizedAttribute as String) ?? false {
            AX.setValue(w, kAXMinimizedAttribute as String, kCFBooleanFalse as CFTypeRef)
        }
        if waitReadable(appRaw, timeout: 2.5) { return }

        // 3. ask the app to reopen its main window (running app + `open` =
        //    applicationShouldHandleReopen).
        running.app.activate()
        _ = KakaoApp.launch()
        if waitReadable(appRaw, timeout: 3.0) { return }

        throw BridgeError(.kakaoWindowNotVisible, "카카오톡 창을 띄우지 못했습니다")
    }

    /// Undo what `ensureAwake` changed, best effort. Called on serve shutdown.
    static func restore(_ prior: PriorState) {
        guard Bridge.autoManage else { return }

        // We launched it: quit, unless the user asked us to leave it.
        if !prior.wasRunning {
            if ProcessInfo.processInfo.environment["KAKAO_CLI_KEEP_KAKAO"] == nil {
                KakaoApp.running()?.app.terminate()
            }
            return
        }

        guard let running = KakaoApp.running() else { return }
        let appRaw = AX.app(pid: running.pid)

        if prior.wasHidden {
            AX.setValue(appRaw, kAXHiddenAttribute as String, kCFBooleanTrue as CFTypeRef)
            return
        }

        let windows = AX.elements(appRaw, kAXWindowsAttribute as String)
        if prior.hadAnyWindow {
            // Re-minimise the windows that were minimised before.
            for w in windows {
                if let title = AX.string(w, kAXTitleAttribute as String),
                   prior.minimizedTitles.contains(title) {
                    AX.setValue(w, kAXMinimizedAttribute as String, kCFBooleanTrue as CFTypeRef)
                }
            }
        } else {
            // It was closed to the menu bar; minimise whatever we opened
            // (AX can't close-to-tray reliably, minimise is the safe echo).
            for w in windows {
                AX.setValue(w, kAXMinimizedAttribute as String, kCFBooleanTrue as CFTypeRef)
            }
        }
    }
}
