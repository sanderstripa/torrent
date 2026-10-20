param([Parameter(Mandatory=$true)][string[]]$Executables)
$ErrorActionPreference='Stop'
Add-Type -ReferencedAssemblies System.Drawing @'
using System;
using System.IO;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Collections.Generic;
public static class ProductIconAudit {
 delegate bool ResourceCallback(IntPtr module,IntPtr type,IntPtr name,IntPtr parameter);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr LoadLibraryEx(string file,IntPtr reserved,uint flags);
 [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr module);
 [DllImport("kernel32.dll")] static extern bool EnumResourceNames(IntPtr module,IntPtr type,ResourceCallback callback,IntPtr parameter);
 [DllImport("kernel32.dll")] static extern IntPtr FindResource(IntPtr module,IntPtr name,IntPtr type);
 [DllImport("kernel32.dll")] static extern IntPtr LoadResource(IntPtr module,IntPtr resource);
 [DllImport("kernel32.dll")] static extern IntPtr LockResource(IntPtr resource);
 [DllImport("kernel32.dll")] static extern uint SizeofResource(IntPtr module,IntPtr resource);
 static byte[] Read(IntPtr module,IntPtr name,int type){var r=FindResource(module,name,(IntPtr)type);if(r==IntPtr.Zero)throw new Exception("Missing icon resource");var data=new byte[SizeofResource(module,r)];Marshal.Copy(LockResource(LoadResource(module,r)),data,0,data.Length);return data;}
 public static void Check(string path){
  var module=LoadLibraryEx(Path.GetFullPath(path),IntPtr.Zero,2);if(module==IntPtr.Zero)throw new Exception("Cannot read executable resources");
  try{
   int groups=0;var sizes=new HashSet<int>();
   ResourceCallback callback=delegate(IntPtr m,IntPtr type,IntPtr name,IntPtr ignored){
    groups++;var group=Read(m,name,14);int count=BitConverter.ToUInt16(group,4);
    for(int i=0;i<count;i++){
     int offset=6+i*14,size=group[offset]==0?256:group[offset];
     var image=Read(m,(IntPtr)BitConverter.ToUInt16(group,offset+12),3);
     using(var stream=new MemoryStream())using(var writer=new BinaryWriter(stream)){
      writer.Write((ushort)0);writer.Write((ushort)1);writer.Write((ushort)1);
      writer.Write(group,offset,8);writer.Write(image.Length);writer.Write(22);writer.Write(image);stream.Position=0;
      using(var icon=new Icon(stream))using(var bitmap=icon.ToBitmap()){
       var center=bitmap.GetPixel(bitmap.Width/2,bitmap.Height/2);
       if(center.A<240||center.R<=center.B+15)throw new Exception("Old or incorrect icon center: "+path+" size "+size);
       if(bitmap.GetPixel(0,0).A>0)throw new Exception("Opaque outer icon tile: "+path);
       sizes.Add(size);
      }
     }
    }
    return true;
   };
   EnumResourceNames(module,(IntPtr)14,callback,IntPtr.Zero);
   foreach(int size in new[]{16,20,24,32,40,48,64,96,128,256})if(!sizes.Contains(size))throw new Exception("Missing size "+size+" in "+path);
   if(groups==0)throw new Exception("No product icon in "+path);
   Console.WriteLine("PASS: "+Path.GetFileName(path)+"; all ten reference icon sizes, filled center, transparent exterior");
  }finally{FreeLibrary(module);}
 }
}
'@
foreach($file in $Executables){[ProductIconAudit]::Check($file)}
