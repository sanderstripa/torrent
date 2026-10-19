param([string]$Setup,[string]$Payload)
$ErrorActionPreference="Stop"
if(-not $env:GITHUB_ACTIONS){throw "Disposable GitHub runner only"}
Add-Type -ReferencedAssemblies System.Drawing @"
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Drawing;
using System.Drawing.Imaging;
[ComImport,Guid("000214F9-0000-0000-C000-000000000046"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface UnicodeShellLink {
 [PreserveSig] int GetPath([Out,MarshalAs(UnmanagedType.LPWStr)]StringBuilder path,int count,IntPtr findData,uint flags);
}
public static class TorrentUI {
 public static string LinkTarget(string path){object link=Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("00021401-0000-0000-C000-000000000046")));try{((System.Runtime.InteropServices.ComTypes.IPersistFile)link).Load(path,0);var text=new StringBuilder(32768);int error=((UnicodeShellLink)link).GetPath(text,text.Capacity,IntPtr.Zero,0);Marshal.ThrowExceptionForHR(error);return text.ToString();}finally{Marshal.FinalReleaseComObject(link);}}

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
 public static IntPtr Find(int process,string cls){IntPtr result=IntPtr.Zero;EnumWindows((w,c)=>{uint id;GetWindowThreadProcessId(w,out id);var text=new StringBuilder(256);GetClassName(w,text,256);if(id==process&&text.ToString()==cls)result=w;return true;},IntPtr.Zero);return result;}
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
 if($version -ne '0.2.5'){throw "Unexpected installed version $version"}
 Uninstall
}}
# Real shell: custom Unicode/spaced path, retained preferences, both themes, actual post-install launch.
$profile=Join-Path $env:LOCALAPPDATA 'Torrent'
New-Item $profile -ItemType Directory -Force|Out-Null
$preferences=Join-Path $profile 'settings.txt'
$download=Join-Path $env:RUNNER_TEMP 'Kept Downloads'
foreach($theme in @('light','dark')) {
 $install=Join-Path $env:RUNNER_TEMP ('Torrent Custom Юникод '+$theme)
 [IO.File]::WriteAllText($preferences,$download+"`n1 2 1",[Text.UTF8Encoding]::new($false))
 $p=Start-Process $setupExe -PassThru -WindowStyle Hidden
 try {
  $w=[IntPtr]::Zero
  for($i=0;$i -lt 100;$i++){$w=[TorrentUI]::Find($p.Id,'TorrentSetupVisual');if($w -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100}
  if($w -eq [IntPtr]::Zero){throw 'Setup window missing'}
  [TorrentUI]::ShowWindow($w,4)|Out-Null
  Start-Sleep -Milliseconds 100
  [TorrentUI]::Click($w,130,365);Start-Sleep -Milliseconds 200
  $edit=[TorrentUI]::GetDlgItem($w,101)
  if($edit -eq [IntPtr]::Zero){throw 'Editable installation path missing'}
  if(-not [TorrentUI]::SetWindowText($edit,$install)){throw 'Could not enter custom installation path'}
  $x=if($theme -eq 'dark'){400}else{330}
  [TorrentUI]::Click($w,$x,130);Start-Sleep -Milliseconds 100
  [TorrentUI]::Click($w,60,290);Start-Sleep -Milliseconds 100
  [TorrentUI]::Click($w,150,350)
  $value=if($theme -eq 'dark'){1}else{0}
  for($i=0;$i -lt 300;$i++){
   if((Test-Path (Join-Path $install 'Torrent.exe')) -and (Test-Path $preferences) -and ((Get-Content $preferences)[1] -eq "1 $value 1")){break}
   Start-Sleep -Milliseconds 100
  }
  if(-not (Test-Path (Join-Path $install 'Torrent.exe'))){throw 'Custom directory was ignored'}
  if((Get-Content $preferences)[0] -ne $download -or (Get-Content $preferences)[1] -ne "1 $value 1"){throw 'Theme was not applied or other preferences were overwritten'}
  Check-Links $true $false
  $shortcutTarget=[TorrentUI]::LinkTarget($desktop)
  $expectedTarget=Join-Path $install 'Torrent.exe'
  if($shortcutTarget -ne $expectedTarget){throw ("Shortcut target mismatch: ["+$shortcutTarget+"] expected ["+$expectedTarget+"]")}
  $registration=Get-ItemProperty 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Torrent'
  if($registration.UninstallString -notlike ('*'+$install+'*')){throw 'Uninstall registration points to wrong directory'}
  Start-Sleep -Milliseconds 300
  [TorrentUI]::Click($w,420,310)
  Wait-Exit $p
  $app=$null;$main=[IntPtr]::Zero
  for($i=0;$i -lt 150;$i++){
   $app=Get-Process Torrent -ErrorAction SilentlyContinue|Where-Object Path -eq (Join-Path $install 'Torrent.exe')|Select-Object -First 1
   if($app){$main=[TorrentUI]::Find($app.Id,'TorrentMain');if($main -ne [IntPtr]::Zero){break}}
   Start-Sleep -Milliseconds 100
  }
  if($main -eq [IntPtr]::Zero){throw 'Post-install launch did not use custom directory'}
  $null=$app.Handle
  $app.EnableRaisingEvents=$true
  Start-Sleep -Milliseconds 300
  New-Item dist/installer-check -ItemType Directory -Force|Out-Null
  $capture=(Join-Path (Get-Location) "dist/installer-check/first-launch-$theme.png")
  [TorrentUI]::Save($main,$capture)
  $color=if($theme -eq 'dark'){0x0C0C0C}else{0xF5F5F4}
  [TorrentUI]::CheckBackground($capture,$color)
  [TorrentUI]::PostMessage($main,0x10,[IntPtr]::Zero,[IntPtr]::Zero)|Out-Null
  Wait-Exit $app
  Uninstall
 }finally{if(-not $p.HasExited){$p.Kill();$p.WaitForExit()};if($app -and -not $app.HasExited){$app.Kill();$app.WaitForExit()}}
}
'PASS: four shortcut combinations, custom Unicode/spaced paths, registration, preserved preferences, light/dark first launch, uninstall cleanup.'
