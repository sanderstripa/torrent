package main

import (
	"encoding/base64"
	"encoding/json"
	"strings"
	"testing"
)

func TestConnectionKeyValidation(t *testing.T) {
	validVLESS := "vless://12345678-1234-1234-1234-123456789abc@example.com:443?security=tls&type=ws&path=%2Fvpn"
	if _, engine, err := engineConfig(validVLESS, 17891); err != nil || engine != "xray" {
		t.Fatal("valid VLESS rejected", engine, err)
	}
	payload, _ := json.Marshal(object{"h": "example.com", "p": float64(443), "k": strings.Repeat("ab", 32), "e": "none", "hd": true})
	validSudoku := "sudoku://" + base64.RawURLEncoding.EncodeToString(payload)
	if _, engine, err := engineConfig(validSudoku, 17891); err != nil || engine != "sudoku" {
		t.Fatal("valid Sudoku key rejected", engine, err)
	}
	for _, invalid := range []string{"", "http://example.com", "vless://key@example.com:0", "sudoku://bad"} {
		if _, _, err := engineConfig(invalid, 17891); err == nil {
			t.Fatal("invalid key accepted", invalid)
		}
	}
}
