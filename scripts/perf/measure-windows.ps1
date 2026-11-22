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
Start-Sleep -Seconds 15

function Get-Descendants([int]$rootPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
  $found = New-Object System.Collections.Generic.List[int]
  $found.Add($rootPid)
  for ($i = 0; $i -lt $found.Count; $i++) {
    foreach ($p in $all) { if ($p.ParentProcessId -eq $found[$i] -and -not $found.Contains([int]$p.ProcessId)) { $found.Add([int]$p.ProcessId) } }
  }
  return $found
}

$pids = Get-Descendants $proc.Id
Write-Host "pids: $($pids -join ' ')"
$cpuSamples = @(); $maxMem = 0.0
for ($s = 0; $s -lt $Seconds; $s++) {
  $rows = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $pids -contains [int]$_.IDProcess }
  $cpu = ($rows | Measure-Object -Property PercentProcessorTime -Sum).Sum
  $mem = ($rows | Measure-Object -Property WorkingSetPrivate -Sum).Sum / 1MB
  $cpuSamples += [double]$cpu
  if ($mem -gt $maxMem) { $maxMem = $mem }
  Start-Sleep -Seconds 1
}
Get-Process -Id $pids -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

$avg = ($cpuSamples | Measure-Object -Average).Average
# Limits: keep in sync with verdict.awk.
$cpuOk = switch ($State) { 'paused' { $avg -lt 1 } 'ac' { $avg -le 8 } default { $false } }
$ok = $cpuOk -and ($maxMem -le 200)
$verdict = if ($ok) { 'PASS' } else { 'FAIL' }
$ci = [System.Globalization.CultureInfo]::InvariantCulture
Write-Output ("state={0} widgets={1} avg_cpu={2} max_mem_mb={3} verdict={4}" -f $State, $Widgets, $avg.ToString('F2', $ci), $maxMem.ToString('F1', $ci), $verdict)
