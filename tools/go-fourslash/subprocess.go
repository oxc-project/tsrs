package lsptestutil

import (
	"context"
	stdjson "encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"sync"
	"testing"
	"time"

	"github.com/microsoft/TypeScript/tsc/internal/core"
	"github.com/microsoft/TypeScript/tsc/internal/json"
	"github.com/microsoft/TypeScript/tsc/internal/jsonrpc"
	"github.com/microsoft/TypeScript/tsc/internal/lsp"
	"github.com/microsoft/TypeScript/tsc/internal/lsp/lsproto"
	"github.com/microsoft/TypeScript/tsc/internal/vfs"
)

type subprocessClient struct {
	options chan *core.CompilerOptions
	ready   chan struct{}
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
	ctx, cancel := context.WithTimeout(t.Context(), 120*time.Second)
	t.Cleanup(cancel)
	cmd := exec.CommandContext(ctx, os.Getenv("TSRS_FOURSLASH_SERVER"), listener.Addr().String())
	cmd.WaitDelay = 5 * time.Second
	stderr, err := os.Create(filepath.Join(t.TempDir(), "server.stderr"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { stderr.Close() })
	cmd.Stderr = stderr
	stdin, err := cmd.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	// Own the read end: Cmd.Wait closes StdoutPipe before the router necessarily finishes draining it.
	stdout, stdoutWriter, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { stdout.Close() })
	t.Cleanup(func() { stdoutWriter.Close() })
	cmd.Stdout = stdoutWriter
	if err := cmd.Start(); err != nil {
		t.Fatalf("start Rust LSP server: %v", err)
	}
	stdoutWriter.Close()
	state := &subprocessClient{options: make(chan *core.CompilerOptions, 1), ready: make(chan struct{})}
	client := &LSPClient{
		inputWriter: lsp.ToWriter(stdin), outputReader: lsp.ToReader(stdout),
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
	processDone := make(chan error, 1)
	go func() { processDone <- cmd.Wait(); cancel() }()
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
	context.AfterFunc(ctx, func() { listener.Close() })
	var closeOnce sync.Once
	var closeErr error
	closed := false
	closeClient := func() error {
		closeOnce.Do(func() {
			closed = true
			stdin.Close()
			select {
			case closeErr = <-processDone:
			case <-time.After(5 * time.Second):
				cancel()
				closeErr = <-processDone
			}
			cancel()
			if err := <-routerDone; closeErr == nil {
				closeErr = err
			}
			if err := <-bridgeDone; closeErr == nil && err != nil && !errors.Is(err, io.EOF) && !errors.Is(err, net.ErrClosed) && !errors.Is(err, context.Canceled) {
				closeErr = err
			}
			if output, _ := os.ReadFile(stderr.Name()); closeErr != nil {
				closeErr = fmt.Errorf("Rust LSP subprocess: %w\n%s", closeErr, output)
			}
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

func (c *LSPClient) WaitForSubprocessInit(t *testing.T) {
	t.Helper()
	select {
	case <-c.subprocess.ready:
	case <-c.ctx.Done():
		t.Fatalf("Rust LSP server did not initialize: %v", c.ctx.Err())
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
			close(s.ready)
		default:
			result, err = callFS(opts.FS, method, args)
		}
		response := map[string]any{"result": result}
		if err != nil {
			response["error"] = err.Error()
		}
		if err := encoder.Encode(response); err != nil {
			return err
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
