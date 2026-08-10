#!/usr/bin/env swift
//
// mac-synthetic-input.swift — post keyboard CGEvents that Quicuts' macOS
// agent will actually recognise as activation input. A test harness, not part
// of the shipped app. Run it via `just mac-input <subcommand>`.
//
// ┌───────────────────────────────────────────────────────────────────────┐
// │ SAFETY — READ BEFORE RUNNING                                          │
// │                                                                       │
// │ Synthetic events are posted at the session event tap, so they go to   │
// │ WHATEVER APP CURRENTLY HAS FOCUS. They are NOT delivered to Quicuts.  │
// │ Quicuts only sees them because its tap observes the whole session.    │
// │                                                                       │
// │ A stray Esc dismisses whatever dialog or sheet is open. A stray       │
// │ keystroke replaces the selection in whatever editor is frontmost.     │
// │ Either can destroy unsaved work, and did nearly do so on 2026-08-10.  │
// │                                                                       │
// │ Only run this on an idle machine with nothing important focused —     │
// │ check the frontmost app first, and prefer a scratch window. If you    │
// │ only need to exercise the overlay render path, use the tray menu's    │
// │ "Show shortcuts" item instead; it posts no events at all.             │
// └───────────────────────────────────────────────────────────────────────┘
//
// WHY THIS SCRIPT EXISTS (the non-obvious part)
//
// `osascript -e 'keystroke "/" using {command down, control down}'` can never
// activate Quicuts, and neither can any CGEvent that sets only the documented
// CGEventFlags masks. The reason is in
// `crates/quicuts-agent-mac/src/tap.rs::translate`:
//
//     CGEventType::FlagsChanged =>
//         let down = event.get_flags().bits() & device_bit(keycode)? != 0;
//
// A `.flagsChanged` event says which modifier key changed but not which
// direction it moved, and the generic `maskCommand` bit cannot answer that —
// it stays set as long as *either* ⌘ key is held, so it is 1 on the press of
// the second ⌘ and still 1 on the release of the first. The agent therefore
// reads the direction from the DEVICE-DEPENDENT `NX_DEVICE*` flag bits
// (`device_bit()`: 0x1 left ⌃, 0x2 left ⇧, 0x8 left ⌘, …), which are
// per-side and so do distinguish press from release. Real hardware events
// always carry those bits; AppleScript sets only the generic ones, so the
// state machine sees every modifier as already released and nothing fires.
//
// So every event below sets BOTH bit sets: the generic mask (what the rest of
// the system expects to see) and the device-dependent bit (what the agent
// reads). Dropping either half breaks it — the generic bit alone is invisible
// to Quicuts, the device bit alone is invisible to everything else.
//
// This is a property of the agent, deliberately, not a bug to fix.
//
// Requires the Accessibility TCC grant for the *responsible process*, which
// for a script run from a terminal is the terminal app — the same grant
// `just mac-run` already needs. See docs/macos-dev.md.

import CoreGraphics
import Foundation

// CGKeyCodes (ANSI positions).
let VK_SLASH: CGKeyCode = 0x2C
let VK_ESC: CGKeyCode = 0x35
let VK_CMD: CGKeyCode = 0x37
let VK_CTRL: CGKeyCode = 0x3B

// NX_DEVICE* device-dependent bits, mirroring tap.rs::device_bit().
let DEV_LCTRL: UInt64 = 0x1
let DEV_LCMD: UInt64 = 0x8

// The generic masks, which the agent ignores and everything else needs.
let GEN_CTRL = CGEventFlags.maskControl.rawValue
let GEN_CMD = CGEventFlags.maskCommand.rawValue

let DEFAULT_HOLD_MS: UInt32 = 1400

let usage = """
usage: mac-synthetic-input.swift <subcommand> [hold-ms]

  hold [ms]   press ⌘, hold it for ms (default \(DEFAULT_HOLD_MS)), release
  holddown    press ⌘ and leave it held — pair with `cmdup`
  cmdup       release ⌘ (the other half of `holddown`)
  chord       the ⌃⌘/ activation chord, press and release
  esc         tap Esc (dismisses the panel when it is visible)
  help        this text

WARNING: events go to the app that currently has FOCUS, not to Quicuts. A
stray Esc or keystroke can dismiss a dialog or replace an editor selection
and lose unsaved work. Idle machine, nothing important focused.
"""

func die(_ message: String) -> Never {
    FileHandle.standardError.write("\(message)\n\n\(usage)\n".data(using: .utf8)!)
    exit(2)
}

func note(_ message: String) {
    FileHandle.standardError.write("\(message) → focused app\n".data(using: .utf8)!)
}

let src = CGEventSource(stateID: .hidSystemState)

/// A modifier transition: `.flagsChanged` for `key`, with `bits` describing
/// the state *after* the transition (0 = everything released).
func flags(_ key: CGKeyCode, _ bits: UInt64) {
    guard let e = CGEvent(keyboardEventSource: src, virtualKey: key, keyDown: true) else { return }
    e.type = .flagsChanged
    e.flags = CGEventFlags(rawValue: bits)
    e.post(tap: .cgSessionEventTap)
}

func key(_ k: CGKeyCode, _ down: Bool, _ bits: UInt64 = 0) {
    guard let e = CGEvent(keyboardEventSource: src, virtualKey: k, keyDown: down) else { return }
    e.flags = CGEventFlags(rawValue: bits)
    e.post(tap: .cgSessionEventTap)
}

let args = Array(CommandLine.arguments.dropFirst())
guard let cmd = args.first else { die("no subcommand given") }

if cmd == "help" || cmd == "-h" || cmd == "--help" {
    print(usage)
    exit(0)
}

// A 30ms gap between the events of a chord: enough that the agent's state
// machine sees them as distinct transitions rather than one burst.
let step: UInt32 = 30_000

switch cmd {
case "hold":
    var ms = DEFAULT_HOLD_MS
    if let raw = args.dropFirst().first {
        guard let parsed = UInt32(raw), parsed > 0, parsed <= 60_000 else {
            die("bad hold-ms '\(raw)': want an integer 1..60000")
        }
        ms = parsed
    }
    flags(VK_CMD, GEN_CMD | DEV_LCMD)
    usleep(ms * 1000)
    flags(VK_CMD, 0)
    note("hold ⌘ \(ms)ms")

case "holddown":
    flags(VK_CMD, GEN_CMD | DEV_LCMD)
    note("⌘ down (call `cmdup` to release)")

case "cmdup":
    flags(VK_CMD, 0)
    note("⌘ up")

case "chord":
    flags(VK_CTRL, GEN_CTRL | DEV_LCTRL)
    usleep(step)
    flags(VK_CMD, GEN_CTRL | GEN_CMD | DEV_LCTRL | DEV_LCMD)
    usleep(step)
    key(VK_SLASH, true, GEN_CTRL | GEN_CMD | DEV_LCTRL | DEV_LCMD)
    usleep(step)
    key(VK_SLASH, false, GEN_CTRL | GEN_CMD | DEV_LCTRL | DEV_LCMD)
    usleep(step)
    flags(VK_CMD, GEN_CTRL | DEV_LCTRL)
    usleep(step)
    flags(VK_CTRL, 0)
    note("chord ⌃⌘/")

case "esc":
    key(VK_ESC, true)
    usleep(step)
    key(VK_ESC, false)
    note("esc")

default:
    die("unknown subcommand '\(cmd)'")
}
