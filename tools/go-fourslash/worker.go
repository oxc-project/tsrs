package lsptestutil

import (
	stdjson "encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"sync"
	"time"
)

// Bound retained parse-cache entries and arena allocations. A lease always creates
// a fresh LSP server; only the process and immutable parsed files are reused.
const subprocessWorkerCases = 32

var subprocessWorkers = make(chan *subprocessWorker, 4)

type subprocessWorker struct {
	cmd       *exec.Cmd
	input     io.WriteCloser
	output    io.ReadCloser
	encoder   *stdjson.Encoder
	decoder   *stdjson.Decoder
	stderr    *os.File
	finished  chan struct{}
	err       error
	used      int
	closeOnce sync.Once
}

func acquireSubprocessWorker() (*subprocessWorker, error) {
	for {
		select {
		case worker := <-subprocessWorkers:
			select {
			case <-worker.finished:
				if err := worker.close(false); err != nil {
					return nil, fmt.Errorf("idle Rust LSP worker exited: %w", err)
				}
			default:
				return worker, nil
			}
		default:
			return startSubprocessWorker()
		}
	}
}

func startSubprocessWorker() (*subprocessWorker, error) {
	w := &subprocessWorker{cmd: exec.Command(os.Getenv("TSRS_FOURSLASH_SERVER"), "--worker"), finished: make(chan struct{})}
	var err error
	w.stderr, err = os.CreateTemp("/tmp", "tsrs-worker-*.stderr")
	if err != nil {
		return nil, err
	}
	w.cmd.Stderr = w.stderr
	w.input, err = w.cmd.StdinPipe()
	if err != nil {
		w.stderr.Close()
		os.Remove(w.stderr.Name())
		return nil, err
	}
	var writer *os.File
	w.output, writer, err = os.Pipe()
	if err == nil {
		w.cmd.Stdout = writer
		err = w.cmd.Start()
		writer.Close()
	}
	if err != nil {
		w.input.Close()
		if w.output != nil {
			w.output.Close()
		}
		w.stderr.Close()
		os.Remove(w.stderr.Name())
		return nil, err
	}
	w.encoder, w.decoder = stdjson.NewEncoder(w.input), stdjson.NewDecoder(w.output)
	go func() { w.err = w.cmd.Wait(); close(w.finished) }()
	return w, nil
}

func (w *subprocessWorker) close(kill bool) error {
	w.closeOnce.Do(func() {
		if kill {
			w.cmd.Process.Kill()
		}
		w.input.Close()
		select {
		case <-w.finished:
		case <-time.After(5 * time.Second):
			w.cmd.Process.Kill()
			<-w.finished
		}
		w.output.Close()
		w.stderr.Close()
		os.Remove(w.stderr.Name())
	})
	return w.err
}

func (w *subprocessWorker) release() error {
	w.used++
	if w.used < subprocessWorkerCases {
		select {
		case subprocessWorkers <- w:
			return nil
		default:
		}
	}
	return w.close(false)
}

// CloseSubprocessWorkers runs from the overlaid TestMain, after all case cleanups.
func CloseSubprocessWorkers() {
	for {
		select {
		case worker := <-subprocessWorkers:
			if err := worker.close(false); err != nil {
				fmt.Fprintln(os.Stderr, "close Rust LSP worker:", err)
				os.Exit(1)
			}
		default:
			return
		}
	}
}
