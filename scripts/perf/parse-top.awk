# Sums %CPU and MEM of the pids in `pids` (comma separated) for each `top -l N` sample.
# The first sample is dropped (top reports CPU since process start there). Output:
#   avg_cpu=<mean of per-sample sums, % of one core> max_mem_mb=<largest per-sample sum in MB>
function mb(s,   n, u) {
  sub(/[+-]$/, "", s)
  u = substr(s, length(s), 1)
  n = s + 0
  if (u == "K") return n / 1024
  if (u == "G") return n * 1024
  if (u == "B") return n / 1048576
  return n   # M, or a bare number
}
function close_sample() {
  if (sample > 1) {
    n++
    cpu_total += cpu
    if (mem > max_mem) max_mem = mem
  }
}
BEGIN {
  split(pids, list, ",")
  for (i in list) want[list[i]] = 1
}
/^Processes:/ { close_sample(); sample++; cpu = 0; mem = 0; next }
($1 in want) { cpu += $2; mem += mb($3) }
END {
  close_sample()
  printf "avg_cpu=%.2f max_mem_mb=%.1f\n", (n ? cpu_total / n : 0), max_mem
}
