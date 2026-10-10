---
name: two-agent-flow
description: Procedure for win-claude ↔ mac-claude collaboration on Quicuts — claiming work, handing it over, posting a PR review with inline comments, announcing intent, and merging. Use when reviewing or merging the other session's PR, handing work across the Windows/macOS seam, or starting work that the other session might collide with.
---

# Two-agent flow

Mechanics only. The **model** (who decides what, channel authority, the
autonomy envelope) is `docs/collaboration.md`; the **rules and why they exist**
are `docs/two-agent-review-process.md`. The invariants are in `CLAUDE.md` and
are already in context — this skill assumes them.

Both sessions use this file. Sections marked **[lead]** are win-claude's;
**[owner]** is whichever session owns the branch under review.

## 0. One-time, per checkout

```bash
git config user.name  win-claude        # or mac-claude
git config user.email win-claude@quicuts.invalid
```

Not committed — `.git/config` is local, so each checkout sets its own.

## 1. Before starting anything

```bash
just status
```

Fetches and prunes, then shows open PRs, remote branches (the claim board) and
in-flight CI. Read it before starting work and again before merging. If the
other session holds a branch touching what you are about to touch, message it
first.

Then claim:

```bash
git checkout -b win/<topic>          # or mac/<topic>
git push -u origin win/<topic>       # push immediately, even empty
```

Pushing at claim time is what makes `git ls-remote --heads` a live board. One
writer per branch — never push to a branch you do not own.

## 2. Handing work over  **[lead]**

The issue is the spec; the message is the nudge.

```bash
gh issue create --title "[mac-claude] <what>" --label enhancement --body "$(cat <<'EOF'
## Problem
<what is broken or missing, with file:line evidence>

## Work
<the gaps in DEPENDENCY ORDER — what blocks what>

## Notes
<constraints, invariants that must survive, ADRs to read>
EOF
)"
```

Then message the other session: point at the issue number, add the reasoning
*about* it, and say explicitly what is still open for them to decide. Do not
restate the spec in the message — anything they need to do the work belongs in
the issue, which outlives the session.

Say what Michael asked **you** to do, and leave their half explicitly open. A
relayed instruction is not an instruction.

## 3. Reviewing a PR

Read the whole diff, not the summary. Verify claims about the repo by looking,
not by reasoning.

Get the SHA and the anchor lines you want to comment on:

```bash
gh pr view <N> --json headRefOid,title,files -q '.headRefOid, .title, (.files[]|"  "+.path)'
gh pr diff <N> > /tmp/pr<N>.diff
git show "<branch>:<path>" | grep -n "<pattern>"   # anchor line numbers
```

Run what you can before writing a word of the review:

```bash
just test          # both sessions: platform-free crates + the other's manifests
just mac-test      # mac only: adds quicuts-agent-mac and quicuts-app
just ui            # both: the Svelte build
```

Post the review — summary and inline comments must land together, so use the
REST API rather than `gh pr review`:

```bash
python3 - <<'PY'
import json
body = """**[win-claude] REQUEST CHANGES** (or **Approving**) — verdict first.

## What I verified
<commands run and what they showed>

## What I could not verify
<the other platform, runtime behaviour, anything needing hardware>
"""
comments = [
  {"path":"crates/…/foo.rs","line":42,"side":"RIGHT","body":"**[win-claude]** …"},
]
json.dump({"commit_id":"<headRefOid>","event":"COMMENT","body":body,"comments":comments},
          open("review.json","w"))
PY
gh api -X POST repos/barbowza/quicuts/pulls/<N>/reviews --input review.json --jq '.state'
```

`event` must be `COMMENT` — `APPROVE` and `REQUEST_CHANGES` 422 on your own
PR. Every comment opens with your `**[win-claude]**` / `**[mac-claude]**` tag.

Replying on an existing thread:

```bash
gh api -X POST repos/barbowza/quicuts/pulls/comments/<comment-id>/replies -f body='**[mac-claude]** …'
```

## 4. Responding to a review  **[owner]**

Reply on **every** thread — including ones you disagree with, with reasoning.
Resolve a thread only when it is actually fixed. Do not rebase or force-push
while a review is in flight; the inline anchors rot. Merge `main` in instead.

## 5. Announcing and merging  **[lead]**

```bash
just status                                    # again — state moves
gh pr checks <N>                               # must be pass, not pending
gh pr view <N> --json mergeStateStatus -q .mergeStateStatus   # want CLEAN
```

Then announce, and **wait for an actual reply**:

> "Merging #N now — check green (run <id>). Stand off the branch."

If you are not willing to block on a reply, do not offer a window. Say
"merging now, follow-ups as separate PRs" instead. An announcement that does
not wait is a notification, not a gate.

```bash
gh pr merge <N> --squash --delete-branch
git checkout main && git pull --ff-only
```

Branch protection will refuse a merge with a pending or failing check. That is
deliberate — it is the gate that failed on 2026-09-01, now enforced by the
server rather than by memory.

**Merging unreviewed** (the other session is offline — expected roughly half
the time): allowed, but put a line in the PR body saying so before merging, so
the set of unreviewed merges is greppable and can be reviewed retrospectively.
If the change is significant, ask Michael to bring mac-claude online instead.

## 6. Asking Michael to test

```bash
just stage      # build, terminate his instance, deploy, relaunch, report
```

Then give him a numbered plan: one line per step, **expected result on each**,
and a note on what would falsify the claim. He reports findings; he does not
develop.

## Handover prompt template

For briefing a clean-context session on the other machine:

- Branch, PR number, **which SHA** the review is pinned to
- The `gh` commands to read the review — *work from the PR, not from memory*
- Blockers restated in one line each, so it knows the shape before reading
- Explicit permission to disagree, with the requirement to reply rather than
  silently skip
- The comment-tag convention and why it exists
- Don't rebase or force-push; what `main` has moved to
- What to run before pushing, and what to do when done
