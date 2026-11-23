package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"time"
)

type object map[string]any

func str(value object, key string) string {
	text, _ := value[key].(string)
	return text
}

func boolean(value object, key string) bool {
	result, _ := value[key].(bool)
	return result
}

func number(value object, key string) int {
	result, _ := value[key].(float64)
	return int(result)
}

var proxyPort = 17891
var running *exec.Cmd
var finished chan struct{}
var configFile string
var packageBase string

func packageDir() string {
	if packageBase != "" {
		return packageBase
	}
	executable, _ := os.Executable()
	return filepath.Dir(executable)
}

func rootDir() string {
	return filepath.Join(os.Getenv("LOCALAPPDATA"), "Torrent", "VPN")
}

func connected() bool {
	if running == nil {
		return false
	}
	select {
	case <-finished:
		return false
	default:
		return true
	}
}

func disconnect() {
	if running != nil && connected() {
		_ = running.Process.Kill()
		<-finished
	}
	running = nil
	if configFile != "" {
		_ = os.Remove(configFile)
		configFile = ""
	}
}

func resolveEngine(id string) (string, error) {
	manifest, err := os.ReadFile(filepath.Join(packageDir(), "components.json"))
	if err != nil {
		return "", err
	}
	var components map[string]struct {
		SHA256 string `json:"sha256"`
	}
	if json.Unmarshal(manifest, &components) != nil || len(components[id].SHA256) != 64 {
		return "", errors.New("invalid VPN component manifest")
	}
	path := filepath.Join(packageDir(), id, id+".exe")
	binary, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	digest := sha256.Sum256(binary)
	if hex.EncodeToString(digest[:]) != components[id].SHA256 {
		return "", errors.New("VPN component checksum mismatch")
	}
	return path, nil
}

func connect(link string) error {
	configuration, engine, err := engineConfig(link, proxyPort)
	if err != nil {
		return err
	}
	executable, err := resolveEngine(engine)
	if err != nil {
		return err
	}
	if previous, dialError := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", proxyPort), 200*time.Millisecond); dialError == nil {
		previous.Close()
		return errors.New("the local Torrent VPN port is already in use")
	}
	if err = os.MkdirAll(rootDir(), 0700); err != nil {
		return err
	}
	file, err := os.CreateTemp(rootDir(), "connection-*.json")
	if err != nil {
		return err
	}
	configFile = file.Name()
	_ = file.Chmod(0600)
	err = json.NewEncoder(file).Encode(configuration)
	_ = file.Close()
	if err != nil {
		return err
	}
	arguments := []string{"run", "-config", configFile}
	if engine == "sudoku" {
		arguments = []string{"-c", configFile}
	}
	command := exec.Command(executable, arguments...)
	hideProcess(command)
	command.Dir = filepath.Dir(executable)
	log, err := os.OpenFile(filepath.Join(rootDir(), "engine.log"), os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0600)
	if err != nil {
		return err
	}
	command.Stdout = log
	command.Stderr = log
	if err = command.Start(); err != nil {
		_ = log.Close()
		return err
	}
	running = command
	finished = make(chan struct{})
	completed := finished
	go func() {
		_ = command.Wait()
		_ = log.Close()
		close(completed)
	}()
	deadline := time.Now().Add(12 * time.Second)
	for time.Now().Before(deadline) {
		if !connected() {
			disconnect()
			return errors.New("VPN core stopped before its SOCKS listener became ready")
		}
		connection, dialError := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", proxyPort), 250*time.Millisecond)
		if dialError == nil {
			_ = connection.SetDeadline(time.Now().Add(time.Second))
			_, _ = connection.Write([]byte{5, 1, 0})
			reply := make([]byte, 2)
			_, dialError = io.ReadFull(connection, reply)
			_ = connection.Close()
			if dialError == nil && reply[0] == 5 && reply[1] == 0 {
				return nil
			}
		}
		time.Sleep(100 * time.Millisecond)
	}
	disconnect()
	return errors.New("VPN SOCKS listener did not start")
}

func writeStatus(path, value string) {
	temporary := path + ".tmp"
	if os.WriteFile(temporary, []byte(value), 0600) == nil {
		_ = replaceFile(temporary, path)
	}
}

func run() error {
	if len(os.Args) != 5 || os.Args[1] != "--run" {
		return errors.New("usage: TorrentVpnHost --run KEY_FILE STATUS_FILE PORT")
	}
	port, err := strconv.Atoi(os.Args[4])
	if err != nil || port < 1024 || port > 65535 {
		return errors.New("invalid local SOCKS port")
	}
	proxyPort = port
	key, err := os.ReadFile(os.Args[2])
	_ = os.Remove(os.Args[2])
	if err != nil {
		return err
	}
	defer disconnect()
	if err = connect(string(key)); err != nil {
		writeStatus(os.Args[3], "error:"+err.Error())
		return err
	}
	writeStatus(os.Args[3], "ready")
	_, _ = io.Copy(io.Discard, os.Stdin)
	return nil
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
