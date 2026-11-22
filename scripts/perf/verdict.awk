# PASS/FAIL for one measurement against the design's budget (section 6).
# Keep in sync with the limits in measure-windows.ps1.
#   usage: awk -v state=<paused|ac|battery> -v cpu=<avg %> -v mem=<max MB> -f verdict.awk
BEGIN {
  ok = 0
  if (state == "paused")       ok = (cpu < 1)
  else if (state == "ac")      ok = (cpu <= 8)
  else if (state == "battery") ok = (cpu <= 4)
  if (mem > 200) ok = 0
  print (ok ? "PASS" : "FAIL")
}
