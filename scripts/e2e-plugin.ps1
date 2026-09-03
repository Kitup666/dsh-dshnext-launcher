param(
  [string]$ProcName = "dshdesk",
  [string]$OutDir = "shots",
  [string]$ProfileName = "plug-test",
  [string]$PluginSpec = "dsh-cost-meter"
)

# Drive a real plugin install through the launcher UI: create a profile, load the
# marketplace, install a plugin by npm spec, verify it lands in the profile's
# package.json, then uninstall and clean up.
# NOTE: keep this file UTF-8 with BOM so PowerShell reads the Chinese button labels.

Add-Type -AssemblyName System.Drawing, UIAutomationClient, UIAutomationTypes

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class PT {
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
}
"@

[PT]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$proc = Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $proc) { Write-Output "FAIL: PROCESS_NOT_RUNNING"; exit 1 }
$pid32 = [uint32]$proc.Id
$main = [PT]::FindMain($pid32, "Tauri Window", "DshDesk")
if ($main -eq [IntPtr]::Zero) { Write-Output "FAIL: MAIN_WINDOW_NOT_FOUND"; exit 1 }

function Wake {
  if ([PT]::IsIconic($main)) { [PT]::ShowWindow($main, 9) | Out-Null; Start-Sleep -Milliseconds 900 }
  $r = New-Object PT+RECT
  [PT]::GetWindowRect($main, [ref]$r) | Out-Null
  if (($r.R - $r.L) -lt 900) {
    [PT]::SetWindowPos($main, [IntPtr]::Zero, 200, 120, 1468, 972, 0x0004 -bor 0x0010) | Out-Null
    Start-Sleep -Milliseconds 500
  }
  [PT]::BringWindowToTop($main) | Out-Null
  [PT]::SetForegroundWindow($main) | Out-Null
  Start-Sleep -Milliseconds 400
}

function Shot([string]$name) {
  Wake
  $r = New-Object PT+RECT
  [PT]::GetWindowRect($main, [ref]$r) | Out-Null
  $bmp = New-Object System.Drawing.Bitmap(($r.R - $r.L), ($r.B - $r.T), [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  [PT]::PrintWindow($main, $hdc, 2) | Out-Null
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

function ClickStartsWith([string]$prefix, [int]$waitMs = 900) {
  foreach ($b in Buttons) {
    if ($b.Current.Name.StartsWith($prefix)) {
      $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
      Start-Sleep -Milliseconds $waitMs
      return $true
    }
  }
  return $false
}

function ClickNav([int]$index, [int]$waitMs = 1200) {
  $r = New-Object PT+RECT
  [PT]::GetWindowRect($main, [ref]$r) | Out-Null
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
  $edits = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $editType)
  if ($edits.Count -eq 0) { return $false }
  $e = $edits[$edits.Count - 1]
  $e.SetFocus()
  Start-Sleep -Milliseconds 200
  $vp = $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
  $vp.SetValue($value)
  Start-Sleep -Milliseconds 350
  return ($vp.Current.Value -eq $value)
}

function AllText {
  $sb = New-Object System.Text.StringBuilder
  foreach ($t in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textType)) {
    $sb.Append($t.Current.Name).Append("`n") | Out-Null
  }
  return $sb.ToString()
}

$fail = 0
function Check([string]$label, [bool]$ok) {
  if ($ok) { Write-Output ("PASS: " + $label) } else { Write-Output ("FAIL: " + $label); $script:fail++ }
}

$profileDir = Join-Path $env:LOCALAPPDATA ("DshDesk\home\profiles\" + $ProfileName)
$pkgPath = Join-Path $profileDir "package.json"

Wake
Write-Output "--- step 1: create a scratch profile ---"
Check "nav to profiles" (ClickNav 1)
Check "open create dialog" (Click "+ 新建版本" 900)
Check "set name" (SetValue $ProfileName)
Check "create" (Click "创建" 2500)
Check "profile dir exists" (Test-Path $pkgPath)

Write-Output "--- step 2: marketplace loads from npm ---"
# Enter the plugins page through the profile row's own 插件 button: driving the page's
# combo box via UI Automation changes the DOM value without firing React's change
# handler, so the page would keep operating on the previously selected profile.
$entered = $false
foreach ($t in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textType)) {
  if ($t.Current.Name -eq $ProfileName) {
    $rowTop = $t.Current.BoundingRectangle.Top
    foreach ($b in Buttons) {
      $r = $b.Current.BoundingRectangle
      if ($b.Current.Name -eq "插件" -and [Math]::Abs($r.Top - $rowTop) -lt 46 -and $r.Left -gt 400) {
        $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 1500
        $entered = $true
        break
      }
    }
    break
  }
}
Check "opened plugins page for the scratch profile" $entered
Check "open marketplace tab" (ClickStartsWith "插件市场" 6000)
Start-Sleep -Seconds 4
$mtxt = AllText
Check "marketplace listed packages" ($mtxt -match "dsh-" -and $mtxt -notmatch "没有匹配的插件")
Shot "plug-01-market"

Write-Output "--- step 3: install a plugin ---"
Check "open manual install dialog" (Click "手动安装" 900)
Check "set plugin spec" (SetValue $PluginSpec)
Shot "plug-02-install-dialog"
# The modal's confirm button reads 安装插件, distinct from the marketplace rows' 安装
# buttons -- matching bare "安装" clicked a marketplace row instead of confirming.
Check "submit install" (Click "安装插件" 1500)
Write-Output "waiting up to 240s for pnpm to finish..."
$installed = $false
for ($i = 0; $i -lt 80; $i++) {
  Start-Sleep -Seconds 3
  if (Test-Path $pkgPath) {
    $pkg = Get-Content -Raw -Encoding UTF8 $pkgPath | ConvertFrom-Json
    if ($pkg.dependencies -and $pkg.dependencies.PSObject.Properties.Name -contains $PluginSpec) {
      $installed = $true; break
    }
  }
}
Check "plugin recorded in profile package.json" $installed
if ($installed) {
  $pkg = Get-Content -Raw -Encoding UTF8 $pkgPath | ConvertFrom-Json
  Write-Output ("  dependencies: " + ($pkg.dependencies.PSObject.Properties | ForEach-Object { $_.Name + '@' + $_.Value }))
}
$modDir = Join-Path $profileDir ("node_modules\" + $PluginSpec)
Check "plugin package present in node_modules" (Test-Path $modDir)
Start-Sleep -Seconds 2
$itxt = AllText
Check "installed tab shows the plugin" ($itxt -match [regex]::Escape($PluginSpec))
Shot "plug-03-installed"

Write-Output "--- step 4: uninstall ---"
# After installing we are still on the marketplace tab, whose rows only offer 安装 /
# 重新安装; the 卸载 button lives on the 已安装 tab, so switch back first.
$removed = $false
Check "switch to installed tab" (ClickStartsWith "已安装" 1500)
Start-Sleep -Milliseconds 800
if (Click "卸载" 1200) {
  if (Click "确认卸载" 1500) {
    for ($i = 0; $i -lt 60; $i++) {
      Start-Sleep -Seconds 3
      $pkg = Get-Content -Raw -Encoding UTF8 $pkgPath | ConvertFrom-Json
      $names = @()
      if ($pkg.dependencies) { $names = $pkg.dependencies.PSObject.Properties.Name }
      if ($names -notcontains $PluginSpec) { $removed = $true; break }
    }
  }
}
Check "plugin removed from package.json" $removed
Shot "plug-04-uninstalled"

Write-Output "--- step 5: cleanup ---"
Check "nav to profiles" (ClickNav 1)
$deleted = $false
foreach ($t in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $textType)) {
  if ($t.Current.Name -eq $ProfileName) {
    $rowTop = $t.Current.BoundingRectangle.Top
    foreach ($b in Buttons) {
      $r = $b.Current.BoundingRectangle
      if ($b.Current.Name -eq "删除" -and [Math]::Abs($r.Top - $rowTop) -lt 46 -and $r.Left -gt 400) {
        $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 900
        $deleted = Click "永久删除" 3000
        break
      }
    }
    break
  }
}
Check "scratch profile deleted" ($deleted -and -not (Test-Path $profileDir))

Write-Output ("=== PLUGIN E2E RESULT: " + $(if ($fail -eq 0) { "ALL PASS" } else { "$fail CHECK(S) FAILED" }))
