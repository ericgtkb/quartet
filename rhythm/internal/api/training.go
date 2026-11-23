package api

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	"google.golang.org/grpc/codes"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
	"github.com/ericgtkb/quartet/rhythm/internal/performance"
	"github.com/ericgtkb/quartet/rhythm/internal/upstream"
)

// Trainer runs training (melody).
type Trainer interface {
	StartTraining(ctx context.Context, cfg *score.TrainingConfig) (*score.TrainingRun, error)
	TrainingRuns(ctx context.Context) ([]*score.TrainingRun, error)
	TrainingRun(ctx context.Context, id string) (*score.TrainingRun, error)
	StopTraining(ctx context.Context, id string) (*score.TrainingRun, error)
	WatchTraining(ctx context.Context, id string) (upstream.MetricsStream, error)
}

// Limits on POST /api/training-runs. melody validates the rest.
const (
	maxTrainingSteps  = 10_000_000_000
	maxTrainingWorlds = 1024
	maxTrainingSlots  = 1024
	// maxWatch bounds a training metrics stream; the browser reconnects.
	maxWatch = 12 * time.Hour
)

// TrainingConfig configures a training run. In a request, zero values take
// melody's defaults; responses carry the values used.
type TrainingConfig struct {
	TotalSteps uint64 `json:"totalSteps"`
	Worlds     uint32 `json:"worlds"`
	Slots      uint32 `json:"slots"`
	Seed       uint64 `json:"seed"`
}

// TrainingMetrics are one PPO update's metrics (see score/melody.proto).
type TrainingMetrics struct {
	Update            uint32  `json:"update"`
	GlobalStep        uint64  `json:"globalStep"`
	ElapsedSeconds    float64 `json:"elapsedSeconds"`
	StepsPerSecond    float64 `json:"stepsPerSecond"`
	EpisodeReturn     float64 `json:"episodeReturn"`
	EpisodeLength     float64 `json:"episodeLength"`
	RewardPerStep     float64 `json:"rewardPerStep"`
	ChordRate         float64 `json:"chordRate"`
	DeathsPer1kBeats  float64 `json:"deathsPer1kBeats"`
	MotesPer1kBeats   float64 `json:"motesPer1kBeats"`
	MeanEnergy        float64 `json:"meanEnergy"`
	PolicyLoss        float64 `json:"policyLoss"`
	ValueLoss         float64 `json:"valueLoss"`
	Entropy           float64 `json:"entropy"`
	ApproxKL          float64 `json:"approxKl"`
	ClipFraction      float64 `json:"clipFraction"`
	ExplainedVariance float64 `json:"explainedVariance"`
	LearningRate      float64 `json:"learningRate"`
}

// TrainingRun describes a training run.
type TrainingRun struct {
	ID        string           `json:"id"`
	Status    string           `json:"status"` // running, finished, stopped, failed
	Config    TrainingConfig   `json:"config"`
	CreatedAt time.Time        `json:"createdAt"`
	Error     string           `json:"error,omitempty"`
	Latest    *TrainingMetrics `json:"latest,omitempty"`
}

// TrainingRunsResponse is the body of GET /api/training-runs.
type TrainingRunsResponse struct {
	Runs []TrainingRun `json:"runs"`
}

// NewTrainingMetrics converts melody's metrics, mapping NaN and ±Inf to 0.
func NewTrainingMetrics(m *score.TrainingMetrics) TrainingMetrics {
	return TrainingMetrics{
		Update: m.GetUpdate(), GlobalStep: m.GetGlobalStep(),
		ElapsedSeconds: num(m.GetElapsedSeconds()), StepsPerSecond: num(m.GetStepsPerSecond()),
		EpisodeReturn: num(m.GetEpisodeReturn()), EpisodeLength: num(m.GetEpisodeLength()),
		RewardPerStep: num(m.GetRewardPerStep()), ChordRate: num(m.GetChordRate()),
		DeathsPer1kBeats: num(m.GetDeathsPer_1KBeats()), MotesPer1kBeats: num(m.GetMotesPer_1KBeats()),
		MeanEnergy: num(m.GetMeanEnergy()), PolicyLoss: num(m.GetPolicyLoss()), ValueLoss: num(m.GetValueLoss()),
		Entropy: num(m.GetEntropy()), ApproxKL: num(m.GetApproxKl()), ClipFraction: num(m.GetClipFraction()),
		ExplainedVariance: num(m.GetExplainedVariance()), LearningRate: float64(m.GetLearningRate()),
	}
}

// NewTrainingRun converts melody's run description.
func NewTrainingRun(r *score.TrainingRun) TrainingRun {
	c := r.GetConfig()
	run := TrainingRun{
		ID:     r.GetId(),
		Status: performance.TrainingStatusName(r.GetStatus()),
		Config: TrainingConfig{
			TotalSteps: c.GetTotalSteps(), Worlds: c.GetWorlds(), Slots: c.GetSlots(), Seed: c.GetSeed(),
		},
		CreatedAt: time.Unix(r.GetCreatedAt(), 0).UTC(),
		Error:     r.GetError(),
	}
	if r.GetLatest() != nil {
		m := NewTrainingMetrics(r.GetLatest())
		run.Latest = &m
	}
	return run
}

func (c TrainingConfig) validate() error {
	switch {
	case c.TotalSteps > maxTrainingSteps:
		return fmt.Errorf("totalSteps must be at most %d", uint64(maxTrainingSteps))
	case c.Worlds > maxTrainingWorlds:
		return fmt.Errorf("worlds must be at most %d", maxTrainingWorlds)
	case c.Slots%4 != 0 || c.Slots > maxTrainingSlots:
		return fmt.Errorf("slots must be a multiple of 4 up to %d", maxTrainingSlots)
	}
	return nil
}

// upstreamError answers with the HTTP status matching an upstream's gRPC code.
func (h *handlers) upstreamError(w http.ResponseWriter, what string, err error) {
	st := performance.GRPCStatus(err)
	switch st.Code() {
	case codes.InvalidArgument:
		writeError(w, http.StatusBadRequest, st.Message())
	case codes.NotFound:
		writeError(w, http.StatusNotFound, st.Message())
	case codes.ResourceExhausted, codes.FailedPrecondition:
		writeError(w, http.StatusConflict, st.Message())
	default:
		h.log.Warn(what, "err", err)
		writeError(w, http.StatusBadGateway, err.Error())
	}
}

// withTrainer rejects training requests when rhythm has no trainer.
func (h *handlers) withTrainer(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if h.trainer == nil {
			writeError(w, http.StatusServiceUnavailable, "training is not available")
			return
		}
		next(w, r)
	}
}

func (h *handlers) listTrainingRuns(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), upstreamTimeout)
	defer cancel()
	runs, err := h.trainer.TrainingRuns(ctx)
	if err != nil {
		h.upstreamError(w, "list training runs", err)
		return
	}
	resp := TrainingRunsResponse{Runs: make([]TrainingRun, 0, len(runs))}
	for _, run := range runs {
		resp.Runs = append(resp.Runs, NewTrainingRun(run))
	}
	writeJSON(w, http.StatusOK, resp)
}

func (h *handlers) startTraining(w http.ResponseWriter, r *http.Request) {
	var cfg TrainingConfig
	dec := json.NewDecoder(http.MaxBytesReader(w, r.Body, maxBodyBytes))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&cfg); err != nil {
		writeError(w, http.StatusBadRequest, fmt.Sprintf("invalid JSON body: %v", err))
		return
	}
	if err := cfg.validate(); err != nil {
		writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), upstreamTimeout)
	defer cancel()
	run, err := h.trainer.StartTraining(ctx, &score.TrainingConfig{
		TotalSteps: cfg.TotalSteps, Worlds: cfg.Worlds, Slots: cfg.Slots, Seed: cfg.Seed,
	})
	if err != nil {
		h.upstreamError(w, "start training", err)
		return
	}
	writeJSON(w, http.StatusCreated, NewTrainingRun(run))
}

func (h *handlers) getTrainingRun(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), upstreamTimeout)
	defer cancel()
	run, err := h.trainer.TrainingRun(ctx, chi.URLParam(r, "id"))
	if err != nil {
		h.upstreamError(w, "get training run", err)
		return
	}
	writeJSON(w, http.StatusOK, NewTrainingRun(run))
}

// stopTraining asks a run to stop. It answers at once; the run saves a
// checkpoint and its stream ends with status "stopped" shortly after.
func (h *handlers) stopTraining(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), upstreamTimeout)
	defer cancel()
	run, err := h.trainer.StopTraining(ctx, chi.URLParam(r, "id"))
	if err != nil {
		h.upstreamError(w, "stop training", err)
		return
	}
	writeJSON(w, http.StatusAccepted, NewTrainingRun(run))
}

// streamTraining sends a "metrics" event per PPO update (the history first)
// and an "end" event with the run's final status.
func (h *handlers) streamTraining(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "id")
	getCtx, cancelGet := context.WithTimeout(r.Context(), upstreamTimeout)
	_, err := h.trainer.TrainingRun(getCtx, id)
	cancelGet()
	if err != nil {
		h.upstreamError(w, "watch training", err)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), maxWatch)
	defer cancel()
	stream, err := h.trainer.WatchTraining(ctx, id)
	if err != nil {
		h.upstreamError(w, "watch training", err)
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

	// Recv blocks, so it runs in its own goroutine; cancel() unblocks it.
	metrics := make(chan *score.TrainingMetrics)
	ended := make(chan struct{})
	go func() {
		defer close(ended)
		for {
			m, err := stream.Recv()
			if err != nil {
				return
			}
			select {
			case metrics <- m:
			case <-ctx.Done():
				return
			}
		}
	}()

	heartbeat := time.NewTicker(sseHeartbeat)
	defer heartbeat.Stop()
	for {
		var err error
		select {
		case <-ctx.Done():
			return
		case m := <-metrics:
			var data []byte
			if data, err = json.Marshal(NewTrainingMetrics(m)); err == nil {
				_, err = fmt.Fprintf(w, "event: metrics\ndata: %s\n\n", data)
			}
		case <-ended:
			status := "unknown"
			endCtx, cancelEnd := context.WithTimeout(r.Context(), upstreamTimeout)
			if run, err := h.trainer.TrainingRun(endCtx, id); err == nil {
				status = performance.TrainingStatusName(run.GetStatus())
			}
			cancelEnd()
			_, _ = fmt.Fprintf(w, "event: end\ndata: {\"status\":%q}\n\n", status)
			_ = rc.Flush()
			return
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
