param(
  [string]$ProcName = "dshnext"
)

# Interaction smoke test for the Dshnext window: open a modal, close it with ESC,
# switch pages with Ctrl+N, and click a sidebar item. Every click asserts that the
# point actually lands on the target window first.
#
# Two traps this avoids (see AGENTS.md):
#  - another window (e.g. an in-app browser pane) covering the click point, so the
#    click silently goes elsewhere. We force the window topmost at a known rect and
#    assert WindowFromPoint before each click.
#  - SetForegroundWindow failing silently when the caller is not the foreground
#    process. We borrow the current foreground thread's input queue first.
#
# Run the app with RUST_LOG=dshnext=debug to see which messages arrived.
#
# NOTE: keep this file pure ASCII. Windows PowerShell reads .ps1 as ANSI without a
# BOM, so non-ASCII literals here would become mojibake and fail to parse.

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Ix {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int ht, uint f);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte k, byte s, uint f, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool c);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@

[Ix]::SetProcessDPIAware() | Out-Null

$p = Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $p) { Write-Output "PROCESS_NOT_RUNNING"; exit 1 }
$hwnd = $p.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) { Write-Output "NO_WINDOW"; exit 1 }

# HWND_TOPMOST at a known rect: nothing can cover the click points.
[Ix]::SetWindowPos($hwnd, [IntPtr](-1), 40, 40, 1600, 1120, 0x0040) | Out-Null
Start-Sleep -Milliseconds 700

for ($i = 0; $i -lt 4; $i++) {
  $fg = [Ix]::GetForegroundWindow()
  if ($fg -eq $hwnd) { break }
  $tid = [Ix]::GetWindowThreadProcessId($fg, [IntPtr]::Zero)
  [Ix]::AttachThreadInput($tid, [Ix]::GetCurrentThreadId(), $true) | Out-Null
  [Ix]::SetForegroundWindow($hwnd) | Out-Null
  [Ix]::AttachThreadInput($tid, [Ix]::GetCurrentThreadId(), $false) | Out-Null
  Start-Sleep -Milliseconds 350
}
# The first key is easily lost: right after SetForegroundWindow the window has not
# actually taken over the input queue yet. Wait and re-check before sending keys.
Start-Sleep -Milliseconds 900
$onFg = ([Ix]::GetForegroundWindow() -eq $hwnd)
Write-Output ("foreground=" + $onFg)
if (-not $onFg) { Write-Output "WARN: not foreground, keys may be lost" }

# Client-area geometry, recomputed here because the window was just moved.
$org = New-Object Ix+POINT
[Ix]::ClientToScreen($hwnd, [ref]$org) | Out-Null
$cr = New-Object Ix+RECT
[Ix]::GetClientRect($hwnd, [ref]$cr) | Out-Null
$scale = 1.25
Write-Output ("clientOrigin=" + $org.X + "," + $org.Y + " clientSize=" + $cr.R + "x" + $cr.B)

# Logical (CSS-like) coordinates -> physical pixels.
function ClickLogical([int]$lx, [int]$ly, [string]$what) {
  $tp = New-Object Ix+POINT
  $tp.X = $org.X + [int]($lx * $scale)
  $tp.Y = $org.Y + [int]($ly * $scale)
  [Ix]::SetCursorPos($tp.X, $tp.Y) | Out-Null
  Start-Sleep -Milliseconds 350
  $hit = ([Ix]::WindowFromPoint($tp) -eq $hwnd)
  Write-Output ("  click " + $what + " logical=" + $lx + "," + $ly + " hit=" + $hit)
  if (-not $hit) { return }
  [Ix]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 100
  [Ix]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 800
}

function Key([byte]$vk, [string]$what) {
  [Ix]::keybd_event($vk, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 60
  [Ix]::keybd_event($vk, 0, 2, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 800
  Write-Output ("  key " + $what)
}

function CtrlKey([byte]$vk, [string]$what) {
  [Ix]::keybd_event(0x11, 0, 0, [IntPtr]::Zero)
  [Ix]::keybd_event($vk, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 60
  [Ix]::keybd_event($vk, 0, 2, [IntPtr]::Zero)
  [Ix]::keybd_event(0x11, 0, 2, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 800
  Write-Output ("  key Ctrl+" + $what)
}

# Sidebar nav item i center (logical y):
#   38 titlebar + 22 padding + 34 brand + 24 spacer + i*(36+2) + 18 = 136 + 38i
# The window is 1600x1120 physical at 1.25x = 1280x896 logical, so x must stay
# below 1280 -- the earlier 1430 fell outside the client area and never hit.
function NavY([int]$i) { return 136 + 38 * $i }

CtrlKey 0x32 "2 (profiles)"
ClickLogical 1180 88 "new-profile-button"
Key 0x1B "ESC (close modal)"
CtrlKey 0x34 "4 (env)"
ClickLogical 116 (NavY 4) "nav-console"
CtrlKey 0x36 "6 (settings)"
Write-Output "done"
