param(
  [string]$ProcName = "dshnext"
)

# Click a few known widget coordinates in the Dshnext window and report whether
# the point actually landed on that window. Used to tell "iced ignored the click"
# apart from "the click went to another window".
#
# NOTE: keep this file pure ASCII. Windows PowerShell reads .ps1 as ANSI without
# a BOM, so non-ASCII literals would become mojibake and fail to parse.

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Clk {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool c);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@

[Clk]::SetProcessDPIAware() | Out-Null

$p = Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $p) { Write-Output "PROCESS_NOT_RUNNING"; exit 1 }
$hwnd = $p.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) { Write-Output "NO_WINDOW"; exit 1 }

# SetForegroundWindow fails silently unless we borrow the current foreground
# thread's input queue first (AGENTS.md, Windows automation note 1).
[Clk]::ShowWindow($hwnd, 9) | Out-Null
for ($i = 0; $i -lt 4; $i++) {
  $fg = [Clk]::GetForegroundWindow()
  if ($fg -eq $hwnd) { break }
  $tid = [Clk]::GetWindowThreadProcessId($fg, [IntPtr]::Zero)
  [Clk]::AttachThreadInput($tid, [Clk]::GetCurrentThreadId(), $true) | Out-Null
  [Clk]::SetForegroundWindow($hwnd) | Out-Null
  [Clk]::AttachThreadInput($tid, [Clk]::GetCurrentThreadId(), $false) | Out-Null
  Start-Sleep -Milliseconds 350
}
$onFg = ([Clk]::GetForegroundWindow() -eq $hwnd)
Write-Output ("foreground=" + $onFg)

$org = New-Object Clk+POINT
[Clk]::ClientToScreen($hwnd, [ref]$org) | Out-Null
$cr = New-Object Clk+RECT
[Clk]::GetClientRect($hwnd, [ref]$cr) | Out-Null
Write-Output ("clientOrigin=" + $org.X + "," + $org.Y + " clientSize=" + $cr.R + "x" + $cr.B)

function Act([int]$x, [int]$y, [string]$what) {
  $tp = New-Object Clk+POINT
  $tp.X = $org.X + $x
  $tp.Y = $org.Y + $y
  [Clk]::SetCursorPos($tp.X, $tp.Y) | Out-Null
  Start-Sleep -Milliseconds 450
  $u = [Clk]::WindowFromPoint($tp)
  $hit = ($u -eq $hwnd)
  Write-Output ("  click " + $what + " client=" + $x + "," + $y + " hitTarget=" + $hit)
  if (-not $hit) { return }
  [Clk]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 110
  [Clk]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 800
}

# Coordinates are physical client pixels at 125% scale (window 1280x860 logical).
Act 440 573 "primary-button"
Act 1495 88 "theme-toggle"
Act 145 113 "sidebar-nav"
Start-Sleep -Milliseconds 600
Write-Output "done"
