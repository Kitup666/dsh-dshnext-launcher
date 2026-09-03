param(
  [string]$ProcName = "dshdesk",
  [string]$OutDir = "shots"
)

# Capture each page reliably:
#  - the window is located via EnumWindows on the target process and class "Tauri Window",
#    then restored and given a known size, so a minimized or off-screen window (which
#    silently yields ~200x34 stub images) can never be captured
#  - navigation uses UI Automation Invoke on the sidebar buttons ordered top-to-bottom,
#    re-queried each round because React rebuilds the tree on navigation
#  - the image comes from PrintWindow(PW_RENDERFULLCONTENT), i.e. the window's own
#    surface, so another app taking focus cannot pollute the capture
# NOTE: keep this file pure ASCII. Windows PowerShell reads .ps1 as ANSI without a BOM,
# so non-ASCII literals here would become mojibake and fail to parse.

Add-Type -AssemblyName System.Drawing, UIAutomationClient, UIAutomationTypes

Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class Cap {
  public delegate bool Proc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(Proc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int cy, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static IntPtr FindByClass(uint pid, string cls) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true;
      var c = new StringBuilder(200); GetClassName(h, c, 200);
      if (c.ToString() == cls) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@

[Cap]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$proc = Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $proc) { Write-Output "PROCESS_NOT_RUNNING"; exit 1 }
$hwnd = [Cap]::FindByClass([uint32]$proc.Id, "Tauri Window")
if ($hwnd -eq [IntPtr]::Zero) { Write-Output "WINDOW_NOT_FOUND"; exit 1 }

function Ensure-Restored {
  # A minimized or occluded WebView2 window keeps an empty UI Automation tree and makes
  # PrintWindow return stale pixels, so bring the window up before touching either.
  if ([Cap]::IsIconic($hwnd)) {
    [Cap]::ShowWindow($hwnd, 9) | Out-Null   # SW_RESTORE
    Start-Sleep -Milliseconds 900
  }
  $r = New-Object Cap+RECT
  [Cap]::GetWindowRect($hwnd, [ref]$r) | Out-Null
  if (($r.R - $r.L) -lt 900 -or ($r.B - $r.T) -lt 600 -or $r.L -lt -2000) {
    # SWP_NOZORDER|SWP_NOACTIVATE, restore to the configured default size
    [Cap]::SetWindowPos($hwnd, [IntPtr]::Zero, 200, 120, 1468, 972, 0x0004 -bor 0x0010) | Out-Null
    Start-Sleep -Milliseconds 600
  }
  # WebView2 only builds its accessibility tree once the window is actually shown
  [Cap]::BringWindowToTop($hwnd) | Out-Null
  [Cap]::SetForegroundWindow($hwnd) | Out-Null
  Start-Sleep -Milliseconds 500
  return -not [Cap]::IsIconic($hwnd)
}

Ensure-Restored | Out-Null
if ([Cap]::IsIconic($hwnd)) { Write-Output "WINDOW_STILL_MINIMIZED - aborting"; exit 1 }
$root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
$rect0 = New-Object Cap+RECT
[Cap]::GetWindowRect($hwnd, [ref]$rect0) | Out-Null
$scaleGuess = ($rect0.R - $rect0.L) / 1174.0
$sidebarRight = $rect0.L + [int](232 * $scaleGuess)

$typeCond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
  [System.Windows.Automation.ControlType]::Button)

function Get-NavButtons {
  $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $typeCond)
  $list = @()
  foreach ($b in $all) {
    $r = $b.Current.BoundingRectangle
    if ($r.Width -le 0) { continue }
    if ($r.Right -le $sidebarRight) { $list += $b }
  }
  return $list | Sort-Object { $_.Current.BoundingRectangle.Top }
}

function Save-Window([string]$path) {
  if (-not (Ensure-Restored)) { return "MINIMIZED" }
  $r = New-Object Cap+RECT
  [Cap]::GetWindowRect($hwnd, [ref]$r) | Out-Null
  $w = $r.R - $r.L
  $h = $r.B - $r.T
  if ($w -lt 900 -or $h -lt 600) { return ("BAD_RECT {0}x{1}" -f $w, $h) }
  $bmp = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  $ok = [Cap]::PrintWindow($hwnd, $hdc, 2)
  $g.ReleaseHdc($hdc)
  $g.Dispose()
  if (-not $ok) { $bmp.Dispose(); return "PRINTWINDOW_FAILED" }
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $res = "OK {0}x{1}" -f $bmp.Width, $bmp.Height
  $bmp.Dispose()
  return $res
}

$names = @("01-home", "02-profiles", "03-plugins", "04-env", "05-console", "06-settings")
$nav = Get-NavButtons
Write-Output ("nav buttons: {0} (window {1}x{2})" -f $nav.Count, ($rect0.R - $rect0.L), ($rect0.B - $rect0.T))
if ($nav.Count -lt $names.Count) { Write-Output "NAV_INCOMPLETE - aborting"; exit 1 }

for ($i = 0; $i -lt $names.Count; $i++) {
  $label = "?"
  try {
    $nav = Get-NavButtons
    $el = $nav[$i]
    $label = $el.Current.Name
    $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
  } catch {
    Write-Output ("{0}: INVOKE_FAILED ({1})" -f $names[$i], $_.Exception.GetType().Name)
    continue
  }
  Start-Sleep -Milliseconds 1500
  $out = Join-Path $OutDir ($names[$i] + ".png")
  Write-Output ("{0} <- nav[{1}] '{2}': {3}" -f $names[$i], $i, $label, (Save-Window $out))
}
