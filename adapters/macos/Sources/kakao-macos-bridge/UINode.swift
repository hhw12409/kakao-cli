import Foundation

/// A read-only view of one accessibility element. Both the live AX tree and a
/// serialized fixture conform, so the parsers (`ListRooms`, `ReadRecent`) run
/// unchanged in tests without KakaoTalk.
protocol UINode {
    var role: String { get }
    var subrole: String? { get }
    var title: String? { get }
    var value: String? { get }
    var descriptionText: String? { get }
    var identifier: String? { get }
    var children: [UINode] { get }
    /// Screen-space left edge, when captured. Only message-body text areas
    /// carry this (to tell outgoing right-aligned bubbles from incoming).
    var frameMinX: Double? { get }
}

extension UINode {
    var frameMinX: Double? { nil }
}

extension UINode {
    /// Depth-first search for the first descendant (including self) matching
    /// `predicate`.
    func firstDescendant(where predicate: (UINode) -> Bool) -> UINode? {
        if predicate(self) { return self }
        for child in children {
            if let hit = child.firstDescendant(where: predicate) { return hit }
        }
        return nil
    }

    /// All descendants (excluding self) matching `predicate`.
    func descendants(where predicate: (UINode) -> Bool) -> [UINode] {
        var out: [UINode] = []
        for child in children {
            if predicate(child) { out.append(child) }
            out.append(contentsOf: child.descendants(where: predicate))
        }
        return out
    }

    /// First non-empty text among value / title / description, trimmed.
    var anyText: String? {
        for candidate in [value, title, descriptionText] {
            if let t = candidate?.trimmingCharacters(in: .whitespacesAndNewlines), !t.isEmpty {
                return t
            }
        }
        return nil
    }
}

// MARK: - Fixture node (Codable) — used by tests and `--dump-tree` output

struct FixtureNode: UINode, Codable {
    let role: String
    let subrole: String?
    let title: String?
    let value: String?
    let descriptionText: String?
    let identifier: String?
    let frameMinX: Double?
    let fixtureChildren: [FixtureNode]?

    var children: [UINode] { fixtureChildren ?? [] }

    enum CodingKeys: String, CodingKey {
        case role, subrole, title, value
        case descriptionText = "description"
        case identifier, frameMinX
        case fixtureChildren = "children"
    }

    init(
        role: String,
        subrole: String? = nil,
        title: String? = nil,
        value: String? = nil,
        descriptionText: String? = nil,
        identifier: String? = nil,
        frameMinX: Double? = nil,
        children: [FixtureNode] = []
    ) {
        self.role = role
        self.subrole = subrole
        self.title = title
        self.value = value
        self.descriptionText = descriptionText
        self.identifier = identifier
        self.frameMinX = frameMinX
        self.fixtureChildren = children
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        role = try c.decode(String.self, forKey: .role)
        subrole = try c.decodeIfPresent(String.self, forKey: .subrole)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        value = try c.decodeIfPresent(String.self, forKey: .value)
        descriptionText = try c.decodeIfPresent(String.self, forKey: .descriptionText)
        identifier = try c.decodeIfPresent(String.self, forKey: .identifier)
        frameMinX = try c.decodeIfPresent(Double.self, forKey: .frameMinX)
        fixtureChildren = try c.decodeIfPresent([FixtureNode].self, forKey: .fixtureChildren)
    }
}
