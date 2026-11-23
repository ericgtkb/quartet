package upstream

import (
	"context"
	"net"
	"testing"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/health"
	healthpb "google.golang.org/grpc/health/grpc_health_v1"
	"google.golang.org/grpc/test/bufconn"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
)

type fakeHarmony struct {
	score.UnimplementedHarmonyServiceServer
}

func (fakeHarmony) GetServiceInfo(context.Context, *score.GetServiceInfoRequest) (*score.GetServiceInfoResponse, error) {
	return &score.GetServiceInfoResponse{Info: &score.ServiceInfo{Name: "harmony", Version: "9.9.9"}}, nil
}

// dial starts an in-memory gRPC server with a fake harmony service and the
// given health status, and returns a client connection to it.
func dial(t *testing.T, status healthpb.HealthCheckResponse_ServingStatus) *grpc.ClientConn {
	t.Helper()
	lis := bufconn.Listen(1 << 20)
	srv := grpc.NewServer()
	score.RegisterHarmonyServiceServer(srv, fakeHarmony{})
	hs := health.NewServer()
	hs.SetServingStatus("", status)
	healthpb.RegisterHealthServer(srv, hs)
	go func() { _ = srv.Serve(lis) }()
	t.Cleanup(srv.Stop)

	conn, err := grpc.NewClient("passthrough:///bufnet",
		grpc.WithContextDialer(func(ctx context.Context, _ string) (net.Conn, error) { return lis.DialContext(ctx) }),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		t.Fatalf("dial: %v", err)
	}
	t.Cleanup(func() { _ = conn.Close() })
	return conn
}

func TestServiceInfoAndHealth(t *testing.T) {
	h := NewHarmony(dial(t, healthpb.HealthCheckResponse_SERVING))

	info, err := h.Info(t.Context())
	if err != nil {
		t.Fatalf("Info: %v", err)
	}
	if info.GetName() != "harmony" || info.GetVersion() != "9.9.9" {
		t.Errorf("Info = %v", info)
	}
	if err := h.Health(t.Context()); err != nil {
		t.Errorf("Health: %v", err)
	}
}

func TestHealthNotServing(t *testing.T) {
	h := NewHarmony(dial(t, healthpb.HealthCheckResponse_NOT_SERVING))
	if err := h.Health(t.Context()); err == nil {
		t.Error("Health: want error for NOT_SERVING, got nil")
	}
}
