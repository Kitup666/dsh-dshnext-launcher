# 截取指定标题的窗口（DPI 感知），用于 UI 视觉验收
param(
  [string]$TitleLike = "DshDesk",
  [string]$Out = "shot.png"
)

Add-Type -AssemblyName System.Drawing, System.Windows.Forms

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@

# 必须在读取任何坐标前声明 DPI 感知，否则拿到的是虚拟化后的逻辑坐标
[Win]::SetProcessDPIAware() | Out-Null

$proc = Get-Process | Where-Object { $_.MainWindowTitle -like "*$TitleLike*" } | Select-Object -First 1
if ($null -eq $proc) {
  Write-Output "WINDOW_NOT_FOUND"
  exit 1
}

[Win]::ShowWindow($proc.MainWindowHandle, 9) | Out-Null   # SW_RESTORE
[Win]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 900

$r = New-Object Win+RECT
[Win]::GetWindowRect($proc.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.R - $r.L
$h = $r.B - $r.T
if ($w -le 0 -or $h -le 0) { Write-Output "BAD_RECT"; exit 1 }

$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object System.Drawing.Size($w, $h)))
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output ("SAVED: {0} [{1}x{2}] from ({3},{4})" -f $Out, $w, $h, $r.L, $r.T)
