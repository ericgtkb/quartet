package api

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"runtime"
	"strings"
	"testing"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
)

type fakeUpstream struct {
	name string
	info *score.ServiceInfo
	err  error
}

func (f fakeUpstream) Name() string { return f.name }

func (f fakeUpstream) Info(context.Context) (*score.ServiceInfo, error) { return f.info, f.err }

func (f fakeUpstream) Health(context.Context) error { return f.err }

func serve(t *testing.T, ups []Upstream, path string) (int, string) {
	t.Helper()
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	rec := httptest.NewRecorder()
	NewRouter(log, ups, nil, nil).ServeHTTP(rec, httptest.NewRequest(http.MethodGet, path, nil))
	if ct := rec.Header().Get("Content-Type"); ct != "application/json" {
		t.Errorf("Content-Type = %q, want application/json", ct)
	}
	return rec.Code, strings.TrimSpace(rec.Body.String())
}

// assertJSON compares JSON documents structurally, pinning the wire shape.
func assertJSON(t *testing.T, got, want string) {
	t.Helper()
	var g, w any
	if err := json.Unmarshal([]byte(got), &g); err != nil {
		t.Fatalf("invalid JSON %q: %v", got, err)
	}
	if err := json.Unmarshal([]byte(want), &w); err != nil {
		t.Fatalf("invalid expected JSON: %v", err)
	}
	gb, _ := json.Marshal(g)
	wb, _ := json.Marshal(w)
	if string(gb) != string(wb) {
		t.Errorf("JSON mismatch\n got: %s\nwant: %s", gb, wb)
	}
}

var (
	harmonyOK = fakeUpstream{name: "harmony", info: &score.ServiceInfo{
		Name: "harmony", Version: "0.1.0", Runtime: "Rust", Details: map[string]string{"observationVersion": "0"},
	}}
	melodyDown = fakeUpstream{name: "melody", err: errors.New("connection refused")}
	melodyOK   = fakeUpstream{name: "melody", info: &score.ServiceInfo{Name: "melody", Version: "0.1.0", Runtime: "Python"}}
)

func TestEnsemble(t *testing.T) {
	code, body := serve(t, []Upstream{harmonyOK, melodyDown}, "/api/ensemble")
	if code != http.StatusOK {
		t.Fatalf("status = %d, want 200", code)
	}
	goVersion := strings.TrimPrefix(runtime.Version(), "go")
	assertJSON(t, body, `{"members": [
		{"name": "rhythm", "status": "ok", "version": "0.1.0", "runtime": "Go `+goVersion+`", "details": {}},
		{"name": "harmony", "status": "ok", "version": "0.1.0", "runtime": "Rust", "details": {"observationVersion": "0"}},
		{"name": "melody", "status": "unavailable", "details": {}, "error": "connection refused"}
	]}`)
}

func TestHealthz(t *testing.T) {
	code, body := serve(t, []Upstream{harmonyOK, melodyOK}, "/api/healthz")
	if code != http.StatusOK {
		t.Errorf("status = %d, want 200", code)
	}
	assertJSON(t, body, `{"status": "ok", "checks": {"harmony": "ok", "melody": "ok"}}`)

	code, body = serve(t, []Upstream{harmonyOK, melodyDown}, "/api/healthz")
	if code != http.StatusServiceUnavailable {
		t.Errorf("status = %d, want 503", code)
	}
	assertJSON(t, body, `{"status": "degraded", "checks": {"harmony": "ok", "melody": "unavailable"}}`)
}

func TestNotFound(t *testing.T) {
	code, body := serve(t, nil, "/api/nope")
	if code != http.StatusNotFound {
		t.Errorf("status = %d, want 404", code)
	}
	assertJSON(t, body, `{"error": "not found"}`)
}
