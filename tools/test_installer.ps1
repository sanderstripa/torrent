param([string]$Setup,[string]$Payload)
$ErrorActionPreference="Stop"
if(-not $env:GITHUB_ACTIONS){throw "Disposable GitHub runner only"}
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
$setupExe=(Resolve-Path $Setup).Path
$payloadExe=(Resolve-Path $Payload).Path
$install=Join-Path $env:LOCALAPPDATA 'Programs/Torrent'
$desktop=Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'Torrent.lnk'
$menu=Join-Path ([Environment]::GetFolderPath('Programs')) 'Torrent/Torrent.lnk'
function Wait-Exit($p){if(-not $p.WaitForExit(30000)){$p.Kill();throw 'Installer timeout'};if($p.ExitCode -ne 0){throw "Exit code $($p.ExitCode)"}}
function Check-Links([bool]$d,[bool]$m){if((Test-Path $desktop) -ne $d -or (Test-Path $menu) -ne $m){throw "Shortcut mismatch: desktop=$d menu=$m"}}
function Uninstall {
 $p=Start-Process (Join-Path $install 'Uninstall.exe') -ArgumentList '/S' -PassThru -WindowStyle Hidden
 Wait-Exit $p
 for($i=0;$i -lt 100 -and (Test-Path (Join-Path $install 'Torrent.exe'));$i++){Start-Sleep -Milliseconds 100}
 if(Test-Path (Join-Path $install 'Torrent.exe')){throw 'Uninstall did not remove payload'}
 Check-Links $false $false
}
# Verify every pair with the unchanged silent NSIS execution path.
foreach($d in @(0,1)){foreach($m in @(0,1)){
 $p=Start-Process $payloadExe -ArgumentList @('/S',"/DESKTOPSHORTCUT=$d","/STARTMENUSHORTCUT=$m") -PassThru -WindowStyle Hidden
 Wait-Exit $p;Check-Links ([bool]$d) ([bool]$m)
 $version=(Get-ItemProperty 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Torrent').DisplayVersion
 if($version -ne '0.2.4'){throw "Unexpected installed version $version"}
 Uninstall
}}
# Exercise the real shell: desktop enabled, Start menu disabled; choose in settings.
$p=Start-Process $setupExe -PassThru -WindowStyle Hidden
try {
 $w=[IntPtr]::Zero
 for($i=0;$i -lt 100;$i++){$w=[TorrentUI]::Find($p.Id,'TorrentSetupVisual');if($w -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100}
 if($w -eq [IntPtr]::Zero){throw 'Setup window missing'}
 [TorrentUI]::Click($w,130,365);Start-Sleep -Milliseconds 100
 [TorrentUI]::Click($w,60,290);Start-Sleep -Milliseconds 100
 [TorrentUI]::Click($w,150,350)
 for($i=0;$i -lt 300;$i++){if(Test-Path $desktop){break};Start-Sleep -Milliseconds 100}
 Check-Links $true $false
 Start-Sleep -Seconds 2
 # Disable launch before Done: do not start the user's torrent session in CI.
 [TorrentUI]::Click($w,290,370);[TorrentUI]::Click($w,420,310)
 Wait-Exit $p
 Uninstall
 'PASS: four shortcut combinations, version registration, complete native setup flow, launch disabled, uninstall cleanup.'
}finally{if(-not $p.HasExited){$p.Kill();$p.WaitForExit()}}
