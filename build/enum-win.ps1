param([int]$ProcId = 8440)
Add-Type @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class WE {
  public delegate bool CB(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(CB cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  public struct R { public int L, T, Rt, B; }
}
'@
$wins = New-Object System.Collections.Generic.List[string]
$cb = {
  param($h, $l)
  $wpid = 0
  [WE]::GetWindowThreadProcessId($h, [ref]$wpid) | Out-Null
  if ($wpid -eq $ProcId) {
    $sb = New-Object System.Text.StringBuilder 256
    [WE]::GetWindowText($h, $sb, 256) | Out-Null
    $cn = New-Object System.Text.StringBuilder 256
    [WE]::GetClassName($h, $cn, 256) | Out-Null
    $r = New-Object WE+R
    [WE]::GetWindowRect($h, [ref]$r) | Out-Null
    $vis = [WE]::IsWindowVisible($h)
    $wins.Add("$h | vis=$vis | class=$($cn.ToString()) | title='$($sb.ToString())' | rect=$($r.L),$($r.T),$($r.Rt),$($r.B)")
  }
  return $true
}
[WE]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
$wins
