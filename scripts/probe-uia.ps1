Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public class Q {
  public delegate bool Proc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(Proc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  public static IntPtr F(uint pid, string cls) { IntPtr f=IntPtr.Zero;
    EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p!=pid) return true;
      var c=new StringBuilder(200); GetClassName(h,c,200);
      if(c.ToString()==cls){ f=h; return false; } return true; }, IntPtr.Zero); return f; }
}
"@
[Q]::SetProcessDPIAware() | Out-Null
$pr = Get-Process -Name dshdesk | Select-Object -First 1
$h = [Q]::F([uint32]$pr.Id, "Tauri Window")
$root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
$all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
Write-Output ("total elements: " + $all.Count)
foreach ($e in $all) {
  $r = $e.Current.BoundingRectangle
  Write-Output ("  " + $e.Current.ControlType.ProgrammaticName.Replace("ControlType.", "") + " [" + $e.Current.Name + "] L=" + [int]$r.Left + " T=" + [int]$r.Top)
}
