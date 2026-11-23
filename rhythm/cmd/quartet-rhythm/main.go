// Command quartet-rhythm is Quartet's conductor: it drives the beat loop and
// serves the public REST API.
//
// Configuration (environment):
//
//	QUARTET_RHYTHM_ADDR   HTTP listen address  (default 127.0.0.1:8080)
//	QUARTET_HARMONY_ADDR  harmony gRPC target  (default 127.0.0.1:50051)
//	QUARTET_MELODY_ADDR   melody gRPC target   (default 127.0.0.1:50052)
package main

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	"github.com/ericgtkb/quartet/rhythm/internal/api"
	"github.com/ericgtkb/quartet/rhythm/internal/performance"
	"github.com/ericgtkb/quartet/rhythm/internal/upstream"
)

type config struct {
	httpAddr    string
	harmonyAddr string
	melodyAddr  string
}

func main() {
	log := slog.New(slog.NewTextHandler(os.Stderr, nil))
	if err := run(log); err != nil {
		log.Error("quartet-rhythm failed", "err", err)
		os.Exit(1)
	}
}

func run(log *slog.Logger) error {
	cfg, err := loadConfig()
	if err != nil {
		return err
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	harmonyConn, err := dial(cfg.harmonyAddr)
	if err != nil {
		return fmt.Errorf("harmony client: %w", err)
	}
	defer func() { _ = harmonyConn.Close() }()
	melodyConn, err := dial(cfg.melodyAddr)
	if err != nil {
		return fmt.Errorf("melody client: %w", err)
	}
	defer func() { _ = melodyConn.Close() }()

	harmony := upstream.NewHarmony(harmonyConn)
	melody := upstream.NewMelody(melodyConn)
	perfs := performance.NewManager(ctx, harmony, melody, log)

	// Handlers see this context end on shutdown, so long-lived streams return
	// and Shutdown can finish.
	baseCtx, cancelBase := context.WithCancel(context.Background())
	defer cancelBase()
	srv := &http.Server{
		Addr:              cfg.httpAddr,
		Handler:           api.NewRouter(log, []api.Upstream{harmony, melody}, perfs, melody),
		ReadHeaderTimeout: 5 * time.Second,
		BaseContext:       func(net.Listener) context.Context { return baseCtx },
	}
	srv.RegisterOnShutdown(cancelBase)

	serveErr := make(chan error, 1)
	go func() {
		log.Info("quartet-rhythm listening", "addr", cfg.httpAddr,
			"harmony", cfg.harmonyAddr, "melody", cfg.melodyAddr)
		serveErr <- srv.ListenAndServe()
	}()

	select {
	case err := <-serveErr:
		return fmt.Errorf("http server: %w", err)
	case <-ctx.Done():
	}

	log.Info("shutting down")
	shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	// Stop beat loops and delete their worlds while harmony is still reachable.
	perfs.Close(shutdownCtx)
	if err := srv.Shutdown(shutdownCtx); err != nil {
		return fmt.Errorf("http shutdown: %w", err)
	}
	if err := <-serveErr; !errors.Is(err, http.ErrServerClosed) {
		return fmt.Errorf("http server: %w", err)
	}
	return nil
}

// dial creates a lazy gRPC client for a host:port target; connection errors
// surface on the first call.
func dial(target string) (*grpc.ClientConn, error) {
	return grpc.NewClient(target, grpc.WithTransportCredentials(insecure.NewCredentials()))
}

func loadConfig() (config, error) {
	cfg := config{
		httpAddr:    env("QUARTET_RHYTHM_ADDR", "127.0.0.1:8080"),
		harmonyAddr: env("QUARTET_HARMONY_ADDR", "127.0.0.1:50051"),
		melodyAddr:  env("QUARTET_MELODY_ADDR", "127.0.0.1:50052"),
	}
	if _, _, err := net.SplitHostPort(cfg.httpAddr); err != nil {
		return config{}, fmt.Errorf("invalid QUARTET_RHYTHM_ADDR %q: %w", cfg.httpAddr, err)
	}
	return cfg, nil
}

func env(key, fallback string) string {
	if v, ok := os.LookupEnv(key); ok && v != "" {
		return v
	}
	return fallback
}
