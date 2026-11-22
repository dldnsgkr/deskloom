#!/bin/bash
# usage: measure-mac.sh <App.app> <paused|ac|battery> <per-window|one-window> [seconds=60]
# Opens the app with the baseline preset and without the lab window (--no-lab), samples CPU and
# memory of the app and its WebKit helper processes once a second, and prints one last line:
#   state=<> widgets=<> avg_cpu=<% of one core> max_mem_mb=<MB> verdict=PASS|FAIL
# The layer windows show on screen while this runs; do not touch the mouse.
# Quit the app first: the script refuses to start while the same app (same path) is running.
set -u
if [ $# -lt 3 ]; then echo "usage: $0 <App.app> <paused|ac|battery> <per-window|one-window> [seconds=60]" >&2; exit 2; fi
here="$(cd "$(dirname "$0")" && pwd)"
app="$(cd "$1" 2>/dev/null && pwd -P)" || { echo "no such app: $1" >&2; exit 2; }
state="$2"; widgets="$3"; secs="${4:-60}"
case "$state" in paused) fps=0 ;; ac) fps=60 ;; battery) fps=30 ;; *) echo "unknown state: $state" >&2; exit 2 ;; esac
case "$widgets" in per-window|one-window) ;; *) echo "unknown widgets: $widgets" >&2; exit 2 ;; esac
exe_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist" 2>/dev/null)"
exe="$app/Contents/MacOS/$exe_name"
if [ -z "$exe_name" ] || [ ! -x "$exe" ]; then echo "not an app bundle (no executable named in Info.plist): $app" >&2; exit 2; fi

# Pids whose command line is exactly the app's executable path (plus arguments). A plain string
# comparison, not a regex, so paths with spaces or brackets ("macos-app (1)") work.
find_app_pids() {
  ps -axww -o pid= -o args= | EXE="$exe" awk '{
    pid = $1; sub(/^[ \t]*[0-9]+[ \t]+/, "")
    if ($0 == ENVIRON["EXE"] || index($0, ENVIRON["EXE"] " ") == 1) print pid
  }' | sort -n
}

already="$(find_app_pids)"
if [ -n "$already" ]; then
  echo "the app is already running (pid $(echo $already)); quit it first so it does not count in this run: $exe" >&2
  exit 2
fi

tmp="${TMPDIR:-/tmp}/measure-top.$$"
stop_app() {
  local left
  left="$(find_app_pids)"
  [ -z "$left" ] && return 0
  # shellcheck disable=SC2086
  kill $left 2>/dev/null
  for _ in 1 2 3 4 5; do sleep 1; [ -z "$(find_app_pids)" ] && return 0; done
  left="$(find_app_pids)"
  # shellcheck disable=SC2086
  [ -n "$left" ] && kill -9 $left 2>/dev/null
  return 0
}
# Whatever ends the script (an error, Ctrl-C, an unset variable), the app it opened does not stay running.
trap 'stop_app; rm -f "$tmp"' EXIT
trap 'exit 130' INT TERM

if [ "$state" = battery ] && pmset -g batt | grep -q 'AC Power'; then
  echo "warning: state=battery but the Mac is on AC power; the numbers are for reference only" >&2
fi

rp=""; helpers=""
before="$(pgrep -f com.apple.WebKit | sort)"
open -n "$app" --args --lab-preset=baseline --no-lab --fps="$fps" --widgets="$widgets"
sleep 15

main_pids="$(find_app_pids)"
if [ -z "$main_pids" ]; then echo "the app did not start: $app" >&2; echo "state=$state widgets=$widgets avg_cpu=NA max_mem_mb=NA verdict=FAIL"; exit 1; fi

# Helpers of ours: WebKit processes whose responsible process (what Activity Monitor groups by) is the app.
# Fallback if that lookup is unavailable: WebKit processes that appeared after the app was opened (unverified).
now_webkit="$(pgrep -f com.apple.WebKit | sort)"
if [ -n "$now_webkit" ]; then
  # Prefer the compiled helper shipped next to this script; else run the source with swift if it exists.
  # shellcheck disable=SC2086
  if [ -x "$here/responsible-pid" ]; then rp="$("$here/responsible-pid" $now_webkit 2>/dev/null)"
  elif command -v swift >/dev/null 2>&1 && [ -f "$here/responsible-pid.swift" ]; then rp="$(swift "$here/responsible-pid.swift" $now_webkit 2>/dev/null)"
  fi
  for a in $main_pids; do helpers="$helpers $(echo "$rp" | awk -v a="$a" '$2 == a { print $1 }')"; done
fi
helpers="$(echo $helpers)"
if [ -z "$helpers" ]; then
  echo "warning: responsible-pid lookup gave no helpers (no responsible-pid binary or swift, or the API unavailable); falling back to WebKit processes started after the app. Attribution is UNVERIFIED." >&2
  helpers="$(comm -13 <(echo "$before") <(echo "$now_webkit") | tr '\n' ' ')"
fi
pids="$(printf '%s\n' $main_pids $helpers | grep -E '^[0-9]+$' | sort -un)"
if [ -n "$rp" ]; then
  echo "WebKit processes: $(echo $now_webkit | wc -w | tr -d ' ') running, $(echo $helpers | wc -w | tr -d ' ') kept (responsible pid = the app), the rest dropped" >&2
fi
echo "measured processes:" >&2
# shellcheck disable=SC2086
ps -o pid,ppid,rss,command -p "$(echo $pids | tr ' ' ',')" >&2

pid_args=""; for p in $pids; do pid_args="$pid_args -pid $p"; done
top_ok=0
# shellcheck disable=SC2086
top -l $((secs + 1)) -s 1 -stats pid,cpu,mem $pid_args > "$tmp" || top_ok=1
stop_app

result="$(awk -v pids="$(echo $pids | tr ' ' ',')" -v counts=1 -f "$here/parse-top.awk" "$tmp")"
field() { echo "$result" | tr ' ' '\n' | sed -n "s/^$1=//p"; }
cpu="$(field avg_cpu)"; mem="$(field max_mem_mb)"; used="$(field samples)"; empty="$(field empty)"
taken=$(( $(grep -c '^Processes:' "$tmp") - 1 )); [ "$taken" -lt 0 ] && taken=0
echo "samples used: ${used:-0} of $taken (the first one, CPU since process start, is always dropped)" >&2
if [ "${empty:-0}" != 0 ]; then
  echo "warning: $empty sample(s) had none of the measured processes and were left out (did the app quit or crash?)" >&2
fi
if [ "$top_ok" != 0 ] || [ "$cpu" = NA ] || [ -z "$cpu" ]; then
  echo "no usable samples (top failed or none of the processes showed up); the run is invalid" >&2
  echo "state=$state widgets=$widgets avg_cpu=NA max_mem_mb=NA verdict=FAIL"
  exit 1
fi
verdict="$(awk -v state="$state" -v cpu="$cpu" -v mem="$mem" -f "$here/verdict.awk")"
echo "state=$state widgets=$widgets avg_cpu=$cpu max_mem_mb=$mem verdict=$verdict"
[ "$verdict" = PASS ]
