package api

// JSON types for the public API. They are mirrored by hand in
// timbre/src/api/types.ts; change both together (see AGENTS.md).
// Performance types (performance.Info, performance.Config, performance.Frame,
// performance.Policy) live in the performance package.

// Member status values.
const (
	StatusOK          = "ok"
	StatusUnavailable = "unavailable"
)

// EnsembleResponse is the body of GET /api/ensemble.
type EnsembleResponse struct {
	Members []Member `json:"members"`
}

// Member describes one backend component as seen by rhythm.
type Member struct {
	Name    string            `json:"name"`
	Status  string            `json:"status"`
	Version string            `json:"version,omitempty"`
	Runtime string            `json:"runtime,omitempty"`
	Details map[string]string `json:"details"`
	Error   string            `json:"error,omitempty"`
}

// HealthResponse is the body of GET /api/healthz.
type HealthResponse struct {
	Status string            `json:"status"`
	Checks map[string]string `json:"checks"`
}

// ErrorResponse is the body of every error response.
type ErrorResponse struct {
	Error string `json:"error"`
}
