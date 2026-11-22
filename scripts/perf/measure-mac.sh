#!/bin/bash
# usage: measure-mac.sh <App.app> <paused|ac|battery> <per-window|one-window> [seconds=60]
# Opens the app with the baseline preset, samples CPU and memory of the app and its WebKit helper
# processes once a second, and prints one last line:
#   state=<> widgets=<> avg_cpu=<% of one core> max_mem_mb=<MB> verdict=PASS|FAIL
# The layer windows show on screen while this runs; do not touch the mouse.
set -u
if [ $# -lt 3 ]; then echo "usage: $0 <App.app> <paused|ac|battery> <per-window|one-window> [seconds=60]" >&2; exit 2; fi
here="$(cd "$(dirname "$0")" && pwd)"
app="$(cd "$1" && pwd)"
state="$2"; widgets="$3"; secs="${4:-60}"
case "$state" in paused) fps=0 ;; ac) fps=60 ;; battery) fps=30 ;; *) echo "unknown state: $state" >&2; exit 2 ;; esac
case "$widgets" in per-window|one-window) ;; *) echo "unknown widgets: $widgets" >&2; exit 2 ;; esac

if [ "$state" = battery ] && pmset -g batt | grep -q 'AC Power'; then
  echo "warning: state=battery but the Mac is on AC power; the numbers are for reference only" >&2
fi

before="$(pgrep -f com.apple.WebKit | sort)"
open -n "$app" --args --lab-preset=baseline --fps="$fps" --widgets="$widgets"
sleep 15

app_pids="$(pgrep -f "$app/Contents/MacOS/" | sort)"
if [ -z "$app_pids" ]; then echo "the app did not start: $app" >&2; echo "state=$state widgets=$widgets avg_cpu=NA max_mem_mb=NA verdict=FAIL"; exit 1; fi

# Helpers of ours: WebKit processes whose responsible process (what Activity Monitor groups by) is the app.
# Fallback if that lookup is unavailable: WebKit processes that appeared after the app was opened (unverified).
now_webkit="$(pgrep -f com.apple.WebKit | sort)"
helpers=""
if [ -n "$now_webkit" ]; then
  # shellcheck disable=SC2086
  rp="$(swift "$here/responsible-pid.swift" $now_webkit 2>/dev/null)"
  for a in $app_pids; do helpers="$helpers $(echo "$rp" | awk -v a="$a" '$2 == a { print $1 }')"; done
fi
helpers="$(echo $helpers)"
if [ -z "$helpers" ]; then
  echo "warning: responsible-pid lookup gave no helpers (swift or the API unavailable); falling back to WebKit processes started after the app. Attribution is UNVERIFIED." >&2
  helpers="$(comm -13 <(echo "$before") <(echo "$now_webkit") | tr '\n' ' ')"
fi
pids="$(printf '%s\n' $app_pids $helpers | grep -E '^[0-9]+$' | sort -un)"
echo "measured processes:" >&2
# shellcheck disable=SC2086
ps -o pid,ppid,rss,command -p "$(echo $pids | tr ' ' ',')" >&2

pid_args=""; for p in $pids; do pid_args="$pid_args -pid $p"; done
# shellcheck disable=SC2086
top_ok=0
top -l $((secs + 1)) -s 1 -stats pid,cpu,mem $pid_args > "${TMPDIR:-/tmp}/measure-top.$$" || top_ok=1
pkill -f "$app/Contents/MacOS/"

result="$(awk -v pids="$(echo $pids | tr ' ' ',')" -f "$here/parse-top.awk" "${TMPDIR:-/tmp}/measure-top.$$")"
rm -f "${TMPDIR:-/tmp}/measure-top.$$"
cpu="${result#avg_cpu=}"; cpu="${cpu%% *}"
mem="${result##*max_mem_mb=}"
if [ "$top_ok" != 0 ] || [ "$cpu" = NA ] || [ -z "$cpu" ]; then
  echo "no usable samples (top failed or none of the processes showed up); the run is invalid" >&2
  echo "state=$state widgets=$widgets avg_cpu=NA max_mem_mb=NA verdict=FAIL"
  exit 1
fi
verdict="$(awk -v state="$state" -v cpu="$cpu" -v mem="$mem" -f "$here/verdict.awk")"
echo "state=$state widgets=$widgets avg_cpu=$cpu max_mem_mb=$mem verdict=$verdict"
[ "$verdict" = PASS ]
