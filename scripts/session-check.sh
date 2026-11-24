#!/usr/bin/env bash
# Session-start check, run by the SessionStart hook in .claude/settings.json
# (and by hand: `just check`). Read-only: it fetches, but never pulls, builds
# or installs. It answers three questions and prints them for Claude:
#   1. Is this clone behind origin/main?
#   2. Is the installed build behind origin/main's app code?
#   3. Is Quicuts running?
# "Installed" is the commit a deploy wrote to its stamp file (`just _stamp`,
# called by deploy/stage on Windows and mac-build/mac-install on the Mac).
# Docs-only commits on main do not make the installed build stale.
# Always exits 0: a failed check must never block a session.
set -u   # macOS /bin/bash is 3.2: expand arrays that may be empty as ${a[@]+"${a[@]}"}
cd "$(dirname "$0")/.." || exit 0
export GIT_TERMINAL_PROMPT=0
export GIT_SSH_COMMAND="${GIT_SSH_COMMAND:-ssh -o ConnectTimeout=8}"

# What the app is built from. A change elsewhere (docs, skills) needs no reinstall.
APP_PATHS=(crates ui manifests manifests-mac Cargo.toml Cargo.lock)

case "$(uname -s)" in
  Darwin)
    stamp="$HOME/Library/Application Support/QuicutsDev/installed-commit"
    update_cmd="just mac-build && just mac-install && open /Applications/Quicuts.app"
    running() { pgrep -f 'Quicuts.app/Contents/MacOS/quicuts$' >/dev/null; } ;;
  MINGW*|MSYS*|CYGWIN*)
    stamp="$(cygpath -u "${LOCALAPPDATA:-}")/QuicutsDev/installed-commit"
    update_cmd="just stage"
    running() { MSYS_NO_PATHCONV=1 tasklist.exe /FI "IMAGENAME eq quicuts.exe" /NH 2>/dev/null | grep -qi 'quicuts.exe'; } ;;
  *) # WSL: deploys to the Windows host's QuicutsDev
    stamp="/mnt/c/Users/${WINUSER:-you}/AppData/Local/QuicutsDev/installed-commit"
    update_cmd="just stage"
    running() { tasklist.exe /FI "IMAGENAME eq quicuts.exe" /NH 2>/dev/null | grep -qi 'quicuts.exe'; } ;;
esac

short() { git rev-parse --short "$1" 2>/dev/null || echo "$1"; }
problems=()
notes=()

git -c http.lowSpeedLimit=1 -c http.lowSpeedTime=10 fetch -q --prune origin 2>/dev/null ||
  notes+=("fetch failed (offline?): compared against the last-fetched origin/main")

target=$(git rev-parse -q --verify origin/main) || { echo "quicuts session check: no origin/main ref; skipped."; exit 0; }
branch=$(git rev-parse --abbrev-ref HEAD)
dirty=$(git status --porcelain | wc -l | tr -d ' ')

# 1. The clone.
head_behind=$(git rev-list --count HEAD..origin/main)
if [ "$head_behind" -gt 0 ]; then
  problems+=("code: branch '$branch' is $head_behind commit(s) behind origin/main ($(short "$target"))")
fi
if [ "$branch" != main ] && git rev-parse -q --verify main >/dev/null; then
  main_behind=$(git rev-list --count main..origin/main)
  [ "$main_behind" -gt 0 ] && notes+=("local main is $main_behind commit(s) behind origin/main")
fi
[ "$dirty" -gt 0 ] && notes+=("working tree has $dirty uncommitted change(s): do not pull over them")

# 2. The installed build.
installed=""
[ -f "$stamp" ] && read -r installed installed_flag < "$stamp"
if [ -z "$installed" ]; then
  problems+=("install: no stamp at $stamp, so the installed build's commit is unknown")
elif ! git cat-file -e "$installed^{commit}" 2>/dev/null; then
  problems+=("install: stamped commit $installed is not in this clone (built from an unpushed branch?)")
else
  base=$(git merge-base "$installed" origin/main)
  if git diff --quiet "$base" origin/main -- "${APP_PATHS[@]}"; then
    notes+=("installed build $(short "$installed") has origin/main's app code")
  else
    missing=$(git rev-list --count "$base"..origin/main -- "${APP_PATHS[@]}")
    problems+=("install: installed build $(short "$installed") lacks $missing app-code commit(s) from origin/main")
  fi
  [ "${installed_flag:-}" = dirty ] && notes+=("installed build was made from a tree with uncommitted changes")
fi

# 3. Running?
if running; then notes+=("Quicuts is running"); else problems+=("run: Quicuts is not running"); fi

if [ ${#problems[@]} -eq 0 ]; then
  echo "quicuts session check: up to date. On '$branch' at $(short HEAD); origin/main is $(short "$target")."
  for n in ${notes[@]+"${notes[@]}"}; do echo "  - $n"; done
  exit 0
fi

echo "quicuts session check: NOT up to date. On '$branch' at $(short HEAD); origin/main is $(short "$target")."
for p in "${problems[@]}"; do echo "  ! $p"; done
for n in ${notes[@]+"${notes[@]}"}; do echo "  - $n"; done
cat <<EOF

ACTION FOR CLAUDE (CLAUDE.md § Session start): before other work, tell Michael
the lines above in plain words and ask him (AskUserQuestion) whether to pull
and update now. Do not pull, build or install without his yes.
If yes: pull (CLAUDE.md § Session start says how), then run: $update_cmd
then run scripts/session-check.sh again and report the result.
EOF
exit 0
