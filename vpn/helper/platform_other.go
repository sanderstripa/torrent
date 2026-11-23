//go:build !windows
package main
import("os";"os/exec")
func hideProcess(cmd *exec.Cmd){}
func replaceFile(from,to string)error{return os.Rename(from,to)}
