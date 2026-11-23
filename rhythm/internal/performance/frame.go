package performance

import (
	"encoding/json"
	"math"

	score "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score"
)

// Frame is one beat as sent to browsers. Per-quartetling fields are
// struct-of-arrays (index = slot), which is compact and maps directly onto
// instanced rendering. Mirrored in timbre/src/api/types.ts.
type Frame struct {
	Beat         uint64           `json:"beat"`
	Width        float32          `json:"width"`
	Height       float32          `json:"height"`
	Quartetlings QuartetlingArray `json:"quartetlings"`
	Motes        Points           `json:"motes"`
	Statics      Statics          `json:"statics"`
	Stats        Stats            `json:"stats"`
}

// QuartetlingArray holds one entry per slot in each field.
type QuartetlingArray struct {
	ID         []uint64  `json:"id"`
	Voice      []int8    `json:"voice"` // 0 soprano, 1 alto, 2 tenor, 3 bass
	X          []float64 `json:"x"`
	Y          []float64 `json:"y"`
	Heading    []float64 `json:"heading"`
	Energy     []float64 `json:"energy"`
	Breath     []float64 `json:"breath"`
	Singing    []int8    `json:"singing"` // 0 or 1
	InChord    []int8    `json:"inChord"` // 0 or 1
	Age        []uint32  `json:"age"`
	Generation []uint32  `json:"generation"`
}

// Points holds positions as parallel arrays.
type Points struct {
	X []float64 `json:"x"`
	Y []float64 `json:"y"`
}

// Statics holds static fields as parallel arrays.
type Statics struct {
	X      []float64 `json:"x"`
	Y      []float64 `json:"y"`
	Radius []float64 `json:"radius"`
}

// Stats are the counters for one beat.
type Stats struct {
	MotesEaten   uint32 `json:"motesEaten"`
	Deaths       uint32 `json:"deaths"`
	ChordMembers uint32 `json:"chordMembers"`
	Births       uint32 `json:"births"`
}

// round3 keeps three decimals, which is plenty for rendering and keeps the
// JSON short.
func round3(v float32) float64 {
	return math.Round(float64(v)*1000) / 1000
}

func boolInt(b bool) int8 {
	if b {
		return 1
	}
	return 0
}

// NewFrame converts a snapshot into a frame.
func NewFrame(s *score.WorldSnapshot) Frame {
	qs := s.GetQuartetlings()
	n := len(qs)
	f := Frame{
		Beat:   s.GetBeat(),
		Width:  s.GetWidth(),
		Height: s.GetHeight(),
		Quartetlings: QuartetlingArray{
			ID: make([]uint64, n), Voice: make([]int8, n), X: make([]float64, n), Y: make([]float64, n),
			Heading: make([]float64, n), Energy: make([]float64, n), Breath: make([]float64, n),
			Singing: make([]int8, n), InChord: make([]int8, n), Age: make([]uint32, n), Generation: make([]uint32, n),
		},
		Motes:   Points{X: make([]float64, 0, len(s.GetMotes())), Y: make([]float64, 0, len(s.GetMotes()))},
		Statics: Statics{X: []float64{}, Y: []float64{}, Radius: []float64{}},
		Stats: Stats{
			MotesEaten:   s.GetStats().GetMotesEaten(),
			Deaths:       s.GetStats().GetDeaths(),
			ChordMembers: s.GetStats().GetChordMembers(),
			Births:       s.GetStats().GetBirths(),
		},
	}
	a := &f.Quartetlings
	for i, q := range qs {
		a.ID[i] = q.GetId()
		a.Voice[i] = int8(q.GetVoice()) - 1 // proto enum is 1-based (0 = unspecified)
		a.X[i] = round3(q.GetX())
		a.Y[i] = round3(q.GetY())
		a.Heading[i] = round3(q.GetHeading())
		a.Energy[i] = round3(q.GetEnergy())
		a.Breath[i] = round3(q.GetBreath())
		a.Singing[i] = boolInt(q.GetSinging())
		a.InChord[i] = boolInt(q.GetInChord())
		a.Age[i] = q.GetAge()
		a.Generation[i] = q.GetGeneration()
	}
	for _, m := range s.GetMotes() {
		f.Motes.X = append(f.Motes.X, round3(m.GetX()))
		f.Motes.Y = append(f.Motes.Y, round3(m.GetY()))
	}
	for _, st := range s.GetStatics() {
		f.Statics.X = append(f.Statics.X, round3(st.GetX()))
		f.Statics.Y = append(f.Statics.Y, round3(st.GetY()))
		f.Statics.Radius = append(f.Statics.Radius, round3(st.GetRadius()))
	}
	return f
}

// EncodeFrame converts and JSON-encodes a snapshot once, for all subscribers.
func EncodeFrame(s *score.WorldSnapshot) ([]byte, error) {
	return json.Marshal(NewFrame(s))
}
