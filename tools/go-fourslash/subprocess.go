package lsptestutil

import (
	"context"
	stdjson "encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"slices"
	"sync"
	"testing"
	"time"
	"unicode/utf8"

	"github.com/microsoft/TypeScript/tsc/internal/core"
	"github.com/microsoft/TypeScript/tsc/internal/json"
	"github.com/microsoft/TypeScript/tsc/internal/jsonrpc"
	"github.com/microsoft/TypeScript/tsc/internal/lsp"
	"github.com/microsoft/TypeScript/tsc/internal/lsp/lsproto"
	"github.com/microsoft/TypeScript/tsc/internal/vfs"
)

type subprocessClient struct {
	options       chan *core.CompilerOptions
	ready         chan struct{}
	stateListener net.Listener
	stateEncoder  *stdjson.Encoder
	stateDecoder  *stdjson.Decoder
	stateConn     *net.UnixConn
}

func newSubprocessClient(t *testing.T, serverOpts lsp.ServerOptions, onServerRequest ServerRequestHandler) (*LSPClient, func() error) {
	t.Helper()
	// Keep Unix socket paths below macOS's 104-byte limit, even for long test names.
	dir, err := os.MkdirTemp("/tmp", "tsrs-fs-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	listener, err := net.Listen("unix", filepath.Join(dir, "vfs.sock"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { listener.Close() })
	stateListener, err := net.Listen("unix", filepath.Join(dir, "state.sock"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { stateListener.Close() })
	ctx, cancel := context.WithTimeout(t.Context(), 120*time.Second)
	t.Cleanup(cancel)
	lspListener, err := net.Listen("unix", filepath.Join(dir, "lsp.sock"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { lspListener.Close() })
	worker, err := acquireSubprocessWorker()
	if err != nil {
		t.Fatalf("start Rust LSP worker: %v", err)
	}
	released := false
	t.Cleanup(func() {
		if !released {
			worker.close(true)
		}
	})
	stopKill := context.AfterFunc(ctx, func() { worker.cmd.Process.Kill() })
	t.Cleanup(func() { stopKill() })
	context.AfterFunc(ctx, func() { lspListener.Close(); listener.Close(); stateListener.Close() })
	if err := worker.encoder.Encode(dir); err != nil {
		t.Fatal(err)
	}
	caseDone := make(chan error, 1)
	go func() {
		var done bool
		err := worker.decoder.Decode(&done)
		if err == nil && !done {
			err = fmt.Errorf("Rust worker did not finish the case")
		}
		caseDone <- err
		if err != nil {
			cancel()
		}
	}()
	conn, err := lspListener.Accept()
	if err != nil {
		output, _ := os.ReadFile(worker.stderr.Name())
		t.Fatalf("connect Rust LSP worker: %v\n%s", err, output)
	}
	lspConn := conn.(*net.UnixConn)
	t.Cleanup(func() { lspConn.Close() })
	context.AfterFunc(ctx, func() { lspConn.Close() })
	state := &subprocessClient{options: make(chan *core.CompilerOptions, 1), ready: make(chan struct{}), stateListener: stateListener}
	client := &LSPClient{
		inputWriter: lsp.ToWriter(lspConn), outputReader: lsp.ToReader(lspConn),
		pendingRequests: make(map[jsonrpc.ID]chan *lsproto.ResponseMessage),
		onServerRequest: onServerRequest, ctx: ctx, subprocess: state,
	}
	routerDone := make(chan error, 1)
	go func() {
		err := client.MessageRouter(ctx)
		routerDone <- err
		if err != nil {
			cancel()
		}
	}()
	bridgeDone := make(chan error, 1)
	go func() {
		conn, err := listener.Accept()
		if err == nil {
			stop := context.AfterFunc(ctx, func() { conn.Close() })
			defer stop()
			defer conn.Close()
			err = state.serveFS(ctx, conn, serverOpts)
		}
		bridgeDone <- err
	}()
	var closeOnce sync.Once
	var closeErr error
	closed := false
	closeClient := func() error {
		closeOnce.Do(func() {
			closed = true
			cleanupTimer := time.AfterFunc(5*time.Second, cancel)
			defer cleanupTimer.Stop()
			select {
			case <-state.ready:
				closeErr = client.shutdownSubprocess()
				if err := state.closeState(); closeErr == nil {
					closeErr = err
				}
			default:
			}
			lspConn.CloseWrite()
			if err := <-caseDone; closeErr == nil {
				closeErr = err
			}
			stopKill()
			if closeErr == nil {
				closeErr = ctx.Err()
			}
			// The worker has finished the server; wake the local socket reader as well.
			cancel()
			if err := <-routerDone; closeErr == nil {
				closeErr = err
			}
			if err := <-bridgeDone; closeErr == nil && err != nil && !errors.Is(err, io.EOF) && !errors.Is(err, net.ErrClosed) && !errors.Is(err, context.Canceled) {
				closeErr = err
			}
			if closeErr != nil {
				output, _ := os.ReadFile(worker.stderr.Name())
				worker.close(true)
				closeErr = fmt.Errorf("Rust LSP worker: %w\n%s", closeErr, output)
			} else {
				closeErr = worker.release()
			}
			released = true
		})
		return closeErr
	}
	t.Cleanup(func() {
		if closed {
			return
		}
		if err := closeClient(); err != nil {
			t.Error(err)
		}
	})
	return client, closeClient
}

func (s *subprocessClient) closeState() error {
	defer s.stateConn.Close()
	// Decode can finish before Rust writes the response's trailing newline.
	// Send request EOF, then drain the response so Rust can finish and close cleanly.
	if err := s.stateConn.CloseWrite(); err != nil {
		return err
	}
	_, err := io.Copy(io.Discard, s.stateConn)
	return err
}

func (c *LSPClient) WaitForSubprocessInit(t *testing.T) {
	t.Helper()
	select {
	case <-c.subprocess.ready:
	case <-c.ctx.Done():
		t.Fatalf("Rust LSP server did not initialize: %v", c.ctx.Err())
	}
}

func (c *LSPClient) shutdownSubprocess() error {
	id := c.NextID()
	reqID := lsproto.NewID(lsproto.IntegerOrString{Integer: &id})
	request := &lsproto.RequestMessage{ID: reqID, Method: lsproto.MethodShutdown}
	response := make(chan *lsproto.ResponseMessage, 1)
	c.pendingRequestsMu.Lock()
	c.pendingRequests[*reqID] = response
	c.pendingRequestsMu.Unlock()
	defer func() {
		c.pendingRequestsMu.Lock()
		delete(c.pendingRequests, *reqID)
		c.pendingRequestsMu.Unlock()
	}()
	if err := c.writeToServer(request.Message()); err != nil {
		return err
	}
	select {
	case <-c.ctx.Done():
		return c.ctx.Err()
	case reply := <-response:
		if reply == nil || reply.Error != nil {
			return fmt.Errorf("Rust LSP shutdown failed: %v", reply)
		}
		return nil
	}
}

// SubprocessState reads the snapshot after the preceding LSP request has completed.
// State inspection has its own connection so it cannot block filesystem callbacks.
func (c *LSPClient) SubprocessState(t *testing.T, openFiles []string, result any) {
	t.Helper()
	c.WaitForSubprocessInit(t)
	if err := c.subprocess.stateEncoder.Encode(openFiles); err != nil {
		t.Fatalf("request Rust project state: %v", err)
	}
	if err := c.subprocess.stateDecoder.Decode(result); err != nil {
		t.Fatalf("read Rust project state: %v", err)
	}
}

func (s *subprocessClient) serveFS(ctx context.Context, conn net.Conn, opts lsp.ServerOptions) error {
	decoder, encoder := stdjson.NewDecoder(conn), stdjson.NewEncoder(conn)
	for {
		var request [2]stdjson.RawMessage
		if err := decoder.Decode(&request); err != nil {
			return err
		}
		var method string
		var args []stdjson.RawMessage
		if err := stdjson.Unmarshal(request[0], &method); err != nil {
			return err
		}
		if err := stdjson.Unmarshal(request[1], &args); err != nil {
			return err
		}
		var result any
		var err error
		switch method {
		case "configure":
			var options *core.CompilerOptions
			select {
			case options = <-s.options:
			case <-ctx.Done():
				return ctx.Err()
			}
			encoded, marshalErr := json.Marshal(options)
			err = marshalErr
			result = map[string]any{
				"compilerOptions": stdjson.RawMessage(encoded), "cwd": opts.Cwd,
				"defaultLibraryPath": opts.DefaultLibraryPath, "caseSensitive": opts.FS.UseCaseSensitiveFileNames(),
			}
		case "ready":
			stateConn, acceptErr := s.stateListener.Accept()
			if acceptErr != nil {
				return acceptErr
			}
			defer stateConn.Close()
			stop := context.AfterFunc(ctx, func() { stateConn.Close() })
			defer stop()
			s.stateConn = stateConn.(*net.UnixConn)
			s.stateEncoder, s.stateDecoder = stdjson.NewEncoder(stateConn), stdjson.NewDecoder(stateConn)
			close(s.ready)
		default:
			result, err = callFS(opts.FS, method, args)
		}
		response := map[string]any{"result": result}
		// File contents dominate bridge traffic. Keep the JSON response small and
		// follow it with exactly this many UTF-8 bytes, without quoting a second copy.
		var contents string
		if method == "readFile" && err == nil && result != nil {
			contents = result.(string)
			if !utf8.ValidString(contents) {
				// Match encoding/json's replacement of each invalid UTF-8 code unit.
				contents = string([]rune(contents))
			}
			response["result"] = len(contents)
		}
		if err != nil {
			response["error"] = err.Error()
		}
		if err := encoder.Encode(response); err != nil {
			return err
		}
		if contents != "" {
			if _, err := io.WriteString(conn, contents); err != nil {
				return err
			}
		}
	}
}

func callFS(fs vfs.FS, method string, args []stdjson.RawMessage) (any, error) {
	if len(args) == 0 {
		return nil, fmt.Errorf("%s: missing path", method)
	}
	var path string
	if err := stdjson.Unmarshal(args[0], &path); err != nil {
		return nil, err
	}
	switch method {
	case "fileExists":
		return fs.FileExists(path), nil
	case "directoryExists":
		return fs.DirectoryExists(path), nil
	case "readFile":
		if text, ok := fs.ReadFile(path); ok {
			return text, nil
		}
		return nil, nil
	case "realpath":
		return fs.Realpath(path), nil
	case "entries":
		entries := fs.GetAccessibleEntries(path)
		var symlinks []string
		if entries.Symlinks != nil {
			symlinks = make([]string, 0, len(entries.Symlinks))
			for name := range entries.Symlinks {
				symlinks = append(symlinks, name)
			}
			slices.Sort(symlinks)
		}
		return map[string]any{"files": entries.Files, "directories": entries.Directories, "symlinks": symlinks}, nil
	case "stat":
		if info := fs.Stat(path); info != nil {
			return map[string]any{"name": info.Name(), "size": info.Size(), "mode": uint32(info.Mode()),
				"seconds": info.ModTime().Unix(), "nanos": info.ModTime().Nanosecond()}, nil
		}
		return nil, nil
	}
	var err error
	switch method {
	case "remove":
		err = fs.Remove(path)
	case "writeFile", "appendFile":
		var text string
		if len(args) != 2 {
			return nil, fmt.Errorf("%s: expected path and text", method)
		}
		if err := stdjson.Unmarshal(args[1], &text); err != nil {
			return nil, err
		}
		if method == "writeFile" {
			err = fs.WriteFile(path, text)
		} else {
			err = fs.AppendFile(path, text)
		}
	case "chtimes":
		var atime, mtime float64
		if len(args) != 3 {
			return nil, fmt.Errorf("chtimes: expected path and two timestamps")
		}
		if err := stdjson.Unmarshal(args[1], &atime); err != nil {
			return nil, err
		}
		if err := stdjson.Unmarshal(args[2], &mtime); err != nil {
			return nil, err
		}
		err = fs.Chtimes(path, time.Unix(int64(atime), int64((atime-float64(int64(atime)))*1e9)),
			time.Unix(int64(mtime), int64((mtime-float64(int64(mtime)))*1e9)))
	default:
		return nil, fmt.Errorf("unknown VFS operation %q", method)
	}
	if err != nil {
		return err.Error(), nil
	}
	return nil, nil
}
