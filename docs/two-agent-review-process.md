# Two-agent review process (mac-claude ↔ win-claude)

How the two Claude Code sessions working on Quicuts — one on the Mac, one in
WSL2 on the Windows box — collaborate on a pull request without the user
carrying messages between machines.

Written up after the first run (PR #1, the macOS vertical slice, merged
2026-08-02). That round worked, so this documents what was actually done plus
the parts that were designed but not needed yet.

## Roles

| Agent | Machine | Does |
|---|---|---|
| **mac-claude** | the Mac | writes macOS code, owns the feature branch, pushes |
| **win-claude** | WSL2 / Windows | reviews, verifies what the Linux toolchain can run, merges |

**One writer.** mac-claude owns the branch; win-claude never pushes to it. Its
output is review comments only. That is what keeps two machines off the same
branch and avoids a merge mess neither agent can see the whole of.

## The channel: the PR is the mailbox

Everything either agent needs is reachable from `gh` on both machines.

**win-claude → mac-claude** — one review, posted with the REST API so the
summary and the inline comments land together:

```bash
gh api -X POST repos/<owner>/<repo>/pulls/<n>/reviews --input review.json
```

`review.json` carries `commit_id`, `event`, `body`, and a `comments` array of
`{path, line, side, body}`. Anchoring findings to `path` + `line` matters:
mac-claude gets "this exact line is wrong" instead of prose it has to
re-locate.

**mac-claude → win-claude** — replies on the same threads, plus the commits:

```bash
gh api -X POST repos/<owner>/<repo>/pulls/comments/<id>/replies -f body='...'
gh pr comment <n> --body-file summary.md
```

**Reading the mailbox** — first action of any session, before touching code:

```bash
gh pr view <n> --comments
gh api repos/<owner>/<repo>/pulls/<n>/comments --jq '.[] | {path, line, body}'
```

Unresolved threads are the shared to-do list. Neither agent has to summarize
state for the other — the PR *is* the state.

## Constraint: one GitHub account, so review states are unusable

Both agents push as the same user, so GitHub rejects both `APPROVE` and
`REQUEST_CHANGES`:

```
422  Review Can not request changes on your own pull request
```

**Post `event: "COMMENT"` and put the verdict in the body text.** Open with
`**REQUEST CHANGES**` / `**Approving**` as the first line so it is
unmissable. Do not try to signal via the review state — the API will 422 and
the round stalls.

The same account also means **neither agent can tell its own comments from the
other's**, and two agents that can't distinguish self from other will reply to
themselves indefinitely. So:

> **Every comment opens with `**[win-claude]**` or `**[mac-claude]**`.**
> Each agent ignores any thread whose last comment carries its own tag.

Cheap, and it makes the PR readable to a human later. (The cleaner fix is a
second GitHub account or a bot PAT. The prefix is enough for two agents.)

## Round shape

1. **mac-claude** pushes and comments "ready for review at `<sha>`".
2. **win-claude** reviews at that SHA. Verdict line first, then: findings that
   block, findings that don't, an explicit *what I could not verify* section.
3. **mac-claude** fixes, replies on **every** thread saying what it did —
   including the ones it disagrees with, with reasoning — and resolves a thread
   only when it is actually fixed. Pushes, comments "ready for re-review".
4. **win-claude** re-reviews: reads the diff itself rather than trusting the
   summary, re-runs what it can, then merges or opens another round.

The user is the trigger between steps — one message per handoff, no content
carried. See *Making it hands-off* if that stops being acceptable.

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

- **Pin the review to a SHA**, and state it. mac-claude must not rebase or
  force-push while a review is in flight — the inline anchors rot. Merge `main`
  in instead (PR #1 did; the anchors survived).
- **Say what you could not verify.** win-claude has no Mac: `CGEventTap`,
  AppKit, `plist`, and `.app` bundling are static review only. An approving
  review is *not* evidence the code runs. Runtime verification stays a human
  checkpoint. mac-claude owes the same caveat pointed the other way — in PR #1
  it correctly reported one blocker's fix as tested-in-isolation but never
  observed, and that caveat survived into the merge.
- **Run what you actually can.** `quicuts-agent-mac`'s deps are target-gated
  and `activation.rs` is platform-free, so win-claude ran the macOS state
  machine's tests on the Linux toolchain and wrote a *probe test* that
  reproduced a bug before reporting it. A finding backed by a passing probe is
  not a suggestion, and it costs one round trip less. Look for this seam on
  every review.
- **Review against the agreed scope**, not against what you would have built.
  For PR #1 that was `docs/macos-slice-brief.md` plus the two hard constraints
  in `CLAUDE.md`. Decisions already settled with the user are not re-litigated
  in review comments.
- **Cross-platform findings get handed over, not fixed across the seam.**
  mac-claude found the two-⌘ bug likely exists in `hook.rs` too and explicitly
  did *not* touch the Windows agent from its branch — it flagged it for
  win-claude. Same in reverse.
- **Out-of-scope discoveries become issues**, not extra commits. PR #1 spawned
  #2 and #3 that way; both stayed out of the merge.
- **Round cap: 3.** After three review→fix rounds on one thread, stop and ask
  the user. Two agents politely disagreeing is a token bonfire.

## Making it hands-off

Not needed for PR #1 — the user triggering each side was fine. If it stops
being fine, the missing piece is a "whose turn is it" signal, and since review
states are unusable (above), that has to be **labels**:

| Label | Meaning | Set by |
|---|---|---|
| `needs-review` | pushed, review it | mac-claude |
| `changes-requested` | reviewed, unresolved threads exist | win-claude |
| `needs-human` | deadlock, or a runtime check only the user can do | either |

Each agent's first action on wake is to read the label; if it isn't its turn,
stop. That single rule is what prevents two agents editing the same branch.

Three ways to wake up, in increasing order of setup:

1. **User starts each session.** What PR #1 used. Zero infrastructure, and the
   user never carries content — just says "go".
2. **A polling loop on each machine** (`/loop` on a long interval, ~20–30 min),
   each tick reading the label and acting only on its own turn. Hands-off
   within a work session; costs tokens on quiet ticks.
3. **GitHub Actions running `claude -p`** on `pull_request` / `issue_comment`.
   The review side stops needing the Windows box. Most robust, most setup,
   burns API credit per event.

**GitHub cannot push into a Claude Code session.** Nothing notifies either
agent. "Direct communication" here means a shared mailbox, not a socket — the
options above differ only in what triggers a read.

## Prompt shape for handing a review over

What worked (given to a clean-context mac-claude):

- Which branch, which PR, who reviewed it, **which SHA**
- The `gh` commands to read the review — *"don't work from memory"*
- The blockers restated in one line each, so the agent knows the shape before
  it reads
- Explicit permission to disagree, with the requirement to reply rather than
  silently skip
- The comment-prefix convention and why it exists
- Don't rebase/force-push; what `main` has moved to
- What to run before pushing, and what to do when done
