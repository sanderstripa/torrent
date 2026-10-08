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
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr w,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr w,StringBuilder text,int size);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr w,StringBuilder text,int size);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern bool SetWindowText(IntPtr w,string text);
 [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr w,int id);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr w,uint msg,IntPtr wp,IntPtr lp);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr w,uint msg,IntPtr wp,IntPtr lp);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr w);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr w,IntPtr dc,uint flags);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr w,out Rect r);
 public struct Rect {public int left,top,right,bottom;}
 public static IntPtr Find(int process,string cls){IntPtr result=IntPtr.Zero;EnumWindows((w,c)=>{uint id;GetWindowThreadProcessId(w,out id);var text=new StringBuilder(256);GetClassName(w,text,256);if(id==process&&text.ToString()==cls)result=w;return true;},IntPtr.Zero);return result;}
 public static string Caption(IntPtr w){var text=new StringBuilder(256);GetWindowText(w,text,256);return text.ToString();}
 public static void Click(IntPtr w,int x,int y){double scale=GetDpiForWindow(w)/96.0;PostMessage(w,0x202,IntPtr.Zero,new IntPtr(((int)(y*scale)<<16)|(int)(x*scale)));}
 public static void Save(IntPtr w,string path){Rect r;GetClientRect(w,out r);using(var bitmap=new Bitmap(r.right,r.bottom)){using(var g=Graphics.FromImage(bitmap)){var dc=g.GetHdc();bool ok=PrintWindow(w,dc,3);g.ReleaseHdc(dc);if(!ok)throw new Exception("Capture failed");}bitmap.Save(path,ImageFormat.Png);}}
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
 [TorrentUI]::SendMessage($menu,0x100,[IntPtr]0x28,[IntPtr]::Zero) | Out-Null
 [TorrentUI]::SendMessage($menu,0x100,[IntPtr]0x0D,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 200
}
try {
 $main=Wait-Window 'TorrentMain';$settings=Wait-Window 'TorrentDialog'
 $download=Join-Path $folder 'selected-downloads'
 [TorrentUI]::SetWindowText([TorrentUI]::GetDlgItem($settings,10),$download) | Out-Null
 Start-Sleep -Milliseconds 100
 $preferences=Join-Path $folder 'ui-test-data/settings.txt'
 if((Get-Content -LiteralPath $preferences)[0] -ne $download){throw 'Folder did not apply immediately'}
 [TorrentUI]::SetWindowText([TorrentUI]::GetDlgItem($settings,10),'relative-path') | Out-Null
 if((Get-Content -LiteralPath $preferences)[0] -ne $download){throw 'Invalid folder overwrote the valid setting'}
 [TorrentUI]::SetWindowText([TorrentUI]::GetDlgItem($settings,10),$download) | Out-Null
 Choose-Second $settings 237
 if((Get-Content -LiteralPath $preferences)[1] -ne '0 1 0'){throw 'Theme did not apply immediately'}
 Choose-Second $settings 291
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
 [TorrentUI]::PostMessage($settings,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 if((Get-Content -LiteralPath $preferences)[1] -ne '1 1 1'){throw 'Closing settings lost changes'}
 [TorrentUI]::PostMessage($main,0x8001,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 $settings=Wait-Window 'TorrentDialog'
 if([TorrentUI]::Caption($settings) -ne 'Settings'){throw 'Reopened settings lost language'}
 # Switch back to light through the same custom menu, using Up from selected Dark.
 [TorrentUI]::Click($settings,390,237);$menu=Wait-Window 'TorrentStyledPopup'
 [TorrentUI]::SendMessage($menu,0x100,[IntPtr]0x26,[IntPtr]::Zero) | Out-Null
 [TorrentUI]::SendMessage($menu,0x100,[IntPtr]0x0D,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::PostMessage($settings,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::PostMessage($main,0x8002,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 $files=Wait-Window 'TorrentDialog';Start-Sleep -Milliseconds 1100
 [TorrentUI]::Save($main,(Join-Path $folder 'pending-selection.png'))
 [TorrentUI]::PostMessage($files,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 Start-Sleep -Milliseconds 100
 [TorrentUI]::PostMessage($main,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
 if(-not $process.WaitForExit(10000)){throw 'UI test shutdown timed out'}
 if($process.ExitCode -ne 0){throw 'UI test exited with an error'}
 'PASS: immediate folder/action/theme/language, validation, custom menu keyboard selection/cancel, settings persistence, activation-frame capture and pending selection capture.'
} finally {if(-not $process.HasExited){$process.Kill();$process.WaitForExit()}}
