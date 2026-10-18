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
$folder=(Resolve-Path -LiteralPath $WorkingFolder).Path
$process=Start-Process $exe -ArgumentList '--interaction-test' -WorkingDirectory $folder -PassThru -WindowStyle Hidden
function Wait-Window([string]$Class) {
 for($i=0;$i -lt 100;$i++){ $w=[TorrentUI]::Find($process.Id,$Class);if($w -ne [IntPtr]::Zero){return $w};Start-Sleep -Milliseconds 50 }
 throw ('Missing window: '+$Class)
}
function Choose-Second([IntPtr]$Window,[int]$Y) {
 [TorrentUI]::Click($Window,390,$Y)
 $menu=Wait-Window 'TorrentStyledPopup'
 Start-Sleep -Milliseconds 100
 [TorrentUI]::SendMessage($menu,0x100,[IntPtr]0x23,[IntPtr]::Zero) | Out-Null
 [TorrentUI]::SendMessage($menu,0x100,[IntPtr]0x0D,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 200
}
try {
 $main=Wait-Window 'TorrentMain';$settings=Wait-Window 'TorrentDialog'
 [TorrentUI]::ShellCheck($main)
 $download=Join-Path $folder 'selected-downloads'
 if(-not [TorrentUI]::SetWindowText([TorrentUI]::GetDlgItem($settings,10),$download)){throw "Download folder edit was not ready"}
 Start-Sleep -Milliseconds 100
 $preferences=Join-Path $folder 'ui-test-data/settings.txt'
 if((Get-Content -LiteralPath $preferences)[0] -ne $download){throw ('Folder did not apply immediately: '+(Get-Content -LiteralPath $preferences -Raw)+'; expected '+$download)}
 [TorrentUI]::SetWindowText([TorrentUI]::GetDlgItem($settings,10),'relative-path') | Out-Null
 if((Get-Content -LiteralPath $preferences)[0] -ne $download){throw 'Invalid folder overwrote the valid setting'}
 [TorrentUI]::SetWindowText([TorrentUI]::GetDlgItem($settings,10),$download) | Out-Null
 [TorrentUI]::Click($settings,395,267);Start-Sleep -Milliseconds 200
 if((Get-Content -LiteralPath $preferences)[1] -ne '0 1 0'){throw ('Theme did not apply immediately: '+(Get-Content -LiteralPath $preferences -Raw))}
 [TorrentUI]::Click($settings,450,225);Start-Sleep -Milliseconds 200
 if([TorrentUI]::Caption($settings) -ne 'Settings'){throw 'Language did not update the open settings window'}
 Choose-Second $settings 183
 if((Get-Content -LiteralPath $preferences)[1] -ne '1 1 1'){throw 'Add action did not apply immediately'}
 [TorrentUI]::Click($settings,390,183)
 $menu=Wait-Window 'TorrentStyledPopup';Start-Sleep -Milliseconds 100
 [TorrentUI]::Save($menu,(Join-Path $folder 'menu-dark.png'))
 [TorrentUI]::SendMessage($menu,0x100,[IntPtr]0x1B,[IntPtr]::Zero) | Out-Null
 [TorrentUI]::SendMessage($settings,0x86,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 [TorrentUI]::SendMessage($settings,0x85,[IntPtr]1,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::Save($settings,(Join-Path $folder 'settings-dark-live.png'))
 [TorrentUI]::CheckBackground((Join-Path $folder 'settings-dark-live.png'),0x0C0C0C)
 [TorrentUI]::PostMessage($settings,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 if((Get-Content -LiteralPath $preferences)[1] -ne '1 1 1'){throw 'Closing settings lost changes'}
 [TorrentUI]::PostMessage($main,0x8001,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 $settings=Wait-Window 'TorrentDialog'
 if([TorrentUI]::Caption($settings) -ne 'Settings'){throw 'Reopened settings lost language'}
 # Select System directly through the new theme segment.
 [TorrentUI]::Click($settings,95,267);Start-Sleep -Milliseconds 100
 if((Get-Content -LiteralPath $preferences)[1] -ne '1 2 1'){throw 'System theme did not persist'}
 [TorrentUI]::SendMessage($main,0x8004,[IntPtr]1,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::Save($settings,(Join-Path $folder 'settings-system-dark.png'))
 [TorrentUI]::CheckBackground((Join-Path $folder 'settings-system-dark.png'),0x0C0C0C)
 [TorrentUI]::SendMessage($main,0x8004,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::Save($settings,(Join-Path $folder 'settings-system-light.png'))
 [TorrentUI]::CheckBackground((Join-Path $folder 'settings-system-light.png'),0xF5F5F4)
 [TorrentUI]::Click($settings,245,267)
 Start-Sleep -Milliseconds 100
 [TorrentUI]::PostMessage($settings,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 $mainWidth=[TorrentUI]::Width($main)
 [TorrentUI]::Click($main,($mainWidth-70),32);Start-Sleep -Milliseconds 100
 if(-not [TorrentUI]::IsIconic($main)){throw 'Custom minimize button did not minimize'}
 [TorrentUI]::ShowWindow($main,9) | Out-Null
 [TorrentUI]::Click($main,($mainWidth-110),32);$settings=Wait-Window 'TorrentDialog'
 [TorrentUI]::PostMessage($settings,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::Save($main,(Join-Path $folder 'header-after-settings.png'))
 [TorrentUI]::CheckPixel($main,(Join-Path $folder 'header-after-settings.png'),($mainWidth-110),14,0xF5F5F4)
 foreach($theme in @('dark','light')) {
  [TorrentUI]::Click($main,($mainWidth-110),32);$settings=Wait-Window 'TorrentDialog'
  $themeX=if($theme -eq 'dark'){395}else{245}
  [TorrentUI]::Click($settings,$themeX,267)
  [TorrentUI]::PostMessage($settings,0x10,[IntPtr]::Zero,[IntPtr]::Zero)|Out-Null
  Start-Sleep -Milliseconds 100
 foreach($offset in @(150,110,70,30)) {
  $scale=[TorrentUI]::GetDpiForWindow($main)/96.0
  [TorrentUI]::SendMessage($main,0x2A3,[IntPtr]::Zero,[IntPtr]::Zero)|Out-Null
  $before=Join-Path $folder 'header-rest.png';[TorrentUI]::Save($main,$before)
  [TorrentUI]::SendMessage($main,0x200,[IntPtr]::Zero,[IntPtr](([int](32*$scale) -shl 16) -bor [int](($mainWidth-$offset)*$scale))) | Out-Null
  $path=Join-Path $folder ('header-hover-'+$theme+'-'+$offset+'.png')
  [TorrentUI]::Save($main,$path)
  $background=if($theme -eq 'dark'){0x0C0C0C}else{0xF5F5F4}
  [TorrentUI]::CheckPixel($main,$path,($mainWidth-$offset),18,$background)
  $a=New-Object Drawing.Bitmap $before;$b=New-Object Drawing.Bitmap $path
  try {
   $changed=0
   for($x=[int](($mainWidth-$offset-14)*$scale);$x -lt [int](($mainWidth-$offset+14)*$scale);$x++){
    for($y=[int](20*$scale);$y -lt [int](44*$scale);$y++){
     if($a.GetPixel($x,$y).ToArgb() -ne $b.GetPixel($x,$y).ToArgb()){$changed++}
    }
   }
   if($changed -lt 5){throw "Header icon did not highlight: $theme $offset"}
  } finally {$a.Dispose();$b.Dispose()}
 }
 }
 [TorrentUI]::PostMessage($main,0x8005,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 $notice=Wait-Window 'TorrentDialog'
 if([TorrentUI]::Caption($notice) -ne 'Error'){throw 'Error notice is not localized'}
 [TorrentUI]::Save($notice,(Join-Path $folder 'notice.png'))
 [TorrentUI]::PostMessage($notice,0x100,[IntPtr]0x0D,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::PostMessage($main,0x8002,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 $files=Wait-Window 'TorrentDialog';Start-Sleep -Milliseconds 1100
 [TorrentUI]::Save($main,(Join-Path $folder 'pending-selection.png'))
 [TorrentUI]::PostMessage($files,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::Click($main,100,120);Start-Sleep -Milliseconds 100
 if([TorrentUI]::Find($process.Id,'TorrentDialog') -ne [IntPtr]::Zero){throw 'Single click opened files instead of selecting'}
 [TorrentUI]::Save($main,(Join-Path $folder 'selected-torrent.png'))
 $before=[TorrentUI]::SendMessage($main,0x8003,[IntPtr]1,[IntPtr]::Zero).ToInt32()
 [TorrentUI]::SendMessage($main,0x100,[IntPtr]0x20,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 150
 $after=[TorrentUI]::SendMessage($main,0x8003,[IntPtr]1,[IntPtr]::Zero).ToInt32()
 if($before -eq $after -or $before -lt 0){throw 'Space did not toggle the selected torrent'}
 [TorrentUI]::SendMessage($main,0x100,[IntPtr]0x20,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 150
 if([TorrentUI]::SendMessage($main,0x8003,[IntPtr]1,[IntPtr]::Zero).ToInt32() -ne $before){throw 'Second Space did not restore pause state'}
 $scale=[TorrentUI]::GetDpiForWindow($main)/96.0
 [TorrentUI]::PostMessage($main,0x203,[IntPtr]::Zero,[IntPtr]((( [int](120*$scale)) -shl 16) -bor [int](100*$scale))) | Out-Null
 $files=Wait-Window 'TorrentDialog'
 [TorrentUI]::PostMessage($files,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 New-Item -ItemType Directory -Path $download -Force | Out-Null
 $sentinel=Join-Path $download 'Before file selection'
 [IO.File]::WriteAllBytes($sentinel,[Text.Encoding]::ASCII.GetBytes(('T'*16384)))
 [TorrentUI]::SendMessage($main,0x100,[IntPtr]0x2E,[IntPtr]::Zero) | Out-Null
 if([TorrentUI]::SendMessage($main,0x8003,[IntPtr]::Zero,[IntPtr]::Zero).ToInt32() -ne 0){throw 'Delete did not remove selected torrent'}
 if(-not (Test-Path -LiteralPath $sentinel)){throw 'Delete removed download data'}
 [TorrentUI]::PostMessage($main,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 if(-not $process.WaitForExit(10000)){throw 'UI test shutdown timed out'}
 if($process.ExitCode -ne 0){throw 'UI test exited with an error'}
 # Restart with the existing settings file. This catches Windows sharing violations
 # that never occur in a fresh profile, even when settings are reopened in-process.
 $process=Start-Process $exe -ArgumentList '--interaction-test' -WorkingDirectory $folder -PassThru -WindowStyle Hidden
 $main=Wait-Window 'TorrentMain';$settings=Wait-Window 'TorrentDialog'
 $secondDownload=Join-Path $folder 'downloads-after-restart'
 [TorrentUI]::SetWindowText([TorrentUI]::GetDlgItem($settings,10),$secondDownload) | Out-Null
 Start-Sleep -Milliseconds 100
 if((Get-Content -LiteralPath $preferences)[0] -ne $secondDownload){throw 'Existing settings file could not be replaced after restart'}
 [TorrentUI]::Save($settings,(Join-Path $folder 'settings-after-restart.png'))
 [TorrentUI]::CheckBackground((Join-Path $folder 'settings-after-restart.png'),0xF5F5F4)
 [TorrentUI]::PostMessage($settings,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::PostMessage($main,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 if(-not $process.WaitForExit(10000) -or $process.ExitCode -ne 0){throw 'Restart test shutdown failed'}
 'PASS: immediate settings, persistence across restart, custom menus, single-click selection, double-click files, Space pause/resume, Delete from list, pending selection.'
} finally {if(-not $process.HasExited){$process.Kill();$process.WaitForExit()}}
