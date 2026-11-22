# usage: powershell -ExecutionPolicy Bypass -File measure-windows.ps1 -Exe <App.exe> -State <paused|ac> -Widgets <per-window|one-window> -WidgetHost <bottom|desktop-child> [-Seconds 60]
# Prints one last line:
#   state=<> widgets=<> avg_cpu=<% of one core> max_mem_mb=<MB> verdict=PASS|FAIL
# The layer windows show on screen while this runs; do not touch the mouse.
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
$proc = Start-Process -FilePath $exePath -ArgumentList "--lab-preset=baseline", "--fps=$fps", "--widgets=$Widgets", "--win-widget-host=$WidgetHost" -PassThru

function Get-Descendants([int]$rootPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
  $found = New-Object System.Collections.Generic.List[int]
  $found.Add($rootPid)
  for ($i = 0; $i -lt $found.Count; $i++) {
    foreach ($p in $all) { if ($p.ParentProcessId -eq $found[$i] -and -not $found.Contains([int]$p.ProcessId)) { $found.Add([int]$p.ProcessId) } }
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
    $pids = Get-Descendants $proc.Id
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
  # Always stop the app and everything it started, even if the measurement threw.
  $final = Get-Descendants $proc.Id
  Get-Process -Id $final -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($cpuSamples.Count -eq 0) {
  [Console]::Error.WriteLine("no usable samples (no process rows came back); the run is invalid")
  Write-Output ("state={0} widgets={1} avg_cpu=NA max_mem_mb=NA verdict=FAIL" -f $State, $Widgets)
  exit 1
}
if ($emptySamples -gt 0) { [Console]::Error.WriteLine("warning: $emptySamples sample(s) had no process rows") }
$avg = ($cpuSamples | Measure-Object -Average).Average
# Limits: keep in sync with verdict.awk.
$cpuOk = switch ($State) { 'paused' { $avg -lt 1 } 'ac' { $avg -le 8 } default { $false } }
$ok = $cpuOk -and ($maxMem -le 200)
$verdict = if ($ok) { 'PASS' } else { 'FAIL' }
$ci = [System.Globalization.CultureInfo]::InvariantCulture
Write-Output ("state={0} widgets={1} avg_cpu={2} max_mem_mb={3} verdict={4}" -f $State, $Widgets, $avg.ToString('F2', $ci), $maxMem.ToString('F1', $ci), $verdict)
if (-not $ok) { exit 1 }
