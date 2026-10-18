package lsptestutil

import (
	stdjson "encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestSubprocessStateCloseDrainsResponse(t *testing.T) {
	// Use a short path to stay within macOS's Unix socket path limit.
	dir, err := os.MkdirTemp("/tmp", "tsrs-state-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	address := &net.UnixAddr{Name: filepath.Join(dir, "state.sock"), Net: "unix"}
	listener, err := net.ListenUnix("unix", address)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { listener.Close() })
	client, err := net.DialUnix("unix", nil, address)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { client.Close() })
	peer, err := listener.AcceptUnix()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { peer.Close() })
	deadline := time.Now().Add(5 * time.Second)
	if err := client.SetDeadline(deadline); err != nil {
		t.Fatal(err)
	}
	if err := peer.SetDeadline(deadline); err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() {
		defer peer.Close()
		_, err := io.WriteString(peer, "{}")
		if err == nil {
			// Hold back Rust's final newline until Go has begun shutdown. Decode
			// accepts the JSON object before this byte has reached the socket.
			var request [1]byte
			if n, readErr := peer.Read(request[:]); n != 0 || readErr != io.EOF {
				err = fmt.Errorf("expected request EOF, got %d bytes and %v", n, readErr)
			} else {
				_, err = io.WriteString(peer, "\n")
			}
		}
		done <- err
	}()
	var response struct{}
	if err := stdjson.NewDecoder(client).Decode(&response); err != nil {
		t.Fatal(err)
	}
	state := &subprocessClient{stateConn: client}
	if err := state.closeState(); err != nil {
		t.Fatal(err)
	}
	if err := <-done; err != nil {
		t.Fatalf("state response interrupted during shutdown: %v", err)
	}
}
