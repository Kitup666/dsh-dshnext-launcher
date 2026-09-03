param(
  [string]$ProcName = "dshdesk",
  [string]$OutDir = "shots",
  [string]$ProfileName = "e2e-test"
)

# End-to-end functional drive of the launcher through UI Automation:
#   create a profile -> start it -> confirm the WebUI window appears -> check the
#   console received log lines -> stop -> delete the profile.
# Screenshots are taken at each milestone via PrintWindow so the run is auditable.
# NOTE: keep this file pure ASCII (Windows PowerShell reads .ps1 as ANSI without a BOM).

Add-Type -AssemblyName System.Drawing, UIAutomationClient, UIAutomationTypes

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class E2 {
  public delegate bool Proc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(Proc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int cy, uint f);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static IntPtr FindMain(uint pid, string cls, string titlePart) {
    // The launcher opens the harness WebUI in a second window of the same class, so
    // matching on class alone can return the WebUI window and break every lookup.
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true;
      var c = new StringBuilder(200); GetClassName(h, c, 200);
      if (c.ToString() != cls) return true;
      var t = new StringBuilder(300); GetWindowText(h, t, 300);
      if (t.ToString().Contains(titlePart)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static int CountByClass(uint pid, string cls) {
    int n = 0;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true;
      var c = new StringBuilder(200); GetClassName(h, c, 200);
      if (c.ToString() == cls) n++;
      return true;
    }, IntPtr.Zero);
    return n;
  }
  public static string TitlesByClass(uint pid, string cls) {
    var sb = new StringBuilder();
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true;
      var c = new StringBuilder(200); GetClassName(h, c, 200);
      if (c.ToString() == cls) {
        var t = new StringBuilder(300); GetWindowText(h, t, 300);
        sb.Append("|").Append(t.ToString());
      }
      return true;
    }, IntPtr.Zero);
    return sb.ToString();
  }
}
"@

[E2]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$proc = Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $proc) { Write-Output "FAIL: PROCESS_NOT_RUNNING"; exit 1 }
$pid32 = [uint32]$proc.Id
$main = [E2]::FindMain($pid32, "Tauri Window", "DshDesk")
if ($main -eq [IntPtr]::Zero) { Write-Output "FAIL: MAIN_WINDOW_NOT_FOUND"; exit 1 }
$preexisting = [E2]::CountByClass($pid32, "Tauri Window")
if ($preexisting -gt 1) {
  Write-Output ("FAIL: PRE-EXISTING EXTRA WINDOWS -> " + [E2]::TitlesByClass($pid32, "Tauri Window"))
  Write-Output "close leftover WebUI windows before running the e2e drive"
  exit 1
}

function Wake {
  if ([E2]::IsIconic($main)) { [E2]::ShowWindow($main, 9) | Out-Null; Start-Sleep -Milliseconds 900 }
  $r = New-Object E2+RECT
  [E2]::GetWindowRect($main, [ref]$r) | Out-Null
  if (($r.R - $r.L) -lt 900) {
    [E2]::SetWindowPos($main, [IntPtr]::Zero, 200, 120, 1468, 972, 0x0004 -bor 0x0010) | Out-Null
    Start-Sleep -Milliseconds 500
  }
  [E2]::BringWindowToTop($main) | Out-Null
  [E2]::SetForegroundWindow($main) | Out-Null
  Start-Sleep -Milliseconds 400
}

function Shot([string]$name) {
  Wake
  $r = New-Object E2+RECT
  [E2]::GetWindowRect($main, [ref]$r) | Out-Null
  $w = $r.R - $r.L; $h = $r.B - $r.T
  $bmp = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  [E2]::PrintWindow($main, $hdc, 2) | Out-Null
  $g.ReleaseHdc($hdc); $g.Dispose()
  $bmp.Save((Join-Path $OutDir ($name + ".png")), [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
}

$root = [System.Windows.Automation.AutomationElement]::FromHandle($main)
$btnType = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
  [System.Windows.Automation.ControlType]::Button)
$editType = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
  [System.Windows.Automation.ControlType]::Edit)
$textType = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
  [System.Windows.Automation.ControlType]::Text)

function Buttons { $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $btnType) }

function Click([string]$name, [int]$waitMs = 900) {
  foreach ($b in Buttons) {
    if ($b.Current.Name -eq $name) {
      $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
      Start-Sleep -Milliseconds $waitMs
      return $true
    }
  }
  return $false
}

function ClickNav([int]$index, [int]$waitMs = 1200) {
  $r = New-Object E2+RECT
  [E2]::GetWindowRect($main, [ref]$r) | Out-Null
  $limit = $r.L + [int](232 * (($r.R - $r.L) / 1174.0))
  $nav = @()
  foreach ($b in Buttons) { if ($b.Current.BoundingRectangle.Right -le $limit) { $nav += $b } }
  $nav = $nav | Sort-Object { $_.Current.BoundingRectangle.Top }
  if ($nav.Count -le $index) { return $false }
  $nav[$index].GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
  Start-Sleep -Milliseconds $waitMs
  return $true
}

function SetValue([string]$value) {
  # Use ValuePattern rather than SendKeys: SendKeys is affected by keyboard layout
  # and Caps Lock, which silently uppercased the typed profile name.
  $edits = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $editType)
  if ($edits.Count -eq 0) { return $false }
  $e = $edits[$edits.Count - 1]   # the modal's input is the newest edit in the tree
  $e.SetFocus()
  Start-Sleep -Milliseconds 200
  $vp = $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
  $vp.SetValue($value)
  Start-Sleep -Milliseconds 350
  return ($vp.Current.Value -eq $value)
}

# Find the action button belonging to one specific list row, identified by the row's
# label text. Taking the top/bottom-most button blindly hits whichever row sorting
# put there, which during development deleted the wrong profile.
function ClickRowAction([string]$rowLabel, [string]$action, [int]$waitMs = 1200) {
  $rowTop = $null
  foreach ($t in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textType)) {
    if ($t.Current.Name -eq $rowLabel) { $rowTop = $t.Current.BoundingRectangle.Top; break }
  }
  if ($null -eq $rowTop) { return $false }
  foreach ($b in Buttons) {
    $r = $b.Current.BoundingRectangle
    if ($b.Current.Name -eq $action -and [Math]::Abs($r.Top - $rowTop) -lt 46 -and $r.Left -gt 400) {
      $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
      Start-Sleep -Milliseconds $waitMs
      return $true
    }
  }
  return $false
}

function AllText {
  $sb = New-Object System.Text.StringBuilder
  foreach ($t in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textType)) {
    $sb.Append($t.Current.Name).Append("`n") | Out-Null
  }
  return $sb.ToString()
}

Add-Type -AssemblyName System.Windows.Forms
$fail = 0
function Check([string]$label, [bool]$ok) {
  if ($ok) { Write-Output ("PASS: " + $label) } else { Write-Output ("FAIL: " + $label); $script:fail++ }
}

Wake
Write-Output "--- step 1: create profile ---"
Check "nav to profiles" (ClickNav 1)
Check "open create dialog" (Click "+ 新建版本" 900)
Check "set profile name" (SetValue $ProfileName)
Shot "e2e-01-create-dialog"
Check "confirm create" (Click "创建" 2500)
$txt = AllText
Check "profile appears in list" ($txt -match [regex]::Escape($ProfileName))
Shot "e2e-02-created"

Write-Output "--- step 2: start it ---"
Check "invoke start on the test profile row" (ClickRowAction $ProfileName "启动" 1200)

Write-Output "waiting up to 90s for the WebUI window..."
$webUiSeen = $false
for ($i = 0; $i -lt 45; $i++) {
  Start-Sleep -Seconds 2
  if ([E2]::CountByClass($pid32, "Tauri Window") -ge 2) { $webUiSeen = $true; break }
}
Check "WebUI window opened" $webUiSeen
$titles = [E2]::TitlesByClass($pid32, "Tauri Window")
Write-Output ("window titles: " + $titles)
Check "WebUI window belongs to the test profile" ($titles -match [regex]::Escape($ProfileName))
Shot "e2e-03-running"

Write-Output "--- step 3: console received logs ---"
Check "nav to console" (ClickNav 4)
Start-Sleep -Milliseconds 1200
$ctxt = AllText
Check "console shows log lines" ($ctxt -notmatch "暂无输出")
Shot "e2e-04-console"

Write-Output "--- step 4: stop ---"
Check "nav to profiles" (ClickNav 1)
Check "invoke stop on the test profile row" (ClickRowAction $ProfileName "停止" 3000)
Start-Sleep -Seconds 3
$after = AllText
Check "row no longer marked running" ($after -notmatch "运行中")
Shot "e2e-05-stopped"

Write-Output "--- step 5: cleanup ---"
Check "open delete dialog for the test profile" (ClickRowAction $ProfileName "删除" 900)
Check "confirm delete" (Click "永久删除" 2500)
# Check the profile directory rather than the page text: the success toast repeats the
# profile name, so a text search would match even after a successful delete.
$profileDir = Join-Path $env:LOCALAPPDATA ("DshDesk\home\profiles\" + $ProfileName)
Check "test profile directory removed" (-not (Test-Path $profileDir))
Shot "e2e-06-cleaned"

Write-Output ("=== E2E RESULT: " + $(if ($fail -eq 0) { "ALL PASS" } else { "$fail CHECK(S) FAILED" }))

# Leave no WebUI window behind, otherwise the next run aborts on the pre-existing check
foreach ($t in ([E2]::TitlesByClass($pid32, "Tauri Window") -split "\|")) {
  if ($t -and $t -notmatch "DshDesk") { Write-Output ("note: leftover window '" + $t + "'") }
}
