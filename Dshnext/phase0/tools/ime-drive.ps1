param(
  [int]$WaitMs = 2600,
  [string]$OutDir = "shots"
)

# Drive the phase0 IME scene with the real Microsoft Pinyin IME and capture the
# window mid-composition, i.e. while the candidate list is on screen.
#
# Why not iced's own window::screenshot: it renders the app surface only. The IME
# candidate window is a separate top-level window owned by the IME, so it is
# invisible to an in-process capture. Only a screen-region grab can prove the
# candidate box exists and sits next to the caret.
#
# Two things this script learned the hard way:
#  - SetForegroundWindow silently no-ops when the caller is not already the
#    foreground process. Without the AttachThreadInput dance below, every
#    keystroke lands in whatever window actually had focus and the probe looks
#    like it ignored the IME.
#  - the input language is switched by posting WM_INPUTLANGCHANGEREQUEST to the
#    target window rather than by sending Ctrl+Space, which depends on the user's
#    IME hotkey configuration and cannot be verified.
#
# NOTE: keep this file pure ASCII. Windows PowerShell reads .ps1 as ANSI without
# a BOM, so non-ASCII literals here would become mojibake and fail to parse.

Add-Type -AssemblyName System.Drawing, System.Windows.Forms

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Ime {
  public delegate bool Proc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(Proc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, IntPtr extra);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint from, uint to, bool attach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern IntPtr LoadKeyboardLayout(string id, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern short GetKeyState(int vk);
  [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint tid);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static IntPtr FindMain(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true;
      var t = new StringBuilder(300); GetWindowText(h, t, 300);
      if (t.ToString().Length > 0) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  // SetForegroundWindow only obeys the caller when their input queues are
  // attached, so borrow the current foreground thread's input first.
  public static bool ForceForeground(IntPtr h) {
    IntPtr fg = GetForegroundWindow();
    uint fgPid; uint fgTid = GetWindowThreadProcessId(fg, out fgPid);
    uint me = GetCurrentThreadId();
    if (fgTid != 0 && fgTid != me) AttachThreadInput(me, fgTid, true);
    BringWindowToTop(h);
    bool ok = SetForegroundWindow(h);
    if (fgTid != 0 && fgTid != me) AttachThreadInput(me, fgTid, false);
    return ok;
  }
}
"@

[Ime]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$proc = Get-Process -Name "phase0" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $proc) { Write-Output "PROCESS_NOT_RUNNING"; exit 1 }
$hwnd = [Ime]::FindMain([uint32]$proc.Id)
if ($hwnd -eq [IntPtr]::Zero) { Write-Output "WINDOW_NOT_FOUND"; exit 1 }

# A minimized or off-screen window yields a stub image, so pin it first.
if ([Ime]::IsIconic($hwnd)) { [Ime]::ShowWindow($hwnd, 9) | Out-Null; Start-Sleep -Milliseconds 500 }
[Ime]::SetWindowPos($hwnd, [IntPtr]::Zero, 60, 60, 1180, 820, 0x0040) | Out-Null
Start-Sleep -Milliseconds 300

$r = New-Object Ime+RECT
[Ime]::GetWindowRect($hwnd, [ref]$r) | Out-Null
Write-Output ("window rect {0},{1} {2}x{3}" -f $r.L, $r.T, ($r.R - $r.L), ($r.B - $r.T))

# Microsoft Pinyin passes keystrokes through as plain English while CapsLock is
# on, so the IME never opens and no InputMethod event is ever produced. This cost
# a full debugging round: keys arrived as Character("H") with no modifiers.
if (([Ime]::GetKeyState(0x14) -band 1) -ne 0) {
  Write-Output "CapsLock is ON - turning it off, otherwise the IME stays in English"
  [Ime]::keybd_event(0x14, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 60
  [Ime]::keybd_event(0x14, 0, 2, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 300
}
Write-Output ("CapsLock now {0}" -f (([Ime]::GetKeyState(0x14) -band 1) -ne 0))

# Do NOT click inside the window. The app focuses its own text input on startup,
# and iced unfocuses a text_input on any click that misses it -- a click on the
# card background silently killed focus, so on_input never fired and the probe
# looked like the IME was broken. Activation alone is enough.
function Activate {
  [Ime]::ForceForeground($hwnd) | Out-Null
  Start-Sleep -Milliseconds 400
  return ([Ime]::GetForegroundWindow() -eq $hwnd)
}

$active = $false
for ($try = 1; $try -le 4; $try++) {
  if (Activate) { $active = $true; break }
  Write-Output ("activation attempt {0} failed, retrying" -f $try)
  Start-Sleep -Milliseconds 600
}
if (-not $active) {
  $fg = [Ime]::GetForegroundWindow()
  $t = New-Object System.Text.StringBuilder 300
  [Ime]::GetWindowText($fg, $t, 300) | Out-Null
  Write-Output ("CANNOT_ACTIVATE foreground is '{0}' - keystrokes would go there" -f $t.ToString())
  exit 1
}
Write-Output "window is foreground; typing will reach it"

# Switch the target window to Simplified Chinese (Microsoft Pinyin). 0x0804 is
# zh-CN; KLF_ACTIVATE = 1.
$hkl = [Ime]::LoadKeyboardLayout("00000804", 1)
Write-Output ("loaded zh-CN layout hkl={0}" -f $hkl)
[Ime]::PostMessage($hwnd, 0x0050, [IntPtr]::Zero, $hkl) | Out-Null   # WM_INPUTLANGCHANGEREQUEST
Start-Sleep -Milliseconds 1200
$tid = 0
[Ime]::GetWindowThreadProcessId($hwnd, [ref]$tid) | Out-Null
Write-Output ("target thread layout now {0}" -f [Ime]::GetKeyboardLayout([uint32]$tid))

function Grab([string]$name) {
  $r2 = New-Object Ime+RECT
  [Ime]::GetWindowRect($hwnd, [ref]$r2) | Out-Null
  # Grab a region taller than the window: the candidate popup can hang below it.
  $w = ($r2.R - $r2.L) + 40
  $h = ($r2.B - $r2.T) + 260
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r2.L - 20, $r2.T - 20, 0, 0, (New-Object System.Drawing.Size $w, $h))
  $path = Join-Path $OutDir $name
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Output ("saved {0} ({1}x{2})" -f $path, $w, $h)
}

# keybd_event, not SendKeys: SendKeys goes through a higher-level queue that the
# IME may not treat as physical input.
function TypeLetters([string]$letters) {
  foreach ($ch in $letters.ToUpper().ToCharArray()) {
    $vk = [byte][char]$ch
    [Ime]::keybd_event($vk, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [Ime]::keybd_event($vk, 0, 2, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 110
  }
  Start-Sleep -Milliseconds $WaitMs
}

Write-Output "typing pinyin 'shenduqiusuo' (uncommitted)"
TypeLetters "shenduqiusuo"
Grab "ime-preedit.png"

# Space commits the first candidate. If Commit events reach the widget, the app
# log prints the real Chinese string.
[Ime]::keybd_event(0x20, 0, 0, [IntPtr]::Zero)
Start-Sleep -Milliseconds 60
[Ime]::keybd_event(0x20, 0, 2, [IntPtr]::Zero)
Start-Sleep -Milliseconds 1200
Grab "ime-committed.png"

# Backspace once: a correct integration deletes one whole hanzi, not one byte.
[Ime]::keybd_event(0x08, 0, 0, [IntPtr]::Zero)
Start-Sleep -Milliseconds 60
[Ime]::keybd_event(0x08, 0, 2, [IntPtr]::Zero)
Start-Sleep -Milliseconds 900
Grab "ime-backspace.png"
Write-Output "DONE"
