# 在指定窗口的客户区坐标处点击（DPI 感知；X/Y 传 CSS 逻辑像素，脚本按窗口 DPI 换算）
param(
  [string]$TitleLike = "DshDesk",
  [Parameter(Mandatory = $true)][int]$X,
  [Parameter(Mandatory = $true)][int]$Y
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Clk {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
"@

[Clk]::SetProcessDPIAware() | Out-Null

$proc = Get-Process | Where-Object { $_.MainWindowTitle -like "*$TitleLike*" } | Select-Object -First 1
if ($null -eq $proc) { Write-Output "WINDOW_NOT_FOUND"; exit 1 }

[Clk]::ShowWindow($proc.MainWindowHandle, 9) | Out-Null
[Clk]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 500

$dpi = [Clk]::GetDpiForWindow($proc.MainWindowHandle)
if ($dpi -le 0) { $dpi = 96 }
$scale = $dpi / 96.0

$pt = New-Object Clk+POINT
$pt.X = [int]($X * $scale)
$pt.Y = [int]($Y * $scale)
[Clk]::ClientToScreen($proc.MainWindowHandle, [ref]$pt) | Out-Null
[Clk]::SetCursorPos($pt.X, $pt.Y) | Out-Null
Start-Sleep -Milliseconds 180
[Clk]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)   # LEFTDOWN
Start-Sleep -Milliseconds 60
[Clk]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)   # LEFTUP
Write-Output ("CLICKED css({0},{1}) scale={2} -> screen({3},{4})" -f $X, $Y, $scale, $pt.X, $pt.Y)
