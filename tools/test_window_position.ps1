param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][string]$WorkingFolder)
$ErrorActionPreference='Stop'
Add-Type -ReferencedAssemblies System.Drawing @"
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Drawing;
using System.Drawing.Imaging;
public static class TorrentUI {
 public delegate bool Callback(IntPtr w,IntPtr context);
 [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb,IntPtr context);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr w);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr w,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr w,StringBuilder text,int size);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr w,StringBuilder text,int size);
 [DllImport("user32.dll",EntryPoint="SendMessageW",CharSet=CharSet.Unicode)] public static extern IntPtr SendText(IntPtr w,uint msg,IntPtr wp,string text);
 public static bool SetWindowText(IntPtr w,string text){return SendText(w,0xC,IntPtr.Zero,text)!=IntPtr.Zero;}
 [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr w,int id);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr w,uint msg,IntPtr wp,IntPtr lp);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr w,uint msg,IntPtr wp,IntPtr lp);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr w);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr w,IntPtr dc,uint flags);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr w,out Rect r);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr w,out Rect r);
 [DllImport("user32.dll",EntryPoint="GetWindowLongPtrW")] public static extern IntPtr Style(IntPtr w,int index);
 [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr w);
 [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr w);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr w,int command);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr w,IntPtr after,int x,int y,int cx,int cy,uint flags);
 public static int Width(IntPtr w){Rect r;GetClientRect(w,out r);return (int)Math.Round(r.right*96.0/GetDpiForWindow(w));}
 public static void ShellCheck(IntPtr w){long style=Style(w,-16).ToInt64();if((style&0xC10000)!=0)throw new Exception("System caption or maximize box remains");Rect r;GetWindowRect(w,out r);double scale=GetDpiForWindow(w)/96.0;if(Math.Abs((r.right-r.left)/scale-840)>1||Math.Abs((r.bottom-r.top)/scale-610)>1)throw new Exception("Outer size changed");int x=r.left+(int)(250*scale),y=r.top+(int)(30*scale);if(SendMessage(w,0x84,IntPtr.Zero,new IntPtr((y<<16)|(x&65535))).ToInt32()!=2)throw new Exception("Custom header is not draggable");SendMessage(w,0x112,new IntPtr(0xF030),IntPtr.Zero);if(IsZoomed(w))throw new Exception("Unexpected maximize");}
 public struct Rect {public int left,top,right,bottom;}
 public static IntPtr Find(int process,string cls){IntPtr result=IntPtr.Zero;EnumWindows((w,c)=>{uint id;GetWindowThreadProcessId(w,out id);var text=new StringBuilder(256);GetClassName(w,text,256);if(id==process&&text.ToString()==cls&&IsWindowVisible(w))result=w;return true;},IntPtr.Zero);return result;}
 public static string Caption(IntPtr w){var text=new StringBuilder(256);GetWindowText(w,text,256);return text.ToString();}
 public static void Click(IntPtr w,int x,int y){double scale=GetDpiForWindow(w)/96.0;PostMessage(w,0x202,IntPtr.Zero,new IntPtr(((int)(y*scale)<<16)|(int)(x*scale)));}
 public static void Save(IntPtr w,string path){Rect r;GetClientRect(w,out r);using(var bitmap=new Bitmap(r.right,r.bottom)){using(var g=Graphics.FromImage(bitmap)){var dc=g.GetHdc();bool ok=PrintWindow(w,dc,3);g.ReleaseHdc(dc);if(!ok)throw new Exception("Capture failed");}bitmap.Save(path,ImageFormat.Png);}}
 public static void CheckBackground(string path,int rgb){using(var bitmap=new Bitmap(path)){if((bitmap.GetPixel(1,1).ToArgb()&0xFFFFFF)!=rgb)throw new Exception("Unexpected theme background: "+path);}}
 public static void CheckPixel(IntPtr w,string path,int x,int y,int rgb){double scale=GetDpiForWindow(w)/96.0;using(var bitmap=new Bitmap(path)){if((bitmap.GetPixel((int)(x*scale),(int)(y*scale)).ToArgb()&0xFFFFFF)!=rgb)throw new Exception("Unexpected header tile/focus pixel: "+path);}}
}
"@
$exe=(Resolve-Path -LiteralPath $Executable).Path
$folder=(New-Item -ItemType Directory -Force $WorkingFolder).FullName
$position=Join-Path $folder 'ui-test-data/window-position.txt'
function Launch {
 $script:process=Start-Process $exe -ArgumentList '--interaction-test' -WorkingDirectory $folder -PassThru -WindowStyle Hidden
 foreach($class in @('TorrentMain','TorrentDialog')) {
  $w=[IntPtr]::Zero
  for($i=0;$i -lt 150;$i++){ $w=[TorrentUI]::Find($process.Id,$class);if($w -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 50 }
  if($w -eq [IntPtr]::Zero){throw "Missing $class"}
  if($class -eq 'TorrentMain'){$script:main=$w}else{[TorrentUI]::SendMessage($w,0x10,[IntPtr]::Zero,[IntPtr]::Zero)|Out-Null}
 }
 [TorrentUI]::ShowWindow($main,9)|Out-Null
 [TorrentUI]::ShellCheck($main)
}
function Stop-App {
 [TorrentUI]::PostMessage($main,0x10,[IntPtr]::Zero,[IntPtr]::Zero)|Out-Null
 if(-not $process.WaitForExit(15000)){throw 'Shutdown timed out'}
}
function Assert-Position([int]$x,[int]$y) {
 $r=New-Object TorrentUI+Rect
 [TorrentUI]::GetWindowRect($main,[ref]$r)|Out-Null
 if($r.left -ne $x -or $r.top -ne $y){throw "Wrong restored position $($r.left),$($r.top), expected $x,$y"}
 if([TorrentUI]::IsIconic($main)){throw 'Restored minimized window'}
}
try {
 Launch
 [TorrentUI]::SetWindowPos($main,[IntPtr]::Zero,60,80,0,0,0x15)|Out-Null
 for($i=0;$i -lt 60;$i++){
  Start-Sleep -Milliseconds 50
  if((Test-Path $position) -and ((Get-Content $position -Raw) -match '^1 60 80 ')){break}
 }
 if((Get-Content $position -Raw) -notmatch '^1 60 80 '){throw 'Move was not persisted before exit'}
 $process.Kill();$process.WaitForExit()
 Launch;Assert-Position 60 80
 [TorrentUI]::ShowWindow($main,6)|Out-Null
 Stop-App
 Launch;Assert-Position 60 80
 [TorrentUI]::SetWindowPos($main,[IntPtr]::Zero,90,100,0,0,0x15)|Out-Null
 [TorrentUI]::SendMessage($main,0x11,[IntPtr]::Zero,[IntPtr]::Zero)|Out-Null
 if((Get-Content $position -Raw) -notmatch '^1 90 100 '){throw 'Session-end notification did not persist position'}
 Stop-App
 Launch;Assert-Position 90 100;Stop-App
 [IO.File]::WriteAllText($position,'1 999999 999999 999000 999000 1000000 1000000')
 Launch
 $r=New-Object TorrentUI+Rect;[TorrentUI]::GetWindowRect($main,[ref]$r)|Out-Null
 if($r.left -gt 100000 -or $r.top -gt 100000){throw 'Disconnected monitor position was not corrected'}
 Stop-App
 [IO.File]::WriteAllText($position,'damaged data')
 Launch;Stop-App
 Write-Output 'PASS: debounced persistence, forced restart, minimized close, session end, missing monitor, malformed file, unchanged dimensions.'
} finally {if($process -and -not $process.HasExited){$process.Kill();$process.WaitForExit()}}
