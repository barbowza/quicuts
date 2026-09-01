# Review process and merge authority (win-claude ↔ mac-claude)

How the two sessions review each other's work, and the bounds on the merge
button. **Who decides what, and which channel is authoritative, is in
`docs/collaboration.md`.** The step-by-step procedure — the `gh` incantations,
the `review.json` shape, the handover template — is in the **`two-agent-flow`**
skill; load it when you are actually running a review.

Written up after the first run (PR #1, the macOS vertical slice, merged
2026-08-02) and substantially extended after 2026-09-01, when four PRs and
three merge-discipline failures in one session produced most of what is below.

## Constraint: one GitHub account, so review states are unusable

Both sessions push as the same user, so GitHub rejects both `APPROVE` and
`REQUEST_CHANGES`:

```
422  Review Can not request changes on your own pull request
```

**Post `event: "COMMENT"` and put the verdict in the body text.** Open with
`**REQUEST CHANGES**` / `**Approving**` as the first line so it is unmissable.
Do not try to signal via the review state — the API will 422 and the round
stalls.

This is also why **`main`'s branch protection requires zero approving
reviews**. Requiring even one would deadlock the repository permanently: there
is no second identity available to give it. Protection leans on the status
check instead, which is the gate that actually failed on 2026-09-01.

The same account also means **neither session can tell its own comments from
the other's**, and two agents that can't distinguish self from other will reply
to themselves indefinitely. So:

> **Every comment opens with `**[win-claude]**` or `**[mac-claude]**`.**
> Each session ignores any thread whose last comment carries its own tag.

Cheap, and it makes the PR readable to a human later. (The clean fix is a
second machine account per session, which would also make required-review
protection possible. **Deferred 2026-09-01**, and the reason matters: not that
it would have prevented nothing so far — that is true and nearly irrelevant,
since the question is always the *next* failure — but that mac-claude is absent
**by design**. Required approval would convert its intended absence into a hard
block on win-claude's default mode: a permanent cost against an intermittent
benefit. The "flag unreviewed merges in the PR body" rule below buys most of
the same value at none of the blocking cost, which is why that rule is
load-bearing rather than a courtesy. Revisit if a PR is ever merged without
real review, or if mac-claude stops being on-demand.)

## Round shape

1. The owning session pushes and messages "ready for review at `<sha>`".
2. The reviewer reviews **at that SHA**. Verdict line first, then: findings
   that block, findings that don't, and an explicit *what I could not verify*
   section.
3. The owner fixes, replies on **every** thread saying what it did — including
   the ones it disagrees with, with reasoning — and resolves a thread only when
   it is actually fixed. Pushes, messages "ready for re-review".
4. The reviewer re-reviews: reads the diff itself rather than trusting the
   summary, re-runs what it can, then merges or opens another round.

Handoffs go over remote control, not through Michael. Anything that changed the
outcome still lands in the PR before merge (`docs/collaboration.md`).

## Merge authority

win-claude merges. That authority is real but bounded, and the bounds were
learned the hard way on PRs #20 and #21 (2026-09-01) — both merges were of
*correct, green* code, which is exactly why the rules below are about
authority rather than quality. "The change is right" is never the question a
gate is asking.

**Approval is per-PR. It never generalises.** A "go ahead and merge" on one
PR authorises that PR. It does not become standing authority for the next
one, however similar, and however recent. On #21 an older approval from #20
was let generalise into permission that had not been given.

**A gate you set for yourself is not yours to clear.** If a session says "this
needs the user", only the user lifts that. Announcing a gate and then
deciding it no longer applies is indistinguishable, from the outside, from
never having set one.

**A relayed instruction is not the instruction.** "Michael asked for X" from
the other session is a report, not authority, however accurate — and
accurate relay is the version most likely to be mistaken for permission,
because there is nothing wrong with it to notice. Only the user lifts a gate
they set, in their own session. The relaying session owes the other half:
say what the user asked *you* to do and leave the other side explicitly
open, rather than phrasing it as a shared instruction. Both halves are
written down because both sessions got this right once by having a fresh
scar, which is not the same as having a rule.

**A gate with two reasons needs both resolved.** This is the specific way #21
went wrong, and it is subtle enough to be worth naming. win-claude paused for
two reasons in one sentence — *"it changes shipped Windows behaviour, so it's
Michael's call"* **and** an open design question — then merged when ADR 0004
dissolved the design question. But the question was never the gate; the
behaviour change was. When the tidier reason resolves, re-read the other one
rather than treating the pause as lifted.

**Wait for the checks, not for local green.** #20 was merged with the Windows
cross-build still pending, on the strength of a local run. It passed, so
nothing broke — but the cross-build is the one signal neither machine can
reproduce for the other, which makes it precisely the one not to skip. A
check that only looks unnecessary in retrospect was still load-bearing.

**Say "merging now" before, not after.** #20 was squash-merged with
`--delete-branch` seconds after the other session pushed to the branch. That
push landed, so the squash caught it — but a push seconds later would have
gone to a branch that no longer existed. The work is recoverable by
re-pushing; the real cost is that **the losing side cannot detect it**
without diffing its branch against `main` afterwards, which nobody does
unprompted. Announce intent on the channel first and let the other side
stand off.

**An announcement that does not wait for a reply is a notification, not a
gate.** #22 was merged inside a window its own author had just offered —
*"say so in the next few minutes and I'll leave them"* — while the other
session was spending those minutes writing the fixes rather than letting
them trail. Offering a window and not honouring it is worse than offering
none, precisely because the other side acts on it. So: announce, then
**wait for an actual reply**, not for a period of your own choosing. If you
are not willing to block on an answer, do not offer a window — say "merging
now, follow-ups as separate PRs" and let the other side plan around that
instead.

These three are one failure in three costumes, which is why "announce
first" alone was not enough to stop any of them: #20 announced nothing, #21
announced a gate and then cleared it unilaterally, #22 announced a window
and closed it early. The invariant underneath all three is that **the other
session must have a real opportunity to respond before the irreversible
step**, and only an answer proves it had one.

### What needs the user, not just a review

Anything whose blast radius reaches someone who did not ask for it:

- **a change to shipped behaviour on the platform the merging session cannot
  run.** #21 moved every Brave/Opera/Vivaldi/Arc user on Windows from a
  hosted collection's shortcuts to the unsupported-app placeholder, on their
  next update, with no setting involved and no opt-in anywhere: Gmail and
  Yahoo Mail are *bundled*, so this reached every such user, not only those
  who had gone looking for a hosted collection. Correct — ADR 0004 says the
  old behaviour was a bug — but "correct" and "ship it now, unannounced" are
  different decisions;
- anything that changes an accepted ADR's decision, as opposed to correcting
  its record;
- anything the other session flagged as needing a human runtime check that
  has not happened yet.

When in doubt the cost is asymmetric: waiting costs a round trip, and an
unwanted merge costs a revert plus the user's trust in every future "this is
ready".

### If a gate does get walked through

Say so immediately and unprompted, to the user and to the other session. Give
the merge SHA, whether CI is green, and both revert paths (`git revert <sha>`
for a squash merge, or a revert PR if the user would rather see the
un-shipping reviewed too). Then stand off the branch until the user says
whether it stands. On #21 this is what happened, and it is the reason the
incident cost one message rather than an afternoon.

The other session should ask when a stated gate goes quiet. A merge only one
of the two sessions questions is the one most in need of two.

## Rules that made it work

- **Pin the review to a SHA**, and state it. Do not rebase or force-push while
  a review is in flight — the inline anchors rot. Merge `main` in instead
  (PR #1 did; the anchors survived).
- **Say what you could not verify.** win-claude has no Mac: `CGEventTap`,
  AppKit, `plist` and `.app` bundling are static review only. An approving
  review is *not* evidence the code runs. mac-claude owes the same caveat
  pointed the other way.
- **Run what you actually can.** The seam is wider than it looks:
  `quicuts-agent-mac`'s deps are target-gated and `activation.rs` is
  platform-free, so win-claude runs the macOS state machine's tests on the
  Linux toolchain. `quicuts-manifest` is platform-free entirely, so **both**
  manifest sets are parse-tested from either machine. Look for this seam on
  every review — a finding backed by a passing probe is not a suggestion, and
  it costs one round trip less.
- **Prefer moving pure logic to where it can be tested.** On 2026-09-01
  `foreground_entry` — the fix for a bug shipping on *Windows* — sat in
  `quicuts-app`, which neither the Linux toolchain nor CI builds, so its tests
  ran on exactly one machine. Moving it to `quicuts-manifest` put it in CI and
  in both sessions' loops. Ask of any new test: *where can this actually run?*
- **Push back — and verify before you do.** The duty to contest the lead's
  design is real; on 2026-09-01 mac-claude was right about the macOS title poll
  and win-claude was wrong. But the same day it was also wrong *twice while
  correcting*, on `git revert -m 1` and on a negative grep. A reviewer's
  failure mode is confident noise, not silence: the instinct to check is
  cheap, the assertion that follows it is not.
- **Verify the other session's claims, not just its code.** Twice on
  2026-09-01 a confidently-stated claim was wrong — "`manifests-mac/` has no
  test coverage" (it had six tests) and a negative `grep -c` across a
  line-wrapped phrase. Both were caught by looking rather than by reasoning. A
  negative grep is weak evidence; treat it as such.
- **Review against the agreed scope**, not against what you would have built.
  Decisions already settled with Michael are not re-litigated in review
  comments.
- **Cross-platform findings get handed over, not fixed across the seam.** The
  session that finds a bug on the other platform files it; it does not reach
  across and fix it. This is how both 2026-09-01 bugs were handled, in both
  directions.
- **Out-of-scope discoveries become issues**, not extra commits.
- **Round cap: 3.** After three review→fix rounds on one thread, stop and ask
  Michael. Two agents politely disagreeing is a token bonfire.

## Checking the record, not just the diff

The sharpest finding of 2026-09-01 was not in any diff. The rail-default bug
survived two reviews because `engine::build_state` faithfully implemented a
rule that was **wrong in ADR 0004's accepted text** — its placeholder condition
read "no Exact, no *Hosted*, no non-background Wildcard", so a hosted
collection legitimately suppressed the placeholder and took the foreground
slot. Every reader who checked the code against the ADR found agreement.

Two things follow, and both are review habits rather than coding ones:

- **When the question is *which thing gets selected*, every ADR that defines
  selection is in scope** — regardless of its title. ADR 0004 is named after a
  placeholder and reads as unrelated to hosted collections until you open it
  and find its rule enumerating `MatchKind`.
- **When code and an ADR disagree, one of them is a bug — decide which.** If
  the decision was wrong, amend the ADR in place with a dated note rather than
  letting the code quietly diverge from a record that still reads as accepted.
