package api

import (
	"errors"
	"math"
	"net/http"

	"github.com/go-chi/chi/v5"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
	"github.com/ericgtkb/quartet/rhythm/internal/performance"
)

// CensusResponse is the body of GET /api/performances/{id}/census. Voice
// indexes are 0 soprano, 1 alto, 2 tenor, 3 bass, as in frames.
type CensusResponse struct {
	Beat       uint64        `json:"beat"`
	Slots      uint32        `json:"slots"`
	Voices     []VoiceCensus `json:"voices"`
	Histograms Histograms    `json:"histograms"`
	Density    Density       `json:"density"`
	History    CensusHistory `json:"history"`
	// Lineages counts distinct founder lineages with a living member.
	Lineages uint32 `json:"lineages"`
}

// VoiceCensus aggregates the quartetlings of one voice.
type VoiceCensus struct {
	Voice          int8       `json:"voice"`
	Population     uint32     `json:"population"`
	MeanEnergy     float64    `json:"meanEnergy"`
	MeanAge        float64    `json:"meanAge"`
	Tempo          TraitStats `json:"tempo"`
	Pitch          TraitStats `json:"pitch"`
	Dynamics       TraitStats `json:"dynamics"`
	Singing        uint32     `json:"singing"`
	InChord        uint32     `json:"inChord"`
	MeanGeneration float64    `json:"meanGeneration"`
	MaxGeneration  uint32     `json:"maxGeneration"`
}

// TraitStats is a trait's mean and standard deviation.
type TraitStats struct {
	Mean float64 `json:"mean"`
	Std  float64 `json:"std"`
}

// Histograms holds one histogram per trait, plus energy.
type Histograms struct {
	Tempo    Histogram `json:"tempo"`
	Pitch    Histogram `json:"pitch"`
	Dynamics Histogram `json:"dynamics"`
	Energy   Histogram `json:"energy"`
}

// Histogram has equal-width bins over [min, max]; counts[voice][bin].
type Histogram struct {
	Min    float64    `json:"min"`
	Max    float64    `json:"max"`
	Counts [][]uint32 `json:"counts"`
}

// Density counts quartetlings per grid cell, counts[row*columns+column],
// row 0 at y = 0.
type Density struct {
	Columns uint32   `json:"columns"`
	Rows    uint32   `json:"rows"`
	Counts  []uint32 `json:"counts"`
	Singing []uint32 `json:"singing"`
}

// CensusHistory is recent history as parallel arrays, one entry per window
// of `beats` beats, oldest first. Per-voice series are [voice][sample].
type CensusHistory struct {
	Beat           []uint64    `json:"beat"`
	Beats          []uint32    `json:"beats"`
	Deaths         []uint32    `json:"deaths"`
	MotesEaten     []uint32    `json:"motesEaten"`
	ChordMembers   []uint32    `json:"chordMembers"`
	Singing        []uint32    `json:"singing"`
	Births         []uint32    `json:"births"`
	MaxGeneration  []uint32    `json:"maxGeneration"`
	Lineages       []uint32    `json:"lineages"`
	MeanEnergy     [][]float64 `json:"meanEnergy"`
	MeanTempo      [][]float64 `json:"meanTempo"`
	MeanPitch      [][]float64 `json:"meanPitch"`
	MeanDynamics   [][]float64 `json:"meanDynamics"`
	MeanGeneration [][]float64 `json:"meanGeneration"`
}

// num rounds to four decimals and maps NaN and ±Inf to 0, which JSON can't carry.
func num(v float32) float64 {
	f := float64(v)
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return 0
	}
	return math.Round(f*1e4) / 1e4
}

func traitStats(t *score.TraitStats) TraitStats {
	return TraitStats{Mean: num(t.GetMean()), Std: num(t.GetStd())}
}

func histogram(h *score.Histogram) Histogram {
	bins := int(h.GetBins())
	counts := make([][]uint32, 4)
	for v := range counts {
		counts[v] = []uint32{}
		if bins > 0 && (v+1)*bins <= len(h.GetCounts()) {
			counts[v] = h.GetCounts()[v*bins : (v+1)*bins]
		}
	}
	return Histogram{Min: num(h.GetMin()), Max: num(h.GetMax()), Counts: counts}
}

// NewCensus converts harmony's census to the public shape.
func NewCensus(c *score.Census) CensusResponse {
	resp := CensusResponse{
		Beat:   c.GetBeat(),
		Slots:  c.GetSlots(),
		Voices: make([]VoiceCensus, 0, len(c.GetVoices())),
		Histograms: Histograms{
			Tempo:    histogram(c.GetTempo()),
			Pitch:    histogram(c.GetPitch()),
			Dynamics: histogram(c.GetDynamics()),
			Energy:   histogram(c.GetEnergy()),
		},
		Density: Density{
			Columns: c.GetDensity().GetColumns(),
			Rows:    c.GetDensity().GetRows(),
			Counts:  append([]uint32{}, c.GetDensity().GetCounts()...),
			Singing: append([]uint32{}, c.GetDensity().GetSinging()...),
		},
	}
	for _, v := range c.GetVoices() {
		resp.Voices = append(resp.Voices, VoiceCensus{
			Voice:          int8(v.GetVoice()) - 1, // proto enum is 1-based
			Population:     v.GetPopulation(),
			MeanEnergy:     num(v.GetMeanEnergy()),
			MeanAge:        num(v.GetMeanAge()),
			Tempo:          traitStats(v.GetTempo()),
			Pitch:          traitStats(v.GetPitch()),
			Dynamics:       traitStats(v.GetDynamics()),
			Singing:        v.GetSinging(),
			InChord:        v.GetInChord(),
			MeanGeneration: num(v.GetMeanGeneration()),
			MaxGeneration:  v.GetMaxGeneration(),
		})
	}
	n := len(c.GetHistory())
	perVoice := func() [][]float64 {
		out := make([][]float64, 4)
		for v := range out {
			out[v] = make([]float64, n)
		}
		return out
	}
	h := CensusHistory{
		Beat: make([]uint64, n), Beats: make([]uint32, n), Deaths: make([]uint32, n),
		MotesEaten: make([]uint32, n), ChordMembers: make([]uint32, n), Singing: make([]uint32, n),
		Births: make([]uint32, n), MaxGeneration: make([]uint32, n), Lineages: make([]uint32, n),
		MeanEnergy: perVoice(), MeanTempo: perVoice(), MeanPitch: perVoice(), MeanDynamics: perVoice(),
		MeanGeneration: perVoice(),
	}
	fill := func(dst [][]float64, i int, src []float32) {
		for v, x := range src {
			if v < 4 {
				dst[v][i] = num(x)
			}
		}
	}
	for i, s := range c.GetHistory() {
		h.Beat[i], h.Beats[i], h.Deaths[i] = s.GetBeat(), s.GetBeats(), s.GetDeaths()
		h.MotesEaten[i], h.ChordMembers[i], h.Singing[i] = s.GetMotesEaten(), s.GetChordMembers(), s.GetSinging()
		h.Births[i], h.MaxGeneration[i], h.Lineages[i] = s.GetBirths(), s.GetMaxGeneration(), s.GetLineages()
		fill(h.MeanEnergy, i, s.GetMeanEnergy())
		fill(h.MeanTempo, i, s.GetMeanTempo())
		fill(h.MeanPitch, i, s.GetMeanPitch())
		fill(h.MeanDynamics, i, s.GetMeanDynamics())
		fill(h.MeanGeneration, i, s.GetMeanGeneration())
	}
	resp.History = h
	resp.Lineages = c.GetLineages()
	return resp
}

func (h *handlers) census(w http.ResponseWriter, r *http.Request) {
	c, err := h.perfs.Census(r.Context(), chi.URLParam(r, "id"))
	switch {
	case errors.Is(err, performance.ErrNotFound):
		writeError(w, http.StatusNotFound, err.Error())
	case err != nil:
		h.log.Warn("census", "err", err)
		writeError(w, http.StatusBadGateway, err.Error())
	default:
		writeJSON(w, http.StatusOK, NewCensus(c))
	}
}
