//! Census: aggregated statistics of one world, and a bounded history of it.
//!
//! [`census`] summarizes the current state: per-voice aggregates, trait and
//! energy histograms, and a density grid. [`CensusRecorder`] is fed every
//! beat of a live world and keeps one [`Sample`] per [`WINDOW`] beats, up to
//! [`HISTORY`] samples, without allocating after construction.

use std::collections::VecDeque;

use crate::voice::{GENOME_SPREAD, Genome, Voice};
use crate::world::{MAX_ENERGY, Quartetling, World};

/// Bins per histogram.
pub const HISTOGRAM_BINS: usize = 16;
/// Density grid cells per side.
pub const GRID: usize = 20;
/// Beats per history sample.
pub const WINDOW: u32 = 10;
/// History samples kept (720 × 10 beats = 8 minutes at 15 beats/s).
pub const HISTORY: usize = 720;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TraitStats {
    pub mean: f32,
    pub std: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceCensus {
    pub voice: Voice,
    pub population: u32,
    pub mean_energy: f32,
    pub mean_age: f32,
    pub tempo: TraitStats,
    pub pitch: TraitStats,
    pub dynamics: TraitStats,
    pub singing: u32,
    pub in_chord: u32,
    pub mean_generation: f32,
    pub max_generation: u32,
}

/// Counts per voice over equal-width bins: `counts[voice * bins + bin]`.
/// Values outside `[min, max]` go to the nearest end bin.
#[derive(Clone, Debug, PartialEq)]
pub struct Histogram {
    pub min: f32,
    pub max: f32,
    pub bins: usize,
    pub counts: Vec<u32>,
}

impl Histogram {
    fn new(min: f32, max: f32) -> Histogram {
        Histogram {
            min,
            max,
            bins: HISTOGRAM_BINS,
            counts: vec![0; 4 * HISTOGRAM_BINS],
        }
    }

    fn add(&mut self, voice: Voice, value: f32) {
        let t = (value - self.min) / (self.max - self.min);
        let bin = ((t * self.bins as f32) as isize).clamp(0, self.bins as isize - 1) as usize;
        self.counts[voice.index() * self.bins + bin] += 1;
    }
}

/// The current state of one world.
#[derive(Clone, Debug, PartialEq)]
pub struct Census {
    pub beat: u64,
    pub slots: usize,
    /// In `Voice::ALL` order.
    pub voices: [VoiceCensus; 4],
    pub tempo: Histogram,
    pub pitch: Histogram,
    pub dynamics: Histogram,
    pub energy: Histogram,
    /// `GRID × GRID`, `counts[row * GRID + column]`, row 0 at y = 0.
    pub density: Vec<u32>,
    /// Singing quartetlings per cell, same layout.
    pub singing_density: Vec<u32>,
    /// Distinct founder lineages with a living member.
    pub lineages: u32,
}

/// The range a trait can take across every voice (see [`GENOME_SPREAD`]).
fn trait_range(get: fn(&Genome) -> f32) -> (f32, f32) {
    let values = Voice::ALL.map(|v| get(&v.base()));
    let lo = values.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    (lo * (1.0 - GENOME_SPREAD), hi * (1.0 + GENOME_SPREAD))
}

fn stats(values: impl Iterator<Item = f32> + Clone) -> TraitStats {
    let (n, sum) = values
        .clone()
        .fold((0usize, 0.0f64), |(n, s), v| (n + 1, s + v as f64));
    if n == 0 {
        return TraitStats::default();
    }
    let mean = sum / n as f64;
    let var = values.map(|v| (v as f64 - mean).powi(2)).sum::<f64>() / n as f64;
    TraitStats {
        mean: mean as f32,
        std: var.sqrt() as f32,
    }
}

/// Summarize `world` as it is now.
pub fn census(world: &World) -> Census {
    let qs = world.quartetlings();
    let cfg = world.config();

    let voices = Voice::ALL.map(|voice| {
        let of_voice = qs.iter().filter(move |q| q.voice == voice);
        let n = of_voice.clone().count();
        let mean = |f: fn(&crate::world::Quartetling) -> f32| {
            if n == 0 {
                0.0
            } else {
                of_voice.clone().map(f).sum::<f32>() / n as f32
            }
        };
        VoiceCensus {
            voice,
            population: n as u32,
            mean_energy: mean(|q| q.energy),
            mean_age: mean(|q| q.age as f32),
            tempo: stats(of_voice.clone().map(|q| q.genome.tempo)),
            pitch: stats(of_voice.clone().map(|q| q.genome.pitch)),
            dynamics: stats(of_voice.clone().map(|q| q.genome.dynamics)),
            singing: of_voice.clone().filter(|q| q.singing).count() as u32,
            in_chord: of_voice.clone().filter(|q| q.in_chord).count() as u32,
            mean_generation: mean(|q| q.generation as f32),
            max_generation: of_voice.clone().map(|q| q.generation).max().unwrap_or(0),
        }
    });

    let range = |get| {
        let (lo, hi) = trait_range(get);
        Histogram::new(lo, hi)
    };
    let mut tempo = range(|g| g.tempo);
    let mut pitch = range(|g| g.pitch);
    let mut dynamics = range(|g| g.dynamics);
    let mut energy = Histogram::new(0.0, MAX_ENERGY);
    let mut density = vec![0; GRID * GRID];
    let mut singing_density = vec![0; GRID * GRID];
    let cell = |v: f32, size: f32| ((v / size * GRID as f32) as usize).min(GRID - 1);
    for q in qs {
        tempo.add(q.voice, q.genome.tempo);
        pitch.add(q.voice, q.genome.pitch);
        dynamics.add(q.voice, q.genome.dynamics);
        energy.add(q.voice, q.energy);
        let i = cell(q.y, cfg.height) * GRID + cell(q.x, cfg.width);
        density[i] += 1;
        if q.singing {
            singing_density[i] += 1;
        }
    }

    Census {
        beat: world.beat(),
        slots: qs.len(),
        voices,
        tempo,
        pitch,
        dynamics,
        energy,
        density,
        singing_density,
        lineages: count_lineages(qs, &mut Vec::new()),
    }
}

/// One window of history.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    /// The last beat in the window.
    pub beat: u64,
    /// Beats in the window.
    pub beats: u32,
    /// Summed over the window.
    pub deaths: u32,
    pub motes_eaten: u32,
    pub chord_members: u32,
    pub singing: u32,
    /// At the window's last beat, in `Voice::ALL` order.
    pub mean_energy: [f32; 4],
    /// Summed over the window: slots refilled by an offspring.
    pub births: u32,
    /// At the window's last beat, per voice: trait means and generation depth.
    pub mean_tempo: [f32; 4],
    pub mean_pitch: [f32; 4],
    pub mean_dynamics: [f32; 4],
    pub mean_generation: [f32; 4],
    pub max_generation: u32,
    pub lineages: u32,
}

/// Distinct `lineage` values, using `scratch` to avoid allocating per call.
fn count_lineages(qs: &[Quartetling], scratch: &mut Vec<u64>) -> u32 {
    scratch.clear();
    scratch.extend(qs.iter().map(|q| q.lineage));
    scratch.sort_unstable();
    scratch.dedup();
    scratch.len() as u32
}

/// Accumulates a live world's history, one [`Sample`] per [`WINDOW`] beats.
#[derive(Clone, Debug)]
pub struct CensusRecorder {
    history: VecDeque<Sample>,
    current: Sample,
    lineage_scratch: Vec<u64>,
}

impl Default for CensusRecorder {
    fn default() -> Self {
        CensusRecorder {
            history: VecDeque::with_capacity(HISTORY),
            current: Sample::default(),
            lineage_scratch: Vec::new(),
        }
    }
}

impl CensusRecorder {
    /// Record the beat `world` just stepped. Call once after every step.
    pub fn record(&mut self, world: &World) {
        let stats = world.last_result().stats;
        let c = &mut self.current;
        c.beat = world.beat();
        c.beats += 1;
        c.deaths += stats.deaths;
        c.motes_eaten += stats.motes_eaten;
        c.chord_members += stats.chord_members;
        c.births += stats.births;
        c.singing += world.quartetlings().iter().filter(|q| q.singing).count() as u32;
        if c.beats < WINDOW {
            return;
        }
        // Per voice: count, energy, tempo, pitch, dynamics, generation.
        let mut sums = [[0.0f32; 6]; 4];
        for q in world.quartetlings() {
            let s = &mut sums[q.voice.index()];
            s[0] += 1.0;
            s[1] += q.energy;
            s[2] += q.genome.tempo;
            s[3] += q.genome.pitch;
            s[4] += q.genome.dynamics;
            s[5] += q.generation as f32;
            c.max_generation = c.max_generation.max(q.generation);
        }
        let mean = |k: usize| sums.map(|s| if s[0] == 0.0 { 0.0 } else { s[k] / s[0] });
        c.mean_energy = mean(1);
        c.mean_tempo = mean(2);
        c.mean_pitch = mean(3);
        c.mean_dynamics = mean(4);
        c.mean_generation = mean(5);
        c.lineages = count_lineages(world.quartetlings(), &mut self.lineage_scratch);
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(*c);
        *c = Sample::default();
    }

    /// Completed samples, oldest first.
    pub fn history(&self) -> impl ExactSizeIterator<Item = &Sample> {
        self.history.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Action, WorldConfig};

    fn world() -> World {
        World::new(WorldConfig {
            seed: 4,
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn counts_add_up() {
        let w = world();
        let c = census(&w);
        assert_eq!(c.slots, 64);
        assert!(c.voices.iter().all(|v| v.population == 16));
        for h in [&c.tempo, &c.pitch, &c.dynamics, &c.energy] {
            assert_eq!(h.counts.len(), 4 * HISTOGRAM_BINS);
            assert_eq!(h.counts.iter().sum::<u32>(), 64);
            // Each voice's row holds exactly its own quartetlings.
            for v in 0..4 {
                let row = &h.counts[v * h.bins..(v + 1) * h.bins];
                assert_eq!(row.iter().sum::<u32>(), 16);
            }
        }
        assert_eq!(c.density.iter().sum::<u32>(), 64);
        assert_eq!(
            c.voices[0].mean_energy, 1.0,
            "founders start with full energy"
        );
        assert_eq!(c.lineages, 64, "every founder is its own lineage");
    }

    #[test]
    fn trait_stats_match_the_voice() {
        let w = world();
        let c = census(&w);
        let soprano = c.voices[0];
        let base = Voice::Soprano.base();
        assert!((soprano.tempo.mean - base.tempo).abs() < base.tempo * GENOME_SPREAD);
        assert!(soprano.tempo.std > 0.0);
        // Sopranos are faster than basses on average.
        assert!(soprano.tempo.mean > c.voices[3].tempo.mean);
    }

    #[test]
    fn recorder_keeps_windows() {
        let mut w = world();
        let mut rec = CensusRecorder::default();
        let actions = vec![
            Action {
                turn: 0.1,
                thrust: 0.5,
                sing: true,
            };
            64
        ];
        for _ in 0..(WINDOW * 3 + 4) {
            w.step(&actions).unwrap();
            rec.record(&w);
        }
        let samples: Vec<_> = rec.history().copied().collect();
        assert_eq!(
            samples.len(),
            3,
            "the partial fourth window is not reported"
        );
        assert_eq!(samples[0].beat, WINDOW as u64);
        assert_eq!(samples[2].beat, 3 * WINDOW as u64);
        assert!(samples.iter().all(|s| s.beats == WINDOW && s.singing > 0));
    }

    #[test]
    fn recorder_is_bounded() {
        let mut w = World::new(WorldConfig {
            seed: 1,
            slots: 4,
            ..Default::default()
        })
        .unwrap();
        let mut rec = CensusRecorder::default();
        let actions = vec![Action::default(); 4];
        for _ in 0..(WINDOW as usize * (HISTORY + 5)) {
            w.step(&actions).unwrap();
            rec.record(&w);
        }
        assert_eq!(rec.history().len(), HISTORY);
        let first = rec.history().next().unwrap();
        assert_eq!(first.beat, 6 * WINDOW as u64);
    }
}
