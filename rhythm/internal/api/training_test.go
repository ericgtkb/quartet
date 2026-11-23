package api

import (
	"bufio"
	"context"
	"io"
	"math"
	"net/http"
	"strings"
	"sync"
	"testing"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
	"github.com/ericgtkb/quartet/rhythm/internal/upstream"
)

// stubTrainer knows run "r1"; its stream sends two updates and ends.
type stubTrainer struct {
	mu      sync.Mutex
	started *score.TrainingConfig
	stopped bool
}

func (s *stubTrainer) run() *score.TrainingRun {
	s.mu.Lock()
	defer s.mu.Unlock()
	st := score.TrainingStatus_TRAINING_STATUS_RUNNING
	if s.stopped {
		st = score.TrainingStatus_TRAINING_STATUS_STOPPED
	}
	return &score.TrainingRun{
		Id: "r1", Status: st, CreatedAt: 1_800_000_000,
		Config: &score.TrainingConfig{TotalSteps: 1000, Worlds: 2, Slots: 8, Seed: 3},
		Latest: &score.TrainingMetrics{Update: 2, GlobalStep: 512, ChordRate: 0.25, ExplainedVariance: float32(math.NaN())},
	}
}

func (s *stubTrainer) StartTraining(_ context.Context, cfg *score.TrainingConfig) (*score.TrainingRun, error) {
	if cfg.GetSeed() == 99 {
		return nil, status.Error(codes.ResourceExhausted, "a training run is already in progress")
	}
	s.mu.Lock()
	s.started = cfg
	s.mu.Unlock()
	return s.run(), nil
}

func (s *stubTrainer) TrainingRuns(context.Context) ([]*score.TrainingRun, error) {
	return []*score.TrainingRun{s.run()}, nil
}

func (s *stubTrainer) TrainingRun(_ context.Context, id string) (*score.TrainingRun, error) {
	if id != "r1" {
		return nil, status.Error(codes.NotFound, "no training run")
	}
	return s.run(), nil
}

func (s *stubTrainer) StopTraining(ctx context.Context, id string) (*score.TrainingRun, error) {
	s.mu.Lock()
	s.stopped = true
	s.mu.Unlock()
	return s.TrainingRun(ctx, id)
}

type stubStream struct{ sent []*score.TrainingMetrics }

func (s *stubStream) Recv() (*score.TrainingMetrics, error) {
	if len(s.sent) == 0 {
		return nil, io.EOF
	}
	m := s.sent[0]
	s.sent = s.sent[1:]
	return m, nil
}

func (s *stubTrainer) WatchTraining(context.Context, string) (upstream.MetricsStream, error) {
	return &stubStream{sent: []*score.TrainingMetrics{{Update: 1}, {Update: 2, GlobalStep: 512}}}, nil
}

func TestTrainingRuns(t *testing.T) {
	trainer := &stubTrainer{}
	srv := newServerWith(t, nil, trainer)

	code, body := do(t, http.MethodPost, srv.URL+"/api/training-runs", `{"totalSteps":1000,"worlds":2,"slots":8,"seed":3}`)
	if code != http.StatusCreated {
		t.Fatalf("start: %d %s", code, body)
	}
	assertJSON(t, body, `{"id": "r1", "status": "running", "createdAt": "2027-01-15T08:00:00Z",
		"config": {"totalSteps": 1000, "worlds": 2, "slots": 8, "seed": 3},
		"latest": {"update": 2, "globalStep": 512, "elapsedSeconds": 0, "stepsPerSecond": 0,
			"episodeReturn": 0, "episodeLength": 0, "rewardPerStep": 0, "chordRate": 0.25,
			"deathsPer1kBeats": 0, "motesPer1kBeats": 0, "meanEnergy": 0, "policyLoss": 0, "valueLoss": 0,
			"entropy": 0, "approxKl": 0, "clipFraction": 0, "explainedVariance": 0, "learningRate": 0}}`)
	if trainer.started.GetWorlds() != 2 {
		t.Errorf("melody got config %v", trainer.started)
	}

	for _, bad := range []string{`{"slots":6}`, `{"worlds":5000}`, `{"nope":1}`} {
		if code, _ := do(t, http.MethodPost, srv.URL+"/api/training-runs", bad); code != http.StatusBadRequest {
			t.Errorf("POST %s: %d, want 400", bad, code)
		}
	}
	if code, _ := do(t, http.MethodPost, srv.URL+"/api/training-runs", `{"seed":99}`); code != http.StatusConflict {
		t.Errorf("start while running: %d, want 409", code)
	}
	if code, body := do(t, http.MethodGet, srv.URL+"/api/training-runs", ""); code != http.StatusOK || !strings.HasPrefix(body, `{"runs":[{"id":"r1"`) {
		t.Errorf("list: %d %s", code, body)
	}
	if code, _ := do(t, http.MethodGet, srv.URL+"/api/training-runs/r2", ""); code != http.StatusNotFound {
		t.Errorf("get unknown: %d, want 404", code)
	}
	if code, body := do(t, http.MethodPost, srv.URL+"/api/training-runs/r1/stop", ""); code != http.StatusAccepted || !strings.Contains(body, `"status":"stopped"`) {
		t.Errorf("stop: %d %s", code, body)
	}
}

func TestTrainingStream(t *testing.T) {
	srv := newServerWith(t, nil, &stubTrainer{stopped: true})
	req, _ := http.NewRequestWithContext(t.Context(), http.MethodGet, srv.URL+"/api/training-runs/r1/stream", nil)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = resp.Body.Close() }()
	var events []string
	lines := bufio.NewScanner(resp.Body)
	for lines.Scan() {
		if line := lines.Text(); line != "" {
			events = append(events, line)
		}
	}
	want := []string{
		"event: metrics", `data: {"update":1,`,
		"event: metrics", `data: {"update":2,"globalStep":512,`,
		"event: end", `data: {"status":"stopped"}`,
	}
	if len(events) != len(want) {
		t.Fatalf("events = %q", events)
	}
	for i := range want {
		if !strings.HasPrefix(events[i], want[i]) {
			t.Errorf("line %d = %q, want prefix %q", i, events[i], want[i])
		}
	}

	if code, _ := do(t, http.MethodGet, srv.URL+"/api/training-runs/r2/stream", ""); code != http.StatusNotFound {
		t.Errorf("stream of unknown run: %d, want 404", code)
	}
}

func TestTrainingUnavailableWithoutTrainer(t *testing.T) {
	srv := newServer(t)
	if code, _ := do(t, http.MethodGet, srv.URL+"/api/training-runs", ""); code != http.StatusServiceUnavailable {
		t.Errorf("list without trainer: %d, want 503", code)
	}
}
