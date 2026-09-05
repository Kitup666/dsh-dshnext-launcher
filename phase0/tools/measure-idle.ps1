param(
  [Parameter(Mandatory=$true)][string]$ProcName,
  [int]$Seconds = 60,
  [string]$Label = ""
)

# Sample one process for N seconds and report idle cost: CPU%, working set,
# private bytes, and GPU utilisation.
#
# Two measurement traps this avoids:
#  - "% Processor Time" from Get-Counter is per-core, so a 100% reading on a
#    16-thread box means one core saturated. We divide by the logical CPU count
#    to get the same number Task Manager shows.
#  - a WebView2 app is several processes (browser, GPU, renderers). Summing only
#    the named process would undercount it badly, so children whose parent is the
#    target are summed too.
#
# GPU comes from "GPU Engine(*)\Utilization Percentage", which is keyed by pid in
# the instance name -- the only counter that attributes GPU work per process.
#
# NOTE: keep this file pure ASCII. Windows PowerShell reads .ps1 as ANSI without
# a BOM, so non-ASCII literals here would become mojibake and fail to parse.

$cores = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
Write-Output ("=== {0} ({1}) cores={2} sampling {3}s ===" -f $ProcName, $Label, $cores, $Seconds)

$procs = Get-Process -Name $ProcName -ErrorAction SilentlyContinue
if ($null -eq $procs) { Write-Output "PROCESS_NOT_RUNNING"; exit 1 }

# Collect the whole process tree, recursively. One level is not enough: WebView2
# spawns a browser process which then spawns GPU/renderer/utility children, so a
# single-level walk finds 2 of 6 processes and undercounts memory by ~200 MB.
$snapshot = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
$allIds = @($procs | ForEach-Object { $_.Id })
$frontier = @($allIds)
while ($frontier.Count -gt 0) {
  $next = @()
  foreach ($row in $snapshot) {
    if (($frontier -contains [int]$row.ParentProcessId) -and
        (-not ($allIds -contains [int]$row.ProcessId))) {
      $allIds += [int]$row.ProcessId
      $next += [int]$row.ProcessId
    }
  }
  $frontier = $next
}
$allIds = $allIds | Sort-Object -Unique
Write-Output ("pids: {0}" -f ($allIds -join ", "))
foreach ($id in $allIds) {
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($p) { Write-Output ("  {0} {1}" -f $id, $p.ProcessName) }
}

# CPU time deltas are more reliable than sampled counters for a near-idle
# process: read TotalProcessorTime at both ends and divide by wall time.
function Snapshot {
  $tot = [TimeSpan]::Zero
  $ws = 0; $pb = 0; $alive = 0
  foreach ($id in $allIds) {
    $p = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -eq $p) { continue }
    $alive++
    $tot += $p.TotalProcessorTime
    $ws += $p.WorkingSet64
    $pb += $p.PrivateMemorySize64
  }
  return [pscustomobject]@{ Cpu = $tot; Ws = $ws; Pb = $pb; Alive = $alive; At = Get-Date }
}

function GpuPercent {
  $sum = 0.0
  try {
    $c = Get-Counter -Counter "\GPU Engine(*)\Utilization Percentage" -ErrorAction Stop
    foreach ($s in $c.CounterSamples) {
      foreach ($id in $allIds) {
        if ($s.InstanceName -like ("pid_" + $id + "_*")) { $sum += $s.CookedValue }
      }
    }
  } catch { return -1 }
  return $sum
}

$start = Snapshot
$gpuSamples = @()
$peakWs = $start.Ws

for ($t = 0; $t -lt $Seconds; $t += 5) {
  Start-Sleep -Seconds 5
  $g = GpuPercent
  if ($g -ge 0) { $gpuSamples += $g }
  $s = Snapshot
  if ($s.Ws -gt $peakWs) { $peakWs = $s.Ws }
  $elapsed = ($s.At - $start.At).TotalSeconds
  $cpuPct = ($s.Cpu - $start.Cpu).TotalSeconds / $elapsed / $cores * 100
  Write-Output ("  t={0,3}s cpu={1,6:F3}%  ws={2,7:F1} MB  private={3,7:F1} MB  gpu={4,6:F2}%" -f `
    [int]$elapsed, $cpuPct, ($s.Ws / 1MB), ($s.Pb / 1MB), $g)
}

$end = Snapshot
$elapsed = ($end.At - $start.At).TotalSeconds
$cpuSec = ($end.Cpu - $start.Cpu).TotalSeconds
$gpuAvg = if ($gpuSamples.Count -gt 0) { ($gpuSamples | Measure-Object -Average).Average } else { -1 }

Write-Output "--- summary ---"
Write-Output ("processes alive : {0}" -f $end.Alive)
Write-Output ("wall time       : {0:F1}s" -f $elapsed)
Write-Output ("cpu time used   : {0:F3}s" -f $cpuSec)
Write-Output ("cpu average     : {0:F4}% of all cores ({1:F2}% of one core)" -f `
  ($cpuSec / $elapsed / $cores * 100), ($cpuSec / $elapsed * 100))
Write-Output ("working set     : {0:F1} MB (peak {1:F1} MB)" -f ($end.Ws / 1MB), ($peakWs / 1MB))
Write-Output ("private bytes   : {0:F1} MB" -f ($end.Pb / 1MB))
Write-Output ("gpu average     : {0:F2}%" -f $gpuAvg)
