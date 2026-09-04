param(
  [Parameter(Mandatory=$true)][string]$Exe,
  [string]$Args = "",
  [int]$Rounds = 5,
  [string]$ProcName = ""
)

# Measure time to first *painted* frame, not time to window creation.
#
# Window creation alone is a misleading metric: a Tauri window appears as an
# empty shell in ~90 ms and only fills in once WebView2 has loaded the bundle, so
# comparing "window visible" times would flatter it. Here the clock stops when
# the window's own surface (PrintWindow, PW_RENDERFULLCONTENT) contains real
# content, judged by counting distinct colors on a sampling grid -- a blank or
# single-color surface has 1-2, a rendered UI has dozens.
#
# NOTE: keep this file pure ASCII (see other scripts in this folder).

Add-Type -AssemblyName System.Drawing

Add-Type -ReferencedAssemblies System.Drawing @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Drawing;
using System.Runtime.InteropServices;
public class FP {
  public delegate bool Proc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(Proc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }

  public static IntPtr FindWindow(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true;
      if (!IsWindowVisible(h)) return true;
      var t = new StringBuilder(300); GetWindowText(h, t, 300);
      if (t.ToString().Length == 0) return true;
      RECT r; GetClientRect(h, out r);
      if ((r.R - r.L) > 100 && (r.B - r.T) > 100) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }

  // Count distinct colors on a coarse grid of the window's own surface.
  public static int ColorCount(IntPtr h) {
    RECT r; if (!GetClientRect(h, out r)) return 0;
    int w = r.R - r.L, ht = r.B - r.T;
    if (w < 100 || ht < 100) return 0;
    using (var bmp = new Bitmap(w, ht))
    using (var g = Graphics.FromImage(bmp)) {
      IntPtr hdc = g.GetHdc();
      bool ok = PrintWindow(h, hdc, 2);
      g.ReleaseHdc(hdc);
      if (!ok) return 0;
      var set = new HashSet<int>();
      for (int y = 4; y < ht; y += 7)
        for (int x = 4; x < w; x += 7)
          set.Add(bmp.GetPixel(x, y).ToArgb());
      return set.Count;
    }
  }
}
"@

[FP]::SetProcessDPIAware() | Out-Null
if ($ProcName -eq "") { $ProcName = [System.IO.Path]::GetFileNameWithoutExtension($Exe) }
$wins = @(); $paints = @()

for ($i = 1; $i -le $Rounds; $i++) {
  Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 1500

  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $p = if ($Args -eq "") { Start-Process -FilePath $Exe -PassThru }
       else { Start-Process -FilePath $Exe -ArgumentList $Args -PassThru }

  $hwnd = [IntPtr]::Zero; $winMs = -1
  while ($sw.ElapsedMilliseconds -lt 20000) {
    $hwnd = [FP]::FindWindow([uint32]$p.Id)
    if ($hwnd -ne [IntPtr]::Zero) { $winMs = $sw.Elapsed.TotalMilliseconds; break }
    Start-Sleep -Milliseconds 4
  }

  $paintMs = -1
  if ($hwnd -ne [IntPtr]::Zero) {
    while ($sw.ElapsedMilliseconds -lt 20000) {
      if ([FP]::ColorCount($hwnd) -ge 20) { $paintMs = $sw.Elapsed.TotalMilliseconds; break }
      Start-Sleep -Milliseconds 4
    }
  }
  $sw.Stop()

  if ($winMs -ge 0 -and $paintMs -ge 0) {
    $wins += $winMs; $paints += $paintMs
    Write-Output ("round {0}: window {1:F0} ms  first paint {2:F0} ms" -f $i, $winMs, $paintMs)
  } else {
    Write-Output ("round {0}: window {1:F0} ms  first paint TIMEOUT" -f $i, $winMs)
  }
  Start-Sleep -Milliseconds 300
  Get-Process -Id $p.Id -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($paints.Count -gt 0) {
  $w = $wins | Measure-Object -Average -Minimum -Maximum
  $q = $paints | Measure-Object -Average -Minimum -Maximum
  Write-Output ("--- {0}: window avg {1:F0} ms | FIRST PAINT avg {2:F0} ms (min {3:F0}, max {4:F0}, n={5}) ---" -f `
    $ProcName, $w.Average, $q.Average, $q.Minimum, $q.Maximum, $paints.Count)
}
