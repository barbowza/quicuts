#!/usr/bin/env swift
//
// mac-menu-shortcuts.swift — dump a running Mac app's menu-bar keyboard
// shortcuts as TSV, for authoring a `manifests-mac/` collection. An authoring
// tool, not part of the shipped app. Run it via `just mac-menus <app>`.
//
// WHY THIS EXISTS
//
// Most apps we want a mac manifest for have no canonical shortcut article the
// way Evernote and Chrome do, and where an article does exist it lags the
// build the user is actually running. An app's own menu bar cannot lag: it is
// what the app will really do. macOS exposes it through the Accessibility API,
// read-only, so this needs no synthetic input and cannot disturb the app —
// unlike `mac-input`, there is nothing dangerous here.
//
// It emits TSV rather than YAML on purpose. Turning menus into a manifest
// needs judgement this script should not fake: which submenus deserve their
// own SectionName, which entries are worth Recommended, how to word a name.
// What it does do is decode the shortcut correctly and flag the rows a human
// would otherwise import by mistake — see the two traps below.
//
// TRAP 1 — THE COMMAND BIT IS INVERTED
//
// `AXMenuItemCmdModifiers` is a bitmask, but it does NOT have a "command" bit.
// It has a *no command* bit:
//
//     0x01  shift
//     0x02  option
//     0x04  control
//     0x08  command is NOT part of this shortcut
//     0x10  fn / Globe
//
// So a mask of 0 means plain ⌘, and the natural reading ("0 = no modifiers")
// gets every single shortcut in the app wrong — silently, since the output
// still looks plausible. Quicuts' manifests map ⌘ to the PTSG `Win` flag, so
// this inversion is exactly what decides `Win: true` vs `Win: false`.
//
// TRAP 2 — 0x10 (fn) ALSO MARKS ITEMS THAT ARE NOT THE APP'S
//
// macOS injects its own items into every app's menus, and they carry the fn
// bit: the Sequoia window-tiling block in the Window menu (Fill, Center, Move
// & Resize ▸ Left/Right/Top/Bottom and the halves/quarters variants), plus
// Start Dictation and Emoji & Symbols in Edit. Those are system-wide, belong
// in `Apple.System` if anywhere, and must not be copied into a per-app
// manifest — importing them yields wrong entries like `⌃F` for "Fill".
//
// Rows are therefore tagged in the `notes` column:
//
//     system-fn    fn/Globe is part of the shortcut. Usually a macOS-injected
//                  item — check before keeping. A few are genuinely the app's
//                  (iTerm2's View ▸ Toggle Full Screen is Globe+F); keep those
//                  as a two-keycap [Fn, X] sequence, since fn has no PTSG flag.
//     apple-menu   From the Apple menu. Never app-specific; always skip.
//
// One more thing no flag can catch: menus contain USER DATA. Window ▸
// Arrangements ▸ Restore Window Arrangement lists arrangements saved on *this*
// Mac, and Profiles lists the user's profiles. They look like ordinary rows.
// Read the titles before committing anything.
//
// USAGE
//
//     just mac-menus iTerm                     # by app name
//     just mac-menus com.googlecode.iterm2     # or by bundle id
//     just mac-menus iTerm > /tmp/menus.tsv
//
// The app must already be RUNNING (this reads a live process). Needs the same
// one-time Accessibility grant as the rest of the mac dev loop — see
// docs/macos-dev.md. Without it AXUIElementCopyAttributeValue returns nothing
// and the script exits non-zero saying so.
//
// OUTPUT — TSV, one row per menu item that has a shortcut:
//
//     menu_path <TAB> title <TAB> modifiers <TAB> key <TAB> notes
//
// `modifiers` is a `+`-joined subset of cmd/shift/opt/ctrl/fn in that order.
// `key` is already normalised to the PTSG `Keys` grammar a manifest wants —
// `A`, `<1>`, `<Up>`, `<Escape>`, `Tab`, `/` — so it can be pasted straight in.

import ApplicationServices
import AppKit
import Foundation

// MARK: - Locate the target app

let args = CommandLine.arguments
guard args.count == 2 else {
    FileHandle.standardError.write(
        "usage: mac-menu-shortcuts.swift <app-name-or-bundle-id>\n".data(using: .utf8)!)
    exit(2)
}
let needle = args[1].lowercased()

// Exact matches first so a precise argument always wins, then a forgiving
// substring pass ("iTerm" -> iTerm2). The chosen app is echoed to stderr, so
// a wrong guess is visible rather than silently producing the wrong manifest.
let apps = NSWorkspace.shared.runningApplications.filter { $0.activationPolicy == .regular }
let running =
    apps.first {
        $0.bundleIdentifier?.lowercased() == needle || $0.localizedName?.lowercased() == needle
    }
    ?? apps.first {
        ($0.bundleIdentifier?.lowercased().split(separator: ".").last).map(String.init) == needle
    }
    ?? apps.first {
        ($0.localizedName?.lowercased().contains(needle) ?? false)
            || ($0.bundleIdentifier?.lowercased().contains(needle) ?? false)
    }
guard let target = running, let pid = running?.processIdentifier else {
    FileHandle.standardError.write(
        "no RUNNING app matches \(args[1]). Launch it first; this reads a live process.\n"
            .data(using: .utf8)!)
    exit(1)
}
FileHandle.standardError.write(
    "reading menus of \(target.localizedName ?? "?") (\(target.bundleIdentifier ?? "?"), pid \(pid))\n"
        .data(using: .utf8)!)

// MARK: - Accessibility plumbing

func attr(_ el: AXUIElement, _ name: String) -> CFTypeRef? {
    var v: CFTypeRef?
    return AXUIElementCopyAttributeValue(el, name as CFString, &v) == .success ? v : nil
}
func children(_ el: AXUIElement) -> [AXUIElement] {
    (attr(el, kAXChildrenAttribute as String) as? [AXUIElement]) ?? []
}
func str(_ el: AXUIElement, _ n: String) -> String? { attr(el, n) as? String }
func int(_ el: AXUIElement, _ n: String) -> Int? { (attr(el, n) as? NSNumber)?.intValue }

let app = AXUIElementCreateApplication(pid)
guard let barRef = attr(app, kAXMenuBarAttribute as String) else {
    FileHandle.standardError.write(
        """
        could not read the menu bar. This is almost always the Accessibility
        grant: System Settings > Privacy & Security > Accessibility > enable
        the terminal running this. See docs/macos-dev.md.

        """.data(using: .utf8)!)
    exit(1)
}
let menuBar = barRef as! AXUIElement

// MARK: - Shortcut decoding

/// `AXMenuItemCmdModifiers` — note 0x08 is *no* command. See TRAP 1 above.
struct Mods {
    let cmd: Bool, shift: Bool, opt: Bool, ctrl: Bool, fn: Bool
    init(_ mask: Int) {
        cmd = (mask & 0x08) == 0
        shift = (mask & 0x01) != 0
        opt = (mask & 0x02) != 0
        ctrl = (mask & 0x04) != 0
        fn = (mask & 0x10) != 0
    }
    var text: String {
        var p: [String] = []
        if cmd { p.append("cmd") }
        if shift { p.append("shift") }
        if opt { p.append("opt") }
        if ctrl { p.append("ctrl") }
        if fn { p.append("fn") }
        return p.joined(separator: "+")
    }
}

/// AppKit reports arrows and function keys as private-use scalars (NSUpArrow-
/// FunctionKey = 0xF700 and friends), not as anything printable.
let functionKeyTokens: [UInt32: String] = [
    0xF700: "<Up>", 0xF701: "<Down>", 0xF702: "<Left>", 0xF703: "<Right>",
    0xF704: "F1", 0xF705: "F2", 0xF706: "F3", 0xF707: "F4", 0xF708: "F5",
    0xF709: "F6", 0xF70A: "F7", 0xF70B: "F8", 0xF70C: "F9", 0xF70D: "F10",
    0xF70E: "F11", 0xF70F: "F12",
    0xF728: "<Delete>", 0xF729: "<Home>", 0xF72B: "<End>",
    0xF72C: "<PageUp>", 0xF72D: "<PageDown>",
]

/// Fallback when the item carries no printable command character.
let virtualKeyTokens: [Int: String] = [
    0x24: "<Enter>", 0x30: "Tab", 0x31: "Space", 0x33: "<Backspace>", 0x35: "<Escape>",
    0x73: "<Home>", 0x74: "<PageUp>", 0x75: "<Delete>", 0x77: "<End>", 0x79: "<PageDown>",
    0x7B: "<Left>", 0x7C: "<Right>", 0x7D: "<Down>", 0x7E: "<Up>",
    0x7A: "F1", 0x78: "F2", 0x63: "F3", 0x76: "F4", 0x60: "F5", 0x61: "F6",
    0x62: "F7", 0x64: "F8", 0x65: "F9", 0x6D: "F10", 0x67: "F11", 0x6F: "F12",
]

/// Normalise into the PTSG `Keys` grammar the manifests use: bare letters
/// uppercase, digits wrapped as `<n>` (PTSG's escape hatch, so they render as
/// the digit rather than a virtual-key code), named keys in angle brackets.
func keyToken(char: String?, virtualKey: Int?) -> String? {
    if let c = char, let scalar = c.unicodeScalars.first, c.count == 1 {
        if let named = functionKeyTokens[scalar.value] { return named }
        if scalar.value >= 0xF700 { return nil }  // unmapped private-use glyph
        if c == "\u{1B}" { return "<Escape>" }
        if c == "\t" { return "Tab" }
        if c == "\r" || c == "\n" { return "<Enter>" }
        if c == " " { return "Space" }
        if c.rangeOfCharacter(from: .letters) != nil { return c.uppercased() }
        if let d = c.first, d.isNumber { return "<\(c)>" }
        if scalar.isASCII && scalar.value > 32 { return c }  // punctuation: / , ; [ ] \ + -
    }
    if let vk = virtualKey, let named = virtualKeyTokens[vk] { return named }
    return nil
}

// MARK: - Walk

var rows = 0
var skippedUnresolved = 0

func walk(_ element: AXUIElement, path: [String], depth: Int) {
    for child in children(element) {
        let title = str(child, kAXTitleAttribute as String) ?? ""
        let char = str(child, "AXMenuItemCmdChar")
        let mask = int(child, "AXMenuItemCmdModifiers")
        let vkey = int(child, "AXMenuItemCmdVirtualKey")
        let kids = children(child)

        if !title.isEmpty, char != nil || vkey != nil {
            let mods = Mods(mask ?? 0)
            if let key = keyToken(char: char, virtualKey: vkey) {
                var notes: [String] = []
                if path.first == "Apple" { notes.append("apple-menu") }
                if mods.fn { notes.append("system-fn") }
                let fields = [
                    path.joined(separator: " > "), title, mods.text, key,
                    notes.joined(separator: ","),
                ]
                print(fields.joined(separator: "\t"))
                rows += 1
            } else {
                skippedUnresolved += 1
                FileHandle.standardError.write(
                    "unresolved key for \(path.joined(separator: " > ")) > \(title) "
                        .appending("(char \(char.map { String(reflecting: $0) } ?? "nil"), ")
                        .appending("vk \(vkey.map(String.init) ?? "nil"))\n")
                        .data(using: .utf8)!)
            }
        }
        if !kids.isEmpty, depth < 8 {
            walk(child, path: title.isEmpty ? path : path + [title], depth: depth + 1)
        }
    }
}

print("# menu_path\ttitle\tmodifiers\tkey\tnotes")
walk(menuBar, path: [], depth: 0)

FileHandle.standardError.write(
    "\(rows) shortcuts emitted\(skippedUnresolved > 0 ? ", \(skippedUnresolved) unresolved (see above)" : "")\n"
        .data(using: .utf8)!)
