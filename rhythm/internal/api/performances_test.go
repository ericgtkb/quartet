package api

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
	"github.com/ericgtkb/quartet/rhythm/internal/performance"
)

type stubEngine struct{ beat atomic.Uint64 }

func (s *stubEngine) snapshot() *score.WorldSnapshot {
	return &score.WorldSnapshot{
		Beat: s.beat.Load(), Width: 100, Height: 100,
		Quartetlings: []*score.Quartetling{{Id: 1, Voice: score.Voice_VOICE_SOPRANO, X: 1, Y: 2}},
		Stats:        &score.BeatStats{},
	}
}

func (s *stubEngine) CreateWorld(context.Context, *score.WorldConfig, bool) (*score.CreateWorldResponse, error) {
	return &score.CreateWorldResponse{WorldId: "w1", Snapshot: s.snapshot()}, nil
}

func (s *stubEngine) StepWorld(context.Context, *score.StepWorldRequest) (*score.StepWorldResponse, error) {
	s.beat.Add(1)
	return &score.StepWorldResponse{Snapshot: s.snapshot()}, nil
}

func (s *stubEngine) DeleteWorld(context.Context, string) error { return nil }

func (s *stubEngine) Census(context.Context, string) (*score.Census, error) {
	hist := func(lo, hi float32) *score.Histogram {
		return &score.Histogram{Min: lo, Max: hi, Bins: 2, Counts: []uint32{1, 0, 0, 1, 1, 1, 0, 0}}
	}
	return &score.Census{
		Beat: 20, Slots: 4,
		Voices: []*score.VoiceCensus{{
			Voice: score.Voice_VOICE_SOPRANO, Population: 1, MeanEnergy: 1.23456, MeanAge: 10,
			Tempo: &score.TraitStats{Mean: 1.6, Std: 0.1}, Pitch: &score.TraitStats{Mean: 18}, Dynamics: &score.TraitStats{Mean: 0.7},
			Singing: 1, InChord: 1,
		}},
		Tempo: hist(0.76, 1.92), Pitch: hist(9.2, 21.6), Dynamics: hist(0.56, 1.56), Energy: hist(0, 2),
		Density: &score.DensityGrid{Columns: 2, Rows: 1, Counts: []uint32{3, 1}, Singing: []uint32{1, 0}},
		History: []*score.CensusSample{
			{
				Beat: 10, Beats: 10, Deaths: 1, MotesEaten: 4, ChordMembers: 3, Singing: 5, MeanEnergy: []float32{1, 1.1, 1.2, 1.3},
				Births: 2, MaxGeneration: 1, Lineages: 4, MeanTempo: []float32{1.6, 1.4, 1.2, 1},
				MeanPitch: []float32{18, 15, 13, 11}, MeanDynamics: []float32{0.7, 0.9, 1.1, 1.3}, MeanGeneration: []float32{0.25, 0, 0, 0},
			},
			{Beat: 20, Beats: 10, MeanEnergy: []float32{0.9, 1, 1.1, 1.2}, Lineages: 3},
		},
		Lineages: 3,
	}, nil
}

func newServer(t *testing.T) *httptest.Server {
	return newServerWith(t, nil, nil)
}

func newServerWith(t *testing.T, actor performance.Actor, trainer Trainer) *httptest.Server {
	t.Helper()
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	perfs := performance.NewManager(t.Context(), &stubEngine{}, actor, log)
	srv := httptest.NewServer(NewRouter(log, nil, perfs, trainer))
	t.Cleanup(func() {
		perfs.Close(context.Background())
		srv.Close()
	})
	return srv
}

func do(t *testing.T, method, url, body string) (int, string) {
	t.Helper()
	req, err := http.NewRequestWithContext(t.Context(), method, url, strings.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = resp.Body.Close() }()
	b, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, strings.TrimSpace(string(b))
}

func TestPerformanceLifecycle(t *testing.T) {
	srv := newServer(t)

	code, body := do(t, http.MethodPost, srv.URL+"/api/performances", `{"policy":"choir","seed":5,"slots":8}`)
	if code != http.StatusCreated {
		t.Fatalf("create: %d %s", code, body)
	}
	var info map[string]any
	if err := json.Unmarshal([]byte(body), &info); err != nil {
		t.Fatal(err)
	}
	for key, want := range map[string]any{"id": "p1", "policy": "choir", "policyKind": "builtin", "evolve": true, "slots": 8.0, "seed": 5.0, "beatsPerSecond": 15.0, "status": "playing"} {
		if info[key] != want {
			t.Errorf("created %s = %v, want %v", key, info[key], want)
		}
	}

	code, body = do(t, http.MethodGet, srv.URL+"/api/performances", "")
	if code != http.StatusOK || !strings.Contains(body, `"id":"p1"`) {
		t.Errorf("list: %d %s", code, body)
	}

	// The stream starts with the latest frame.
	req, _ := http.NewRequestWithContext(t.Context(), http.MethodGet, srv.URL+"/api/performances/p1/stream", nil)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	if ct := resp.Header.Get("Content-Type"); ct != "text/event-stream" {
		t.Errorf("stream Content-Type = %q", ct)
	}
	lines := bufio.NewScanner(resp.Body)
	lines.Scan()
	if lines.Text() != "event: frame" {
		t.Fatalf("first line = %q", lines.Text())
	}
	lines.Scan()
	var frame performance.Frame
	if err := json.Unmarshal([]byte(strings.TrimPrefix(lines.Text(), "data: ")), &frame); err != nil {
		t.Fatalf("frame data: %v", err)
	}
	if len(frame.Quartetlings.ID) != 1 {
		t.Errorf("frame = %+v", frame)
	}

	// Deleting ends the stream with an "end" event.
	if code, body := do(t, http.MethodDelete, srv.URL+"/api/performances/p1", ""); code != http.StatusNoContent {
		t.Errorf("delete: %d %s", code, body)
	}
	sawEnd := false
	for lines.Scan() {
		if lines.Text() == "event: end" {
			sawEnd = true
		}
	}
	_ = resp.Body.Close()
	if !sawEnd {
		t.Error("stream ended without an end event")
	}

	if code, _ := do(t, http.MethodGet, srv.URL+"/api/performances/p1", ""); code != http.StatusNotFound {
		t.Errorf("get after delete: %d", code)
	}
}

func TestCensus(t *testing.T) {
	srv := newServer(t)
	if code, body := do(t, http.MethodPost, srv.URL+"/api/performances", `{"policy":"forage","slots":4}`); code != http.StatusCreated {
		t.Fatalf("create: %d %s", code, body)
	}
	code, body := do(t, http.MethodGet, srv.URL+"/api/performances/p1/census", "")
	if code != http.StatusOK {
		t.Fatalf("census: %d %s", code, body)
	}
	assertJSON(t, body, `{
		"beat": 20, "slots": 4,
		"voices": [{"voice": 0, "population": 1, "meanEnergy": 1.2346, "meanAge": 10,
			"tempo": {"mean": 1.6, "std": 0.1}, "pitch": {"mean": 18, "std": 0}, "dynamics": {"mean": 0.7, "std": 0},
			"singing": 1, "inChord": 1, "meanGeneration": 0, "maxGeneration": 0}],
		"histograms": {
			"tempo": {"min": 0.76, "max": 1.92, "counts": [[1, 0], [0, 1], [1, 1], [0, 0]]},
			"pitch": {"min": 9.2, "max": 21.6, "counts": [[1, 0], [0, 1], [1, 1], [0, 0]]},
			"dynamics": {"min": 0.56, "max": 1.56, "counts": [[1, 0], [0, 1], [1, 1], [0, 0]]},
			"energy": {"min": 0, "max": 2, "counts": [[1, 0], [0, 1], [1, 1], [0, 0]]}},
		"density": {"columns": 2, "rows": 1, "counts": [3, 1], "singing": [1, 0]},
		"history": {"beat": [10, 20], "beats": [10, 10], "deaths": [1, 0], "motesEaten": [4, 0],
			"chordMembers": [3, 0], "singing": [5, 0], "births": [2, 0], "maxGeneration": [1, 0], "lineages": [4, 3],
			"meanEnergy": [[1, 0.9], [1.1, 1], [1.2, 1.1], [1.3, 1.2]],
			"meanTempo": [[1.6, 0], [1.4, 0], [1.2, 0], [1, 0]],
			"meanPitch": [[18, 0], [15, 0], [13, 0], [11, 0]],
			"meanDynamics": [[0.7, 0], [0.9, 0], [1.1, 0], [1.3, 0]],
			"meanGeneration": [[0.25, 0], [0, 0], [0, 0], [0, 0]]},
		"lineages": 3
	}`)
	if code, _ := do(t, http.MethodGet, srv.URL+"/api/performances/p9/census", ""); code != http.StatusNotFound {
		t.Errorf("census of unknown performance: %d, want 404", code)
	}
}

func TestCreatePerformanceRejectsBadInput(t *testing.T) {
	srv := newServer(t)
	for _, body := range []string{
		`{"policy":"nope"}`,
		`{"policy":"choir","slots":6}`,
		`{"policy":"choir","extra":1}`,
		`not json`,
	} {
		code, resp := do(t, http.MethodPost, srv.URL+"/api/performances", body)
		if code != http.StatusBadRequest || !strings.Contains(resp, `"error"`) {
			t.Errorf("POST %s: %d %s, want 400 with error", body, code, resp)
		}
	}
}

func TestPolicies(t *testing.T) {
	srv := newServer(t)
	code, body := do(t, http.MethodGet, srv.URL+"/api/policies", "")
	if code != http.StatusOK {
		t.Fatalf("policies: %d", code)
	}
	var resp PoliciesResponse
	if err := json.Unmarshal([]byte(body), &resp); err != nil {
		t.Fatal(err)
	}
	if len(resp.Policies) != 3 || resp.Policies[0].Name != "random" {
		t.Errorf("policies = %+v", resp.Policies)
	}
}

type stubActor struct{ fail bool }

func (a stubActor) Act(_ context.Context, policy string, _ *score.Tensor, _ uint32) (*score.Tensor, error) {
	if policy != "ppo-1" {
		return nil, fmt.Errorf("melody: Act: %w", status.Error(codes.NotFound, "no trained policy"))
	}
	return &score.Tensor{}, nil
}

func (a stubActor) Policies(context.Context) ([]*score.PolicyInfo, error) {
	if a.fail {
		return nil, errors.New("melody down")
	}
	return []*score.PolicyInfo{{Name: "ppo-1", GlobalStep: 1_500_000, Status: score.TrainingStatus_TRAINING_STATUS_RUNNING}}, nil
}

func TestPoliciesIncludeCheckpoints(t *testing.T) {
	srv := newServerWith(t, stubActor{}, nil)
	_, body := do(t, http.MethodGet, srv.URL+"/api/policies", "")
	var resp map[string][]map[string]any
	if err := json.Unmarshal([]byte(body), &resp); err != nil {
		t.Fatal(err)
	}
	gb, _ := json.Marshal(resp["policies"][3])
	assertJSON(t, string(gb), `{"name": "ppo-1", "kind": "checkpoint",
		"description": "PPO policy trained for 1.5M agent-steps.", "globalStep": 1500000, "trainingStatus": "running"}`)

	code, body := do(t, http.MethodPost, srv.URL+"/api/performances", `{"policy":"ppo-1"}`)
	if code != http.StatusCreated || !strings.Contains(body, `"policyKind":"checkpoint"`) {
		t.Errorf("create with checkpoint: %d %s", code, body)
	}
	code, body = do(t, http.MethodPost, srv.URL+"/api/performances", `{"policy":"ppo-404"}`)
	if code != http.StatusBadRequest || !strings.Contains(body, "unknown policy") {
		t.Errorf("create with unknown checkpoint: %d %s", code, body)
	}

	// melody down: the builtins are still listed.
	srv = newServerWith(t, stubActor{fail: true}, nil)
	code, body = do(t, http.MethodGet, srv.URL+"/api/policies", "")
	if code != http.StatusOK || strings.Count(body, `"kind":"builtin"`) != 3 {
		t.Errorf("policies with melody down: %d %s", code, body)
	}
}
