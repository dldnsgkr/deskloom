# usage: powershell -ExecutionPolicy Bypass -File measure-windows.ps1 -Exe <App.exe> -State <paused|ac> -Widgets <per-window|one-window> -WidgetHost <bottom|desktop-child> [-Seconds 60]
# Opens the app with the baseline preset and without the lab window (--no-lab), samples CPU and
# memory of the app and the processes it started once a second, and prints one last line:
#   state=<> widgets=<> avg_cpu=<% of one core> max_mem_mb=<MB> verdict=PASS|FAIL
# The layer windows show on screen while this runs; do not touch the mouse.
# Quit the app first: the script refuses to start while the same exe (same path) is running.
# The exe gets Unblock-File first: a copy downloaded with a browser carries the Mark of the Web,
# and SmartScreen would stop it before it starts.
param(
  [Parameter(Mandatory)][string]$Exe,
  [Parameter(Mandatory)][ValidateSet('paused','ac')][string]$State,
  [Parameter(Mandatory)][ValidateSet('per-window','one-window')][string]$Widgets,
  [Parameter(Mandatory)][ValidateSet('bottom','desktop-child')][string]$WidgetHost,
  [int]$Seconds = 60
)
$ErrorActionPreference = 'Stop'
$fps = if ($State -eq 'paused') { 0 } else { 60 }
$exePath = (Resolve-Path $Exe).Path

$running = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and ($_.ExecutablePath -ieq $exePath) })
if ($running.Count -gt 0) {
  [Console]::Error.WriteLine("the app is already running (pid $(($running | ForEach-Object { $_.ProcessId }) -join ' ')); quit it first so it does not count in this run: $exePath")
  exit 2
}
try { Unblock-File -Path $exePath } catch { [Console]::Error.WriteLine("warning: Unblock-File failed ($_); if SmartScreen asks, allow the app and run again") }

$proc = Start-Process -FilePath $exePath -ArgumentList "--lab-preset=baseline", "--no-lab", "--fps=$fps", "--widgets=$Widgets", "--win-widget-host=$WidgetHost" -PassThru
$rootStart = $null
try { $rootStart = $proc.StartTime } catch { }

# The app and the processes it started. ParentProcessId is not cleared when a parent exits and pids
# are reused, so a child only counts if it was created after its parent; and the root only while it
# is still the process started above.
function Get-Descendants([int]$rootPid, $rootStarted) {
  $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, CreationDate)
  $created = @{}
  foreach ($p in $all) { $created[[int]$p.ProcessId] = $p.CreationDate }
  $found = New-Object System.Collections.Generic.List[int]
  if (-not $created.ContainsKey($rootPid)) { return $found }
  if ($rootStarted -and $created[$rootPid] -and [math]::Abs(($created[$rootPid] - $rootStarted).TotalSeconds) -gt 2) { return $found }
  $found.Add($rootPid)
  for ($i = 0; $i -lt $found.Count; $i++) {
    $parent = $found[$i]
    $parentCreated = $created[$parent]
    foreach ($p in $all) {
      $id = [int]$p.ProcessId
      if ($p.ParentProcessId -ne $parent -or $found.Contains($id)) { continue }
      if (-not $p.CreationDate -or -not $parentCreated -or $p.CreationDate -lt $parentCreated) { continue }
      $found.Add($id)
    }
  }
  return $found
}

$pids = @($proc.Id)
$cpuSamples = @(); $maxMem = 0.0; $emptySamples = 0
try {
  Start-Sleep -Seconds 15
  # The first PerfFormattedData read can show 0 CPU (no previous reading), so one extra sample is taken and dropped.
  for ($s = 0; $s -le $Seconds; $s++) {
    # Walk the process tree again each time: WebView2 processes can respawn.
    $pids = @(Get-Descendants $proc.Id $rootStart)
    $rows = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $pids -contains [int]$_.IDProcess })
    if ($s -gt 0) {
      if ($rows.Count -eq 0) { $emptySamples++ }
      else {
        $cpuSamples += [double](($rows | Measure-Object -Property PercentProcessorTime -Sum).Sum)
        $mem = ($rows | Measure-Object -Property WorkingSetPrivate -Sum).Sum / 1MB
        if ($mem -gt $maxMem) { $maxMem = $mem }
      }
    }
    if ($s -eq 1) { Write-Host "measured processes: $($pids -join ' ')" }
    Start-Sleep -Seconds 1
  }
} finally {
  # Always stop the app, even if listing its children fails, then everything it started.
  # The children are listed first: once the app is gone its creation time can no longer be checked.
  $children = @()
  try { $children = @(Get-Descendants $proc.Id $rootStart | Where-Object { $_ -ne $proc.Id }) }
  catch { [Console]::Error.WriteLine("warning: could not list the app's child processes: $_") }
  try { if (-not $proc.HasExited) { $proc.Kill() } }
  catch { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
  foreach ($id in $children) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
}

if ($cpuSamples.Count -eq 0) {
  [Console]::Error.WriteLine("no usable samples (no process rows came back); the run is invalid")
  Write-Output ("state={0} widgets={1} avg_cpu=NA max_mem_mb=NA verdict=FAIL" -f $State, $Widgets)
  exit 1
}
Write-Host "samples used: $($cpuSamples.Count) of $Seconds"
if ($emptySamples -gt 0) { [Console]::Error.WriteLine("warning: $emptySamples sample(s) had no process rows") }
$avg = ($cpuSamples | Measure-Object -Average).Average
# Limits: keep in sync with verdict.awk.
$cpuOk = switch ($State) { 'paused' { $avg -lt 1 } 'ac' { $avg -le 8 } default { $false } }
$ok = $cpuOk -and ($maxMem -le 200)
$verdict = if ($ok) { 'PASS' } else { 'FAIL' }
$ci = [System.Globalization.CultureInfo]::InvariantCulture
Write-Output ("state={0} widgets={1} avg_cpu={2} max_mem_mb={3} verdict={4}" -f $State, $Widgets, $avg.ToString('F2', $ci), $maxMem.ToString('F1', $ci), $verdict)
if (-not $ok) { exit 1 }
