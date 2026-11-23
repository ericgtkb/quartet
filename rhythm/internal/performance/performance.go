// Package performance runs live worlds: one goroutine per performance drives
// harmony one beat at a time and fans each snapshot out to subscribers.
package performance

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"math/rand/v2"
	"slices"
	"strings"
	"sync"
	"time"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
)

// Engine is the world engine (harmony).
type Engine interface {
	CreateWorld(ctx context.Context, cfg *score.WorldConfig, includeObservations bool) (*score.CreateWorldResponse, error)
	StepWorld(ctx context.Context, req *score.StepWorldRequest) (*score.StepWorldResponse, error)
	DeleteWorld(ctx context.Context, worldID string) error
	Census(ctx context.Context, worldID string) (*score.Census, error)
}

// Actor runs trained policies (melody).
type Actor interface {
	Act(ctx context.Context, policy string, observations *score.Tensor, observationVersion uint32) (*score.Tensor, error)
	Policies(ctx context.Context) ([]*score.PolicyInfo, error)
}

// Limits and defaults.
const (
	MaxPerformances       = 16
	DefaultSlots          = 64
	MaxSlots              = 1024
	DefaultBeatsPerSecond = 15
	MaxBeatsPerSecond     = 60
	// maxConsecutiveFailures ends a performance whose engine keeps failing.
	maxConsecutiveFailures = 5
	engineTimeout          = 2 * time.Second
)

// Status values.
const (
	StatusPlaying = "playing"
	StatusFailed  = "failed"
)

// Policy kinds.
const (
	KindBuiltin    = "builtin"
	KindCheckpoint = "checkpoint"
)

// Policy describes a policy a performance can use.
type Policy struct {
	Name        string `json:"name"`
	Kind        string `json:"kind"`
	Description string `json:"description"`
	// Checkpoints only: agent-steps trained and the run's status.
	GlobalStep     uint64 `json:"globalStep,omitempty"`
	TrainingStatus string `json:"trainingStatus,omitempty"`
}

var builtins = map[string]score.BuiltinPolicy{
	"random": score.BuiltinPolicy_BUILTIN_POLICY_RANDOM,
	"forage": score.BuiltinPolicy_BUILTIN_POLICY_FORAGE,
	"choir":  score.BuiltinPolicy_BUILTIN_POLICY_CHOIR,
}

// Builtins lists the policies harmony implements.
func Builtins() []Policy {
	return []Policy{
		{Name: "random", Kind: KindBuiltin, Description: "Random turns and thrust; sings now and then."},
		{Name: "forage", Kind: KindBuiltin, Description: "Seeks the nearest mote and flees static. Never sings."},
		{Name: "choir", Kind: KindBuiltin, Description: "Forages, and stops to sing when two other voices are near."},
	}
}

// TrainingStatusName is the public name of a training status.
func TrainingStatusName(s score.TrainingStatus) string {
	switch s {
	case score.TrainingStatus_TRAINING_STATUS_RUNNING:
		return "running"
	case score.TrainingStatus_TRAINING_STATUS_FINISHED:
		return "finished"
	case score.TrainingStatus_TRAINING_STATUS_STOPPED:
		return "stopped"
	case score.TrainingStatus_TRAINING_STATUS_FAILED:
		return "failed"
	default:
		return "unknown"
	}
}

// Policies lists builtins, then melody's trained policies (newest first).
// Without an actor, or if melody fails, it returns the builtins and the error.
func (m *Manager) Policies(ctx context.Context) ([]Policy, error) {
	policies := Builtins()
	if m.actor == nil {
		return policies, nil
	}
	trained, err := m.actor.Policies(ctx)
	if err != nil {
		return policies, err
	}
	for _, t := range trained {
		if _, builtin := builtins[t.GetName()]; builtin {
			continue
		}
		policies = append(policies, Policy{
			Name:           t.GetName(),
			Kind:           KindCheckpoint,
			Description:    fmt.Sprintf("PPO policy trained for %.1fM agent-steps.", float64(t.GetGlobalStep())/1e6),
			GlobalStep:     t.GetGlobalStep(),
			TrainingStatus: TrainingStatusName(t.GetStatus()),
		})
	}
	return policies, nil
}

// Config is a request to start a performance. Zero values take defaults.
type Config struct {
	Policy         string  `json:"policy"`
	Slots          uint32  `json:"slots,omitempty"`
	Seed           *uint64 `json:"seed,omitempty"`
	BeatsPerSecond float64 `json:"beatsPerSecond,omitempty"`
	// Evolve fills vacated slots with offspring (default true).
	Evolve *bool `json:"evolve,omitempty"`
}

// ValidationError is a bad Config.
type ValidationError struct{ msg string }

func (e *ValidationError) Error() string { return e.msg }

func invalid(format string, args ...any) error {
	return &ValidationError{msg: fmt.Sprintf(format, args...)}
}

// ErrNotFound means no performance has the given id.
var ErrNotFound = errors.New("performance not found")

// ErrLimit means MaxPerformances are already running.
var ErrLimit = fmt.Errorf("at most %d performances may run at once", MaxPerformances)

// normalize fills in defaults and validates c. A policy that isn't a builtin
// is taken to be a trained checkpoint (builtin = 0) when trained is true;
// Create then checks it with melody.
func (c *Config) normalize(trained bool) (score.BuiltinPolicy, error) {
	policy, ok := builtins[c.Policy]
	if !ok && (!trained || c.Policy == "") {
		names := make([]string, 0, len(builtins))
		for name := range builtins {
			names = append(names, name)
		}
		slices.Sort(names)
		return 0, invalid("unknown policy %q (builtins: %s)", c.Policy, strings.Join(names, ", "))
	}
	if c.Slots == 0 {
		c.Slots = DefaultSlots
	}
	if c.Slots%4 != 0 || c.Slots > MaxSlots {
		return 0, invalid("slots must be a positive multiple of 4 up to %d, got %d", MaxSlots, c.Slots)
	}
	if c.BeatsPerSecond == 0 {
		c.BeatsPerSecond = DefaultBeatsPerSecond
	}
	if c.BeatsPerSecond < 1 || c.BeatsPerSecond > MaxBeatsPerSecond {
		return 0, invalid("beatsPerSecond must be in [1, %d], got %g", MaxBeatsPerSecond, c.BeatsPerSecond)
	}
	if c.Seed == nil {
		seed := rand.Uint64()
		c.Seed = &seed
	}
	if c.Evolve == nil {
		evolve := true
		c.Evolve = &evolve
	}
	return policy, nil
}

// Info is a performance's public description.
type Info struct {
	ID             string    `json:"id"`
	Policy         string    `json:"policy"`
	PolicyKind     string    `json:"policyKind"`
	Slots          uint32    `json:"slots"`
	Seed           uint64    `json:"seed"`
	BeatsPerSecond float64   `json:"beatsPerSecond"`
	Evolve         bool      `json:"evolve"`
	Beat           uint64    `json:"beat"`
	Status         string    `json:"status"`
	Error          string    `json:"error,omitempty"`
	Subscribers    int       `json:"subscribers"`
	CreatedAt      time.Time `json:"createdAt"`
}

// Performance is one live world.
type Performance struct {
	id        string
	worldID   string
	config    Config
	policy    score.BuiltinPolicy // 0 for a checkpoint policy
	actor     Actor
	createdAt time.Time
	hub       *Hub
	cancel    context.CancelFunc
	done      chan struct{}

	// Checkpoint policies only; used by the beat loop alone.
	observations       *score.Tensor
	observationVersion uint32

	mu     sync.Mutex
	beat   uint64
	status string
	err    string
}

func (p *Performance) kind() string {
	if p.policy == 0 {
		return KindCheckpoint
	}
	return KindBuiltin
}

// Info returns the performance's current description.
func (p *Performance) Info() Info {
	p.mu.Lock()
	defer p.mu.Unlock()
	return Info{
		ID: p.id, Policy: p.config.Policy, PolicyKind: p.kind(), Slots: p.config.Slots, Seed: *p.config.Seed,
		BeatsPerSecond: p.config.BeatsPerSecond, Evolve: *p.config.Evolve, Beat: p.beat, Status: p.status, Error: p.err,
		Subscribers: p.hub.Count(), CreatedAt: p.createdAt,
	}
}

// Subscribe streams encoded frames; see Hub.Subscribe.
func (p *Performance) Subscribe() (<-chan []byte, func()) {
	return p.hub.Subscribe()
}

func (p *Performance) publish(snap *score.WorldSnapshot, log *slog.Logger) {
	frame, err := EncodeFrame(snap)
	if err != nil {
		log.Error("encode frame", "performance", p.id, "err", err)
		return
	}
	p.mu.Lock()
	p.beat = snap.GetBeat()
	p.mu.Unlock()
	p.hub.Publish(frame)
}

// step plays one beat. A checkpoint policy asks melody for actions on the
// last observations, then harmony steps with them and returns the next ones.
func (p *Performance) step(ctx context.Context, engine Engine) (*score.WorldSnapshot, error) {
	req := &score.StepWorldRequest{WorldId: p.worldID}
	if p.policy != 0 {
		req.Control = &score.StepWorldRequest_Builtin{Builtin: p.policy}
	} else {
		actions, err := p.actor.Act(ctx, p.config.Policy, p.observations, p.observationVersion)
		if err != nil {
			return nil, err
		}
		req.Control = &score.StepWorldRequest_Actions{Actions: actions}
		req.IncludeObservations = true
	}
	resp, err := engine.StepWorld(ctx, req)
	if err != nil {
		return nil, err
	}
	if p.policy == 0 {
		p.observations = resp.GetObservations()
	}
	return resp.GetSnapshot(), nil
}

// run drives the beat loop until ctx is cancelled or a beat keeps failing.
// A slow beat skips ticks instead of queueing them (time.Ticker drops ticks).
func (p *Performance) run(ctx context.Context, engine Engine, log *slog.Logger) {
	defer close(p.done)
	defer p.hub.Close()
	ticker := time.NewTicker(time.Duration(float64(time.Second) / p.config.BeatsPerSecond))
	defer ticker.Stop()

	failures := 0
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
		stepCtx, cancel := context.WithTimeout(ctx, engineTimeout)
		snap, err := p.step(stepCtx, engine)
		cancel()
		if err != nil {
			if ctx.Err() != nil {
				return
			}
			failures++
			log.Warn("beat failed", "performance", p.id, "failures", failures, "err", err)
			if failures >= maxConsecutiveFailures {
				p.mu.Lock()
				p.status, p.err = StatusFailed, err.Error()
				p.mu.Unlock()
				return
			}
			continue
		}
		failures = 0
		p.publish(snap, log)
	}
}

// Manager owns all performances.
type Manager struct {
	engine Engine
	actor  Actor // nil: builtin policies only
	log    *slog.Logger
	ctx    context.Context
	next   uint64

	mu    sync.Mutex
	perfs map[string]*Performance
}

// NewManager returns a manager whose beat loops run until ctx is cancelled
// or Close is called. actor may be nil, which allows builtin policies only.
func NewManager(ctx context.Context, engine Engine, actor Actor, log *slog.Logger) *Manager {
	return &Manager{engine: engine, actor: actor, log: log, ctx: ctx, perfs: make(map[string]*Performance)}
}

// Create validates cfg, creates a world in the engine, and starts its beat loop.
func (m *Manager) Create(ctx context.Context, cfg Config) (*Performance, error) {
	policy, err := cfg.normalize(m.actor != nil)
	if err != nil {
		return nil, err
	}
	m.mu.Lock()
	if len(m.perfs) >= MaxPerformances {
		m.mu.Unlock()
		return nil, ErrLimit
	}
	m.next++
	id := fmt.Sprintf("p%d", m.next)
	m.mu.Unlock()

	createCtx, cancel := context.WithTimeout(ctx, engineTimeout)
	defer cancel()
	checkpoint := policy == 0
	created, err := m.engine.CreateWorld(createCtx, &score.WorldConfig{Seed: *cfg.Seed, Slots: cfg.Slots, Evolve: *cfg.Evolve}, checkpoint)
	if err != nil {
		return nil, err
	}
	worldID := created.GetWorldId()
	if checkpoint {
		// Check the policy now, so an unknown or incompatible checkpoint is a
		// bad request rather than a performance that fails on its first beat.
		_, err := m.actor.Act(createCtx, cfg.Policy, created.GetObservations(), created.GetObservationVersion())
		if err != nil {
			if derr := m.engine.DeleteWorld(context.WithoutCancel(ctx), worldID); derr != nil {
				m.log.Warn("delete world after failed policy check", "world", worldID, "err", derr)
			}
			switch st := GRPCStatus(err); st.Code() {
			case codes.NotFound:
				return nil, invalid("unknown policy %q (see GET /api/policies)", cfg.Policy)
			case codes.FailedPrecondition, codes.InvalidArgument:
				return nil, invalid("policy %q can't drive this world: %s", cfg.Policy, st.Message())
			}
			return nil, err
		}
	}

	runCtx, stop := context.WithCancel(m.ctx)
	p := &Performance{
		id: id, worldID: worldID, config: cfg, policy: policy, actor: m.actor, createdAt: time.Now().UTC(),
		hub: NewHub(), cancel: stop, done: make(chan struct{}), status: StatusPlaying,
		observations: created.GetObservations(), observationVersion: created.GetObservationVersion(),
	}
	p.publish(created.GetSnapshot(), m.log)

	m.mu.Lock()
	m.perfs[id] = p
	m.mu.Unlock()
	go p.run(runCtx, m.engine, m.log)
	m.log.Info("performance started", "performance", id, "world", worldID, "policy", cfg.Policy, "slots", cfg.Slots)
	return p, nil
}

// GRPCStatus finds the gRPC status in a (possibly wrapped) error, so callers
// see the upstream's code and message rather than the wrapper's text.
func GRPCStatus(err error) *status.Status {
	var se interface{ GRPCStatus() *status.Status }
	if errors.As(err, &se) {
		return se.GRPCStatus()
	}
	return status.Convert(err)
}

// Census returns the aggregated statistics of a performance's world.
func (m *Manager) Census(ctx context.Context, id string) (*score.Census, error) {
	p, ok := m.Get(id)
	if !ok {
		return nil, ErrNotFound
	}
	cctx, cancel := context.WithTimeout(ctx, engineTimeout)
	defer cancel()
	return m.engine.Census(cctx, p.worldID)
}

// Get returns the performance with id.
func (m *Manager) Get(id string) (*Performance, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	p, ok := m.perfs[id]
	return p, ok
}

// List describes every performance, oldest first.
func (m *Manager) List() []Info {
	m.mu.Lock()
	perfs := make([]*Performance, 0, len(m.perfs))
	for _, p := range m.perfs {
		perfs = append(perfs, p)
	}
	m.mu.Unlock()
	infos := make([]Info, 0, len(perfs))
	for _, p := range perfs {
		infos = append(infos, p.Info())
	}
	slices.SortFunc(infos, func(a, b Info) int { return a.CreatedAt.Compare(b.CreatedAt) })
	return infos
}

// Delete stops a performance and deletes its world. The world is removed
// from the manager even if the engine call fails.
func (m *Manager) Delete(ctx context.Context, id string) error {
	m.mu.Lock()
	p, ok := m.perfs[id]
	delete(m.perfs, id)
	m.mu.Unlock()
	if !ok {
		return ErrNotFound
	}
	p.cancel()
	<-p.done
	deleteCtx, cancel := context.WithTimeout(ctx, engineTimeout)
	defer cancel()
	if err := m.engine.DeleteWorld(deleteCtx, p.worldID); err != nil {
		return err
	}
	m.log.Info("performance stopped", "performance", id)
	return nil
}

// Close stops every performance and deletes its world.
func (m *Manager) Close(ctx context.Context) {
	m.mu.Lock()
	ids := make([]string, 0, len(m.perfs))
	for id := range m.perfs {
		ids = append(ids, id)
	}
	m.mu.Unlock()
	for _, id := range ids {
		if err := m.Delete(ctx, id); err != nil && !errors.Is(err, ErrNotFound) {
			m.log.Warn("stop performance", "performance", id, "err", err)
		}
	}
}
