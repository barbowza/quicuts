# Collaboration model (Michael ↔ pro-win-quicuts ↔ pro-wsl-quicuts ↔ zax-quicuts ↔ mac-quicuts)

How five actors — one human and four Claude Code sessions on three machines —
build Quicuts without treading on each other.

This document is the **model**: who decides what, which channel is
authoritative, and where the line to Michael sits. The *review procedure* (the
`gh` incantations, round shape, templates) lives in the `two-agent-flow` skill.
The *rules and the incidents that produced them* live in
`docs/two-agent-review-process.md`. The one-page summary that both sessions
always have in context is in `CLAUDE.md`.

The sessions are named `<machine>-<env>-<project>`. The lead is
`pro-win-quicuts`, the native Windows clone on Prometheus; `pro-wsl-quicuts`,
the WSL clone on the same box, held the role until issue #43 (2026-10-13).
Until 2026-10-13 the sessions were `win-claude` and `mac-claude`. That's
what older PRs, issues and comment tags say, and this document has been
renamed throughout, including the incidents that predate the change. Where an
incident below names pro-wsl-quicuts as the lead, that is who held the role at
the time.

## The actors

| | Machine | Owns |
|---|---|---|
| **Michael** | the Windows box | Scope, priorities, on-device testing, bringing mac-quicuts online. Product delivery management — **not** development. |
| **pro-win-quicuts** | that box, natively (PowerShell) | Lead. Design decisions, task assignment, Windows development, the order colliding PRs land in. The default worker. |
| **pro-wsl-quicuts** | WSL2 on that box | Secondary. Keeps the WSL cross-build working, takes assigned work, reviews. |
| **zax-quicuts** | zaxoc, natively (since 2026-10-18) | Second Windows machine, at 125% scaling. Takes assigned work, reviews. |
| **mac-quicuts** | the Mac | On-demand. The macOS port, deep peer review, and a duty to push back — having verified first. |

**Michael does not push.** He stepped out of development on 2026-09-01, and the
sessions do their own branch management. He remains available to both, but
operates primarily through the lead: work flows Michael → pro-win-quicuts →
pro-wsl-quicuts / mac-quicuts, and findings flow back the same way.

**The lead works solo roughly half the time.** mac-quicuts is not a
continuously-running peer; it is brought online for a port or for an in-depth
review. Asking Michael to wake it is a lever the lead should use, not a
favour to be rationed — a change that deserves a second pair of eyes is worth
the round trip.

**Lead means owning decisions, not the keyboard.** mac-quicuts's duty to push
back is load-bearing, not decorative — and is paired with a duty to **verify
before pushing back**, because an eager reviewer's failure mode is confident
noise rather than silence. On 2026-09-01 it was right and pro-wsl-quicuts
was wrong about gating the macOS title poll on `SetOverlayVisible`; a weaker
role would have made that pushback less likely and the feature would have
shipped broken. A lead who is never contradicted is being told what they want
to hear.

## What the sessions settle, and what comes to Michael

**Settle between yourselves:** implementation approach; test strategy and
placement; refactors with no user-visible change; documentation and ADR
*wording*; which session does a piece of work.

**Comes to Michael:**

- anything that changes **shipped behaviour** on either platform;
- anything that changes an accepted ADR's **decision**, as opposed to
  correcting its record;
- anything needing **on-device verification**;
- **scope** changes;
- any disagreement still live after one round;
- **anything you are unsure about.** The standing rule: uncertainty about which
  side of the line something falls on resolves *towards* asking.

The envelope is deliberately wide. Michael's framing: *"I'm happy to give you a
lot of rope. If the app takes a wrong turn because of your decision, we can
always roll back and discuss how to tighten the boundaries."* A wide envelope is
not a substitute for gate discipline, though — the 2026-09-01 failures were all
cases where the classification was **correct** and the gate was walked through
anyway.

## Channels: what is authoritative

Two channels, and they are not interchangeable.

**The repo is authoritative.** PRs, issues, ADRs and docs are what survive a
session ending, what Michael can read, and what a future reader can search.

**Remote control (session-to-session messages) is for speed.** It is
encouraged — it turned a week of relay through Michael into a day — but it is
*ephemeral*. Neither Michael nor a future session can read it.

> **The hard rule: if a design argument changed the outcome, it lands in the PR
> or an ADR before the merge.**

The failure this prevents is invisible until it bites. On 2026-09-01 the
argument about the macOS title poll's cost and the reason `SetOverlayVisible`
was rejected survived *only* because mac-quicuts deliberately wrote it into ADR
0006. Had it stayed in the message thread, the next session to look at a 5 Hz
poll would have "optimised" it and silently deleted a working feature.

## Handing work over

**The issue is the spec; the message is the nudge.** Anything mac-quicuts needs
in order to do the work goes in a GitHub issue — including the gaps in
dependency order, so it can see what blocks what. The message points at the
issue and carries reasoning *about* it.

Issue #19 is the worked example: it enumerated three gaps in dependency order
(agent title reporting → bundle-id host matching → manifest port), and
mac-quicuts cited that ordering as the reason the work took a day rather than a
week. What did *not* survive was the surrounding message context — which is the
point.

## Staying in sync

`git ls-remote --heads` is the claim board. Push your branch **at claim time**,
even empty, before doing the work. Branches are `pw/<topic>` (pro-win-quicuts),
`win/<topic>` (pro-wsl-quicuts), `zax/<topic>` (zax-quicuts) and `mac/<topic>`, **one writer each** — no
session ever pushes to a branch it does not own.

Run `just status` before starting work and before merging: it fetches, prunes,
and shows the resolved `gh` account, open PRs, remote branches and in-flight
CI. It is the same view for every actor, so Michael can run it too.

It prints the `gh` account deliberately. `gh` resolves per-machine, and `just`
recipes do not source your shell rc — so a machine that routes between GitHub
accounts by shell hook will silently pick the wrong one, and still succeed,
because the repo is readable either way. Seeing the login is what makes that
loud.

Never commit to `main`. Branch protection enforces this (PR required, zero
approvals — see `docs/two-agent-review-process.md` for why zero — the Windows
cross-build must pass, and admins are not exempt).

## Asking Michael to test

He is the only one who can run the app on real hardware, and he has asked not to
have to remember what needs testing. So:

1. **Stage first.** `just stage` builds, terminates his running instance,
   deploys, relaunches and reports. On 2026-09-01 he spent an afternoon on an
   08:31 build while four merges landed, including a fix for a bug he could
   have reproduced. **Windows sessions only** (both Prometheus clones deploy to
   the same folder, so either one's stage replaces the other's build) — there
   is no mac equivalent yet, and
   the Mac loop is a different shape (`mac-run` holds the terminal on
   `cargo tauri dev` rather than deploying and detaching), so `mac-stage`
   should be written once its right shape is known rather than guessed at.
2. **Give a numbered plan.** One line per step, each with its **expected
   result**, so "it did something else" is reportable without knowing what the
   code was meant to do.
3. **Say what would falsify it.** A plan that can only pass is not a test.

The 9-step plan on 2026-09-01 is the model, and it earned its keep beyond the
steps: unprompted, Michael found that switching browser tabs while the panel is
on screen flips the page live — a stricter bar than either session had claimed,
and now recorded in ADR 0006.

## Who merges

**The author merges its own PR, once another session has approved it**
(Michael, 2026-10-13). Until then the lead held the only merge button, which
turned every finished PR into a relay: on #45 mac-quicuts had an approved PR
and Michael's go-ahead, and still had to route the merge through two other
sessions. The author is also the branch's only writer, so the "merged while
I was still pushing" race the announce rules guard against mostly disappears.

The lead keeps design decisions, task assignment, and the call on which PR
lands first when two collide.

## When no reviewer is available

Only Michael can let a PR merge without another session's approval, and he
says so in the author's own session (a relayed "Michael said merge" is not
it). **Flag it in the PR body** — mandatory, not a courtesy. An unreviewed
merge that announces itself is recoverable and greppable; one that looks like
every other merge is neither. The flag doubles as the shortlist of PRs worth
reviewing retrospectively.

If the change is significant enough to want review before it lands, ask Michael
to bring mac-quicuts online rather than asking him to waive it.
