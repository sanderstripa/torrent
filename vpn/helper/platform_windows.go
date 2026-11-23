package main
import("os/exec";"syscall";"unsafe")
func hideProcess(cmd *exec.Cmd){cmd.SysProcAttr=&syscall.SysProcAttr{HideWindow:true,CreationFlags:0x08000000}}
func replaceFile(from,to string)error{a,e:=syscall.UTF16PtrFromString(from);if e!=nil{return e};b,e:=syscall.UTF16PtrFromString(to);if e!=nil{return e};v,_,err:=syscall.NewLazyDLL("kernel32.dll").NewProc("MoveFileExW").Call(uintptr(unsafe.Pointer(a)),uintptr(unsafe.Pointer(b)),9);if v==0{return err};return nil}
