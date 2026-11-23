package performance

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"slices"
	"sync"
	"testing"
	"time"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
)

func TestHubDropsOldestForSlowSubscriber(t *testing.T) {
	h := NewHub()
	frames, unsubscribe := h.Subscribe()
	defer unsubscribe()
	for i := range subscriberBuffer + 3 {
		h.Publish([]byte{byte(i)})
	}
	var got []byte
	for range subscriberBuffer {
		got = append(got, (<-frames)[0])
	}
	if got[0] != 3 || got[len(got)-1] != subscriberBuffer+2 {
		t.Errorf("got frames %v, want the newest %d", got, subscriberBuffer)
	}
}

func TestHubReplaysLatestAndCloses(t *testing.T) {
	h := NewHub()
	h.Publish([]byte("a"))
	h.Publish([]byte("b"))
	frames, unsubscribe := h.Subscribe()
	defer unsubscribe()
	if f := <-frames; string(f) != "b" {
		t.Errorf("first frame = %q, want the latest %q", f, "b")
	}
	if h.Count() != 1 {
		t.Errorf("Count = %d, want 1", h.Count())
	}
	h.Close()
	if _, ok := <-frames; ok {
		t.Error("channel still open after Close")
	}
	late, _ := h.Subscribe()
	if _, ok := <-late; ok {
		t.Error("subscribing after Close should give a closed channel")
	}
}

func snapshot(beat uint64) *score.WorldSnapshot {
	return &score.WorldSnapshot{
		Beat: beat, Width: 100, Height: 100,
		Quartetlings: []*score.Quartetling{
			{Slot: 0, Id: 7, Voice: score.Voice_VOICE_ALTO, X: 1.23456, Y: 2, Heading: 0.5, Energy: 1, Breath: 0.5, Singing: true, Generation: 2},
		},
		Motes:   []*score.Mote{{X: 3, Y: 4}},
		Statics: []*score.StaticField{{X: 5, Y: 6, Radius: 10}},
		Stats:   &score.BeatStats{MotesEaten: 1, ChordMembers: 0},
	}
}

func TestFrameEncoding(t *testing.T) {
	data, err := EncodeFrame(snapshot(3))
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatal(err)
	}
	want := `{"beat":3,"width":100,"height":100,
		"quartetlings":{"id":[7],"voice":[1],"x":[1.235],"y":[2],"heading":[0.5],"energy":[1],"breath":[0.5],
			"singing":[1],"inChord":[0],"age":[0],"generation":[2]},
		"motes":{"x":[3],"y":[4]},"statics":{"x":[5],"y":[6],"radius":[10]},
		"stats":{"motesEaten":1,"deaths":0,"chordMembers":0,"births":0}}`
	var w map[string]any
	if err := json.Unmarshal([]byte(want), &w); err != nil {
		t.Fatal(err)
	}
	gb, _ := json.Marshal(got)
	wb, _ := json.Marshal(w)
	if string(gb) != string(wb) {
		t.Errorf("frame JSON\n got: %s\nwant: %s", gb, wb)
	}
}

// fakeEngine counts calls and can be told to fail steps. Observations are a
// 1-element tensor holding the beat, so tests can follow them through melody.
type fakeEngine struct {
	mu       sync.Mutex
	beats    map[string]uint64
	created  int
	deleted  []string
	steps    []*score.StepWorldRequest
	failStep bool
}

func newFakeEngine() *fakeEngine { return &fakeEngine{beats: map[string]uint64{}} }

func observations(beat uint64) *score.Tensor {
	return &score.Tensor{Dtype: score.Dtype_DTYPE_FLOAT32, Shape: []uint32{1}, Data: []byte{byte(beat), 0, 0, 0}}
}

func (f *fakeEngine) CreateWorld(_ context.Context, _ *score.WorldConfig, includeObs bool) (*score.CreateWorldResponse, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.created++
	id := "w" + string(rune('0'+f.created))
	f.beats[id] = 0
	resp := &score.CreateWorldResponse{WorldId: id, Snapshot: snapshot(0), ObservationVersion: 3}
	if includeObs {
		resp.Observations = observations(0)
	}
	return resp, nil
}

func (f *fakeEngine) StepWorld(_ context.Context, req *score.StepWorldRequest) (*score.StepWorldResponse, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.failStep {
		return nil, errors.New("engine down")
	}
	f.steps = append(f.steps, req)
	id := req.GetWorldId()
	f.beats[id]++
	resp := &score.StepWorldResponse{Snapshot: snapshot(f.beats[id])}
	if req.GetIncludeObservations() {
		resp.Observations = observations(f.beats[id])
	}
	return resp, nil
}

func (f *fakeEngine) Census(_ context.Context, id string) (*score.Census, error) {
	return &score.Census{Beat: f.beats[id]}, nil
}

func (f *fakeEngine) stepped() []*score.StepWorldRequest {
	f.mu.Lock()
	defer f.mu.Unlock()
	return slices.Clone(f.steps)
}

// fakeActor knows one policy, "ppo-1", at observation version 3. Its actions
// echo the observed beat, so tests can check what was sent where.
type fakeActor struct {
	mu   sync.Mutex
	seen []byte
}

func (a *fakeActor) Act(_ context.Context, policy string, obs *score.Tensor, version uint32) (*score.Tensor, error) {
	if policy != "ppo-1" {
		return nil, fmt.Errorf("melody: Act: %w", status.Error(codes.NotFound, "no trained policy"))
	}
	if version != 3 {
		return nil, fmt.Errorf("melody: Act: %w", status.Error(codes.FailedPrecondition, "trained on version 3"))
	}
	a.mu.Lock()
	a.seen = append(a.seen, obs.GetData()[0])
	a.mu.Unlock()
	return &score.Tensor{Dtype: score.Dtype_DTYPE_FLOAT32, Shape: []uint32{1, 3}, Data: obs.GetData()}, nil
}

func (a *fakeActor) Policies(context.Context) ([]*score.PolicyInfo, error) {
	return []*score.PolicyInfo{
		{Name: "ppo-1", GlobalStep: 2_500_000, Status: score.TrainingStatus_TRAINING_STATUS_FINISHED},
		{Name: "forage"}, // shadowed by the builtin
	}, nil
}

func (f *fakeEngine) DeleteWorld(_ context.Context, id string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.deleted = append(f.deleted, id)
	return nil
}

func discard() *slog.Logger { return slog.New(slog.NewTextHandler(io.Discard, nil)) }

func waitFor(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(3 * time.Second)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(5 * time.Millisecond)
	}
}

func TestCreateValidates(t *testing.T) {
	m := NewManager(t.Context(), newFakeEngine(), nil, discard())
	for _, cfg := range []Config{
		{Policy: "nope"},
		{Policy: "choir", Slots: 6},
		{Policy: "choir", Slots: 2048},
		{Policy: "choir", BeatsPerSecond: 500},
	} {
		_, err := m.Create(t.Context(), cfg)
		var verr *ValidationError
		if !errors.As(err, &verr) {
			t.Errorf("Create(%+v) error = %v, want ValidationError", cfg, err)
		}
	}
}

func TestPerformanceBeatsAndStreams(t *testing.T) {
	engine := newFakeEngine()
	m := NewManager(t.Context(), engine, nil, discard())
	p, err := m.Create(t.Context(), Config{Policy: "forage", BeatsPerSecond: 60})
	if err != nil {
		t.Fatal(err)
	}
	info := p.Info()
	if info.Slots != DefaultSlots || info.Status != StatusPlaying || info.Policy != "forage" {
		t.Errorf("Info = %+v", info)
	}

	frames, unsubscribe := p.Subscribe()
	defer unsubscribe()
	first := <-frames // the initial snapshot, replayed on subscribe
	var f Frame
	if err := json.Unmarshal(first, &f); err != nil {
		t.Fatal(err)
	}
	if f.Beat != 0 {
		t.Errorf("first frame beat = %d, want 0", f.Beat)
	}
	waitFor(t, "beats to advance", func() bool { return p.Info().Beat >= 3 })

	if got := m.List(); len(got) != 1 || got[0].ID != p.Info().ID {
		t.Errorf("List = %+v", got)
	}
	if err := m.Delete(t.Context(), info.ID); err != nil {
		t.Fatal(err)
	}
	for range frames { // drains, then ends because the hub closed
	}
	if len(engine.deleted) != 1 {
		t.Errorf("engine deletes = %v, want 1", engine.deleted)
	}
	if err := m.Delete(t.Context(), info.ID); !errors.Is(err, ErrNotFound) {
		t.Errorf("second Delete error = %v, want ErrNotFound", err)
	}
}

func TestPerformanceFailsAfterRepeatedEngineErrors(t *testing.T) {
	engine := newFakeEngine()
	engine.failStep = true
	m := NewManager(t.Context(), engine, nil, discard())
	p, err := m.Create(t.Context(), Config{Policy: "random", BeatsPerSecond: 60})
	if err != nil {
		t.Fatal(err)
	}
	waitFor(t, "failure", func() bool { return p.Info().Status == StatusFailed })
	if p.Info().Error == "" {
		t.Error("failed performance should report its error")
	}
}

func TestCloseStopsEverything(t *testing.T) {
	engine := newFakeEngine()
	m := NewManager(t.Context(), engine, nil, discard())
	for range 3 {
		if _, err := m.Create(t.Context(), Config{Policy: "choir"}); err != nil {
			t.Fatal(err)
		}
	}
	m.Close(t.Context())
	if len(m.List()) != 0 || len(engine.deleted) != 3 {
		t.Errorf("after Close: %d performances, %d deletes", len(m.List()), len(engine.deleted))
	}
}

func TestCheckpointPolicyDrivesBeatsThroughMelody(t *testing.T) {
	engine := newFakeEngine()
	actor := &fakeActor{}
	m := NewManager(t.Context(), engine, actor, discard())
	p, err := m.Create(t.Context(), Config{Policy: "ppo-1", BeatsPerSecond: 60})
	if err != nil {
		t.Fatal(err)
	}
	if got := p.Info().PolicyKind; got != KindCheckpoint {
		t.Errorf("PolicyKind = %q, want %q", got, KindCheckpoint)
	}
	waitFor(t, "beats", func() bool { return p.Info().Beat >= 3 })
	if err := m.Delete(t.Context(), p.Info().ID); err != nil {
		t.Fatal(err)
	}
	steps := engine.stepped()
	for i, req := range steps[:3] {
		if !req.GetIncludeObservations() {
			t.Errorf("step %d did not ask for observations", i)
		}
		// Each beat's actions come from the previous beat's observations.
		if got := req.GetActions().GetData()[0]; got != byte(i) {
			t.Errorf("step %d used actions from beat %d", i, got)
		}
	}
	// The first Act call checked the policy at create time.
	actor.mu.Lock()
	defer actor.mu.Unlock()
	if actor.seen[0] != 0 || actor.seen[1] != 0 {
		t.Errorf("Act saw beats %v, want the initial observations twice first", actor.seen)
	}
}

func TestCreateRejectsUnknownOrIncompatibleCheckpoints(t *testing.T) {
	engine := newFakeEngine()
	m := NewManager(t.Context(), engine, &fakeActor{}, discard())
	_, err := m.Create(t.Context(), Config{Policy: "ppo-404"})
	var verr *ValidationError
	if !errors.As(err, &verr) {
		t.Fatalf("Create(unknown) error = %v, want ValidationError", err)
	}
	if len(engine.deleted) != 1 {
		t.Errorf("the world created for a rejected policy was not deleted: %v", engine.deleted)
	}
	if len(m.List()) != 0 {
		t.Error("a rejected performance was registered")
	}
	// Without melody, only builtins are allowed.
	noActor := NewManager(t.Context(), newFakeEngine(), nil, discard())
	if _, err := noActor.Create(t.Context(), Config{Policy: "ppo-1"}); !errors.As(err, &verr) {
		t.Errorf("Create without actor error = %v, want ValidationError", err)
	}
}

func TestPoliciesMergesTrainedAfterBuiltins(t *testing.T) {
	m := NewManager(t.Context(), newFakeEngine(), &fakeActor{}, discard())
	policies, err := m.Policies(t.Context())
	if err != nil {
		t.Fatal(err)
	}
	if len(policies) != 4 {
		t.Fatalf("policies = %+v, want 3 builtins and ppo-1", policies)
	}
	last := policies[3]
	if last.Name != "ppo-1" || last.Kind != KindCheckpoint || last.GlobalStep != 2_500_000 || last.TrainingStatus != "finished" {
		t.Errorf("checkpoint policy = %+v", last)
	}
}

func TestCensusNeedsAPerformance(t *testing.T) {
	engine := newFakeEngine()
	m := NewManager(t.Context(), engine, nil, discard())
	if _, err := m.Census(t.Context(), "p9"); !errors.Is(err, ErrNotFound) {
		t.Errorf("Census(unknown) error = %v, want ErrNotFound", err)
	}
	p, err := m.Create(t.Context(), Config{Policy: "forage"})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := m.Census(t.Context(), p.Info().ID); err != nil {
		t.Errorf("Census error = %v", err)
	}
}

// recordingEngine remembers the world configs it was asked to create.
type recordingEngine struct {
	*fakeEngine
	configs []*score.WorldConfig
}

func (r *recordingEngine) CreateWorld(ctx context.Context, cfg *score.WorldConfig, obs bool) (*score.CreateWorldResponse, error) {
	r.configs = append(r.configs, cfg)
	return r.fakeEngine.CreateWorld(ctx, cfg, obs)
}

func TestPerformancesEvolveByDefault(t *testing.T) {
	engine := &recordingEngine{fakeEngine: newFakeEngine()}
	m := NewManager(t.Context(), engine, nil, discard())
	off := false
	if _, err := m.Create(t.Context(), Config{Policy: "forage"}); err != nil {
		t.Fatal(err)
	}
	p, err := m.Create(t.Context(), Config{Policy: "forage", Evolve: &off})
	if err != nil {
		t.Fatal(err)
	}
	if !engine.configs[0].GetEvolve() || engine.configs[1].GetEvolve() {
		t.Errorf("evolve sent to harmony = %v, %v; want true, false", engine.configs[0].GetEvolve(), engine.configs[1].GetEvolve())
	}
	if p.Info().Evolve {
		t.Error("Info().Evolve = true for a performance started with evolve false")
	}
	m.Close(t.Context())
}
