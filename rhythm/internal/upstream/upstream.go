// Package upstream wraps rhythm's gRPC connections to the internal services.
package upstream

import (
	"context"
	"fmt"

	"google.golang.org/grpc"
	healthpb "google.golang.org/grpc/health/grpc_health_v1"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
)

type infoFunc func(context.Context, *score.GetServiceInfoRequest, ...grpc.CallOption) (*score.GetServiceInfoResponse, error)

// Service is one internal gRPC service: its self-description and its standard
// gRPC health check.
type Service struct {
	name   string
	info   infoFunc
	health healthpb.HealthClient
}

// Harmony is the harmony (Rust world engine) upstream: service info and
// health, plus live world operations.
type Harmony struct {
	*Service
	client score.HarmonyServiceClient
}

// NewHarmony returns the harmony upstream.
func NewHarmony(conn grpc.ClientConnInterface) *Harmony {
	client := score.NewHarmonyServiceClient(conn)
	return &Harmony{
		Service: &Service{
			name:   "harmony",
			info:   client.GetServiceInfo,
			health: healthpb.NewHealthClient(conn),
		},
		client: client,
	}
}

// CreateWorld creates a live world. With includeObservations, the response
// carries the initial observations for an external policy.
func (h *Harmony) CreateWorld(ctx context.Context, cfg *score.WorldConfig, includeObservations bool) (*score.CreateWorldResponse, error) {
	resp, err := h.client.CreateWorld(ctx, &score.CreateWorldRequest{Config: cfg, IncludeObservations: includeObservations})
	if err != nil {
		return nil, fmt.Errorf("harmony: CreateWorld: %w", err)
	}
	return resp, nil
}

// StepWorld advances a live world one beat.
func (h *Harmony) StepWorld(ctx context.Context, req *score.StepWorldRequest) (*score.StepWorldResponse, error) {
	resp, err := h.client.StepWorld(ctx, req)
	if err != nil {
		return nil, fmt.Errorf("harmony: StepWorld %s: %w", req.GetWorldId(), err)
	}
	return resp, nil
}

// DeleteWorld removes a live world.
func (h *Harmony) DeleteWorld(ctx context.Context, worldID string) error {
	if _, err := h.client.DeleteWorld(ctx, &score.DeleteWorldRequest{WorldId: worldID}); err != nil {
		return fmt.Errorf("harmony: DeleteWorld %s: %w", worldID, err)
	}
	return nil
}

// Census returns a live world's aggregated statistics.
func (h *Harmony) Census(ctx context.Context, worldID string) (*score.Census, error) {
	resp, err := h.client.GetCensus(ctx, &score.GetCensusRequest{WorldId: worldID})
	if err != nil {
		return nil, fmt.Errorf("harmony: GetCensus %s: %w", worldID, err)
	}
	return resp.GetCensus(), nil
}

// Melody is the melody (Python learning service) upstream: service info and
// health, trained-policy inference, and training runs.
type Melody struct {
	*Service
	client score.MelodyServiceClient
}

// NewMelody returns the melody upstream.
func NewMelody(conn grpc.ClientConnInterface) *Melody {
	client := score.NewMelodyServiceClient(conn)
	return &Melody{
		Service: &Service{
			name:   "melody",
			info:   client.GetServiceInfo,
			health: healthpb.NewHealthClient(conn),
		},
		client: client,
	}
}

// Policies lists trained policies, newest first.
func (m *Melody) Policies(ctx context.Context) ([]*score.PolicyInfo, error) {
	resp, err := m.client.ListPolicies(ctx, &score.ListPoliciesRequest{})
	if err != nil {
		return nil, fmt.Errorf("melody: ListPolicies: %w", err)
	}
	return resp.GetPolicies(), nil
}

// Act runs one beat of inference. The returned error keeps melody's gRPC
// status (NotFound, FailedPrecondition, ...) for status.Code.
func (m *Melody) Act(ctx context.Context, policy string, observations *score.Tensor, observationVersion uint32) (*score.Tensor, error) {
	resp, err := m.client.Act(ctx, &score.ActRequest{
		Policy: policy, Observations: observations, ObservationVersion: observationVersion,
	})
	if err != nil {
		return nil, fmt.Errorf("melody: Act %s: %w", policy, err)
	}
	return resp.GetActions(), nil
}

// StartTraining starts a training run.
func (m *Melody) StartTraining(ctx context.Context, cfg *score.TrainingConfig) (*score.TrainingRun, error) {
	resp, err := m.client.StartTraining(ctx, &score.StartTrainingRequest{Config: cfg})
	if err != nil {
		return nil, fmt.Errorf("melody: StartTraining: %w", err)
	}
	return resp.GetRun(), nil
}

// TrainingRuns lists training runs, newest first.
func (m *Melody) TrainingRuns(ctx context.Context) ([]*score.TrainingRun, error) {
	resp, err := m.client.ListTrainingRuns(ctx, &score.ListTrainingRunsRequest{})
	if err != nil {
		return nil, fmt.Errorf("melody: ListTrainingRuns: %w", err)
	}
	return resp.GetRuns(), nil
}

// TrainingRun describes one training run.
func (m *Melody) TrainingRun(ctx context.Context, id string) (*score.TrainingRun, error) {
	resp, err := m.client.GetTrainingRun(ctx, &score.GetTrainingRunRequest{Id: id})
	if err != nil {
		return nil, fmt.Errorf("melody: GetTrainingRun %s: %w", id, err)
	}
	return resp.GetRun(), nil
}

// StopTraining asks a run to stop; it saves a checkpoint and ends shortly after.
func (m *Melody) StopTraining(ctx context.Context, id string) (*score.TrainingRun, error) {
	resp, err := m.client.StopTraining(ctx, &score.StopTrainingRequest{Id: id})
	if err != nil {
		return nil, fmt.Errorf("melody: StopTraining %s: %w", id, err)
	}
	return resp.GetRun(), nil
}

// MetricsStream yields a training run's metrics; Recv returns io.EOF when
// the run ends.
type MetricsStream interface {
	Recv() (*score.TrainingMetrics, error)
}

// WatchTraining streams a run's metrics: its history first, then live
// updates until it ends. Cancel ctx to stop watching.
func (m *Melody) WatchTraining(ctx context.Context, id string) (MetricsStream, error) {
	stream, err := m.client.WatchTraining(ctx, &score.WatchTrainingRequest{Id: id})
	if err != nil {
		return nil, fmt.Errorf("melody: WatchTraining %s: %w", id, err)
	}
	return stream, nil
}

// Name is the component name, e.g. "harmony".
func (s *Service) Name() string { return s.name }

// Info calls the service's GetServiceInfo RPC.
func (s *Service) Info(ctx context.Context) (*score.ServiceInfo, error) {
	resp, err := s.info(ctx, &score.GetServiceInfoRequest{})
	if err != nil {
		return nil, fmt.Errorf("%s: GetServiceInfo: %w", s.name, err)
	}
	return resp.GetInfo(), nil
}

// Health calls the standard gRPC health check for the whole server.
// It returns nil only when the server reports SERVING.
func (s *Service) Health(ctx context.Context) error {
	resp, err := s.health.Check(ctx, &healthpb.HealthCheckRequest{})
	if err != nil {
		return fmt.Errorf("%s: health check: %w", s.name, err)
	}
	if st := resp.GetStatus(); st != healthpb.HealthCheckResponse_SERVING {
		return fmt.Errorf("%s: health status %s", s.name, st)
	}
	return nil
}
