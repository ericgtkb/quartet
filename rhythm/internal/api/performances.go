package api

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"

	"github.com/ericgtkb/quartet/rhythm/internal/performance"
)

const (
	maxBodyBytes = 1 << 20
	// sseHeartbeat keeps idle streams alive through proxies.
	sseHeartbeat = 15 * time.Second
)

// PerformancesResponse is the body of GET /api/performances.
type PerformancesResponse struct {
	Performances []performance.Info `json:"performances"`
}

// PoliciesResponse is the body of GET /api/policies.
type PoliciesResponse struct {
	Policies []performance.Policy `json:"policies"`
}

// policies lists builtins and melody's trained policies. If melody is
// unreachable, it still lists the builtins.
func (h *handlers) policies(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), upstreamTimeout)
	defer cancel()
	policies, err := h.perfs.Policies(ctx)
	if err != nil {
		h.log.Warn("trained policies unavailable", "err", err)
	}
	writeJSON(w, http.StatusOK, PoliciesResponse{Policies: policies})
}

func (h *handlers) listPerformances(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, PerformancesResponse{Performances: h.perfs.List()})
}

func (h *handlers) createPerformance(w http.ResponseWriter, r *http.Request) {
	var cfg performance.Config
	dec := json.NewDecoder(http.MaxBytesReader(w, r.Body, maxBodyBytes))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&cfg); err != nil {
		writeError(w, http.StatusBadRequest, fmt.Sprintf("invalid JSON body: %v", err))
		return
	}
	p, err := h.perfs.Create(r.Context(), cfg)
	var verr *performance.ValidationError
	switch {
	case errors.As(err, &verr):
		writeError(w, http.StatusBadRequest, verr.Error())
	case errors.Is(err, performance.ErrLimit):
		writeError(w, http.StatusTooManyRequests, err.Error())
	case err != nil:
		h.log.Error("create performance", "err", err)
		writeError(w, http.StatusBadGateway, err.Error())
	default:
		writeJSON(w, http.StatusCreated, p.Info())
	}
}

func (h *handlers) getPerformance(w http.ResponseWriter, r *http.Request) {
	p, ok := h.perfs.Get(chi.URLParam(r, "id"))
	if !ok {
		writeError(w, http.StatusNotFound, performance.ErrNotFound.Error())
		return
	}
	writeJSON(w, http.StatusOK, p.Info())
}

func (h *handlers) deletePerformance(w http.ResponseWriter, r *http.Request) {
	err := h.perfs.Delete(r.Context(), chi.URLParam(r, "id"))
	switch {
	case errors.Is(err, performance.ErrNotFound):
		writeError(w, http.StatusNotFound, err.Error())
	case err != nil:
		// The performance is stopped either way; only the engine cleanup failed.
		h.log.Warn("delete performance", "err", err)
		w.WriteHeader(http.StatusNoContent)
	default:
		w.WriteHeader(http.StatusNoContent)
	}
}

// streamPerformance sends one Server-Sent Event per beat ("frame", JSON
// performance.Frame) and an "end" event when the performance stops.
func (h *handlers) streamPerformance(w http.ResponseWriter, r *http.Request) {
	p, ok := h.perfs.Get(chi.URLParam(r, "id"))
	if !ok {
		writeError(w, http.StatusNotFound, performance.ErrNotFound.Error())
		return
	}
	rc := http.NewResponseController(w)
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("X-Accel-Buffering", "no")
	w.WriteHeader(http.StatusOK)
	if err := rc.Flush(); err != nil {
		return
	}

	frames, unsubscribe := p.Subscribe()
	defer unsubscribe()
	heartbeat := time.NewTicker(sseHeartbeat)
	defer heartbeat.Stop()
	for {
		var err error
		select {
		case <-r.Context().Done():
			return
		case frame, open := <-frames:
			if !open {
				_, _ = fmt.Fprintf(w, "event: end\ndata: {\"status\":%q}\n\n", p.Info().Status)
				_ = rc.Flush()
				return
			}
			_, err = fmt.Fprintf(w, "event: frame\ndata: %s\n\n", frame)
		case <-heartbeat.C:
			_, err = fmt.Fprint(w, ": heartbeat\n\n")
		}
		if err == nil {
			err = rc.Flush()
		}
		if err != nil {
			return // client went away
		}
	}
}

func writeError(w http.ResponseWriter, code int, msg string) {
	writeJSON(w, code, ErrorResponse{Error: msg})
}
