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
