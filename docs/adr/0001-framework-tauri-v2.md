# ADR 0001 — Framework: Tauri v2

Status: accepted (2026-07-02)

## Context

Quicuts is a PowerToys Shortcut Guide clone that must run on Windows and macOS,
be developed from WSL2, and satisfy hard requirements: a low-level keyboard
hook that detects a *held* modifier and can *suppress* the Win/Cmd key-up; a
frameless transparent always-on-top overlay; foreground-app detection; and
tray residency. We evaluated Tauri v2, Electron, Avalonia, and Flutter.

## Decision

**Tauri v2** (Rust core + Svelte/web UI).

Rationale:
- Smallest footprint for an always-resident utility (~10 MB, low idle RAM).
- The one genuinely hard requirement — a WH_KEYBOARD_LL hook with Win-key
  suppression (PowerToys' own mechanism) — is implementable natively in Rust
  in-process, no FFI wall. On macOS the equivalent is CGEventTap.
- Built-in tray, autostart plugin, transparent/click-through windows.
- The user requires system input access to live in a **separate sidecar
  process** so AV heuristics don't flag the main binary as a keylogger.
  Tauri's sidecar (`externalBin`) model fits this exactly.

Electron was the runner-up (every requirement has a mature library) but its
~100 MB / 150–300 MB-RAM residency is wrong for a cheat-sheet, and its hooks
are listen-only (Win suppression would be a hack). Avalonia's SharpHook is the
only off-the-shelf hook with built-in suppression, but click-through /
above-fullscreen / foreground detection are all manual native interop.

## Consequences

- Two-language stack (Rust + TS).
- Win-key suppression is bespoke Rust (done: `quicuts-agent-win`).
- macOS is a later phase; the IPC protocol is the platform seam.
- Cross-compiling the app from WSL needs clang-cl + llvm-rc (see ADR 0002).
