param(
  [Parameter(Mandatory=$true)][string]$Exe,
  [string]$Args = "",
  [int]$Rounds = 5,
  [string]$ProcName = ""
)

# Measure cold start as "process launch -> window visible on screen", which is
# what a user perceives. Timing to process exit or to first log line would both
# be wrong: the first misses rendering, the second misses window presentation.
#
# The clock stops when the target's top-level window both exists and reports a
# non-empty client rect, i.e. it has actually been mapped.
#
# NOTE: keep this file pure ASCII (see other scripts in this folder).

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class CS {
  public delegate bool Proc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Proc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static bool HasVisibleWindow(uint pid) {
    bool found = false;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true;
      if (!IsWindowVisible(h)) return true;
      var t = new StringBuilder(300); GetWindowText(h, t, 300);
      if (t.ToString().Length == 0) return true;
      RECT r; GetClientRect(h, out r);
      if ((r.R - r.L) > 100 && (r.B - r.T) > 100) { found = true; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@

if ($ProcName -eq "") { $ProcName = [System.IO.Path]::GetFileNameWithoutExtension($Exe) }
$times = @()

for ($i = 1; $i -le $Rounds; $i++) {
  Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 1200

  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $p = if ($Args -eq "") { Start-Process -FilePath $Exe -PassThru }
       else { Start-Process -FilePath $Exe -ArgumentList $Args -PassThru }

  $ok = $false
  while ($sw.ElapsedMilliseconds -lt 15000) {
    if ([CS]::HasVisibleWindow([uint32]$p.Id)) { $ok = $true; break }
    Start-Sleep -Milliseconds 5
  }
  $sw.Stop()
  if ($ok) {
    $times += $sw.Elapsed.TotalMilliseconds
    Write-Output ("round {0}: {1:F0} ms" -f $i, $sw.Elapsed.TotalMilliseconds)
  } else {
    Write-Output ("round {0}: TIMEOUT" -f $i)
  }
  Start-Sleep -Milliseconds 400
  Get-Process -Id $p.Id -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($times.Count -gt 0) {
  $stats = $times | Measure-Object -Average -Minimum -Maximum
  Write-Output ("--- {0}: avg {1:F0} ms  min {2:F0} ms  max {3:F0} ms  (n={4}) ---" -f `
    $ProcName, $stats.Average, $stats.Minimum, $stats.Maximum, $times.Count)
}
