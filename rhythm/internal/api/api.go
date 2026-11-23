// Package api serves rhythm's public REST API.
package api

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
	"github.com/ericgtkb/quartet/rhythm/internal/performance"
)

// Version is rhythm's component version.
const Version = "0.1.0"

const (
	// upstreamTimeout bounds every call rhythm makes to an internal service.
	upstreamTimeout = 2 * time.Second
	// requestTimeout bounds non-streaming requests. Streaming routes (SSE)
	// must not use it.
	requestTimeout = 10 * time.Second
)

// Upstream is an internal service rhythm depends on.
type Upstream interface {
	Name() string
	Info(ctx context.Context) (*score.ServiceInfo, error)
	Health(ctx context.Context) error
}

// NewRouter builds the public HTTP handler. trainer may be nil, which
// disables the training routes (503).
func NewRouter(log *slog.Logger, upstreams []Upstream, perfs *performance.Manager, trainer Trainer) http.Handler {
	h := &handlers{log: log, upstreams: upstreams, perfs: perfs, trainer: trainer}

	r := chi.NewRouter()
	r.Use(middleware.RequestID, middleware.Recoverer)
	r.NotFound(func(w http.ResponseWriter, _ *http.Request) {
		writeError(w, http.StatusNotFound, "not found")
	})
	r.MethodNotAllowed(func(w http.ResponseWriter, _ *http.Request) {
		writeError(w, http.StatusMethodNotAllowed, "method not allowed")
	})

	r.Route("/api", func(r chi.Router) {
		r.Group(func(r chi.Router) {
			r.Use(middleware.Timeout(requestTimeout))
			r.Get("/healthz", h.healthz)
			r.Get("/ensemble", h.ensemble)
			r.Get("/policies", h.policies)
			r.Get("/performances", h.listPerformances)
			r.Post("/performances", h.createPerformance)
			r.Get("/performances/{id}", h.getPerformance)
			r.Delete("/performances/{id}", h.deletePerformance)
			r.Get("/performances/{id}/census", h.census)
			r.Get("/training-runs", h.withTrainer(h.listTrainingRuns))
			r.Post("/training-runs", h.withTrainer(h.startTraining))
			r.Get("/training-runs/{id}", h.withTrainer(h.getTrainingRun))
			r.Post("/training-runs/{id}/stop", h.withTrainer(h.stopTraining))
		})
		// Streams run for as long as the client listens: no request timeout.
		r.Get("/performances/{id}/stream", h.streamPerformance)
		r.Get("/training-runs/{id}/stream", h.withTrainer(h.streamTraining))
	})
	return r
}

type handlers struct {
	log       *slog.Logger
	upstreams []Upstream
	perfs     *performance.Manager
	trainer   Trainer
}

// healthz reports rhythm's own liveness plus the gRPC health of every
// upstream. It returns 503 when any upstream is not serving.
func (h *handlers) healthz(w http.ResponseWriter, r *http.Request) {
	errs := fanOut(r.Context(), h.upstreams, func(ctx context.Context, _ int, u Upstream) error {
		return u.Health(ctx)
	})

	resp := HealthResponse{Status: StatusOK, Checks: map[string]string{}}
	code := http.StatusOK
	for i, u := range h.upstreams {
		resp.Checks[u.Name()] = StatusOK
		if errs[i] != nil {
			h.log.Warn("upstream unhealthy", "upstream", u.Name(), "err", errs[i])
			resp.Checks[u.Name()] = StatusUnavailable
			resp.Status = "degraded"
			code = http.StatusServiceUnavailable
		}
	}
	writeJSON(w, code, resp)
}

// ensemble describes rhythm and every upstream. An unreachable upstream is
// reported with status "unavailable" rather than failing the request.
func (h *handlers) ensemble(w http.ResponseWriter, r *http.Request) {
	infos := make([]*score.ServiceInfo, len(h.upstreams))
	errs := fanOut(r.Context(), h.upstreams, func(ctx context.Context, i int, u Upstream) error {
		info, err := u.Info(ctx)
		infos[i] = info // each goroutine writes only its own index
		return err
	})

	members := []Member{{
		Name:    "rhythm",
		Status:  StatusOK,
		Version: Version,
		Runtime: "Go " + strings.TrimPrefix(runtime.Version(), "go"),
		Details: map[string]string{},
	}}
	for i, u := range h.upstreams {
		if errs[i] != nil {
			h.log.Warn("upstream unavailable", "upstream", u.Name(), "err", errs[i])
			members = append(members, Member{
				Name: u.Name(), Status: StatusUnavailable, Details: map[string]string{}, Error: errs[i].Error(),
			})
			continue
		}
		details := infos[i].GetDetails()
		if details == nil {
			details = map[string]string{}
		}
		members = append(members, Member{
			Name:    u.Name(),
			Status:  StatusOK,
			Version: infos[i].GetVersion(),
			Runtime: infos[i].GetRuntime(),
			Details: details,
		})
	}
	writeJSON(w, http.StatusOK, EnsembleResponse{Members: members})
}

// fanOut calls fn for every upstream concurrently, each with its own deadline,
// and returns the errors in upstream order. fn receives the upstream's index.
func fanOut(ctx context.Context, ups []Upstream, fn func(context.Context, int, Upstream) error) []error {
	errs := make([]error, len(ups))
	var wg sync.WaitGroup
	for i, u := range ups {
		wg.Go(func() {
			cctx, cancel := context.WithTimeout(ctx, upstreamTimeout)
			defer cancel()
			errs[i] = fn(cctx, i, u)
		})
	}
	wg.Wait()
	return errs
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(v)
}
