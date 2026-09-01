# Collaboration model (Michael ↔ win-claude ↔ mac-claude)

How three actors — one human and two Claude Code sessions on two machines —
build Quicuts without treading on each other.

This document is the **model**: who decides what, which channel is
authoritative, and where the line to Michael sits. The *review procedure* (the
`gh` incantations, round shape, templates) lives in the `two-agent-flow` skill.
The *rules and the incidents that produced them* live in
`docs/two-agent-review-process.md`. The one-page summary that both sessions
always have in context is in `CLAUDE.md`.

## The three actors

| | Machine | Owns |
|---|---|---|
| **Michael** | the Windows box | Scope, priorities, on-device testing, bringing mac-claude online. Product delivery management — **not** development. |
| **win-claude** | WSL2 on that box | Lead. Design decisions, task assignment, Windows development, merges. The default worker. |
| **mac-claude** | the Mac | On-demand. The macOS port, deep peer review, and a duty to push back — having verified first. |

**Michael does not push.** He stepped out of development on 2026-09-01, and the
sessions do their own branch management. He remains available to both, but
operates primarily through win-claude: work flows Michael → win-claude →
mac-claude, and findings flow back the same way.

**win-claude works solo roughly half the time.** mac-claude is not a
continuously-running peer; it is brought online for a port or for an in-depth
review. Asking Michael to wake it is a lever win-claude should use, not a
favour to be rationed — a change that deserves a second pair of eyes is worth
the round trip.

**Lead means owning decisions, not the keyboard.** mac-claude's duty to push
back is load-bearing, not decorative — and is paired with a duty to **verify
before pushing back**, because an eager reviewer's failure mode is confident
noise rather than silence. On 2026-09-01 it was right and win-claude
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
was rejected survived *only* because mac-claude deliberately wrote it into ADR
0006. Had it stayed in the message thread, the next session to look at a 5 Hz
poll would have "optimised" it and silently deleted a working feature.

## Handing work over

**The issue is the spec; the message is the nudge.** Anything mac-claude needs
in order to do the work goes in a GitHub issue — including the gaps in
dependency order, so it can see what blocks what. The message points at the
issue and carries reasoning *about* it.

Issue #19 is the worked example: it enumerated three gaps in dependency order
(agent title reporting → bundle-id host matching → manifest port), and
mac-claude cited that ordering as the reason the work took a day rather than a
week. What did *not* survive was the surrounding message context — which is the
point.

## Staying in sync

`git ls-remote --heads` is the claim board. Push your branch **at claim time**,
even empty, before doing the work. Branches are `win/<topic>` and `mac/<topic>`,
**one writer each** — the other session never pushes to a branch it does not
own.

Run `just status` before starting work and before merging: it fetches, prunes,
and shows the resolved `gh` account, open PRs, remote branches and in-flight
CI. It is the same view for all three actors, so Michael can run it too.

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
   have reproduced. **win-claude only** — there is no mac equivalent yet, and
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

## When the other session is offline

win-claude may merge unreviewed, and often will. **Flag it in the PR body** —
this is mandatory, not a courtesy. An unreviewed merge that announces itself is
recoverable and greppable; one that looks like every other merge is neither.
The flag doubles as the shortlist of PRs worth asking mac-claude to review
retrospectively.

If the change is significant enough to want review before it lands, ask Michael
to bring mac-claude online rather than merging and hoping.
