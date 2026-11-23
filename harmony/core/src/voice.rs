//! Voices and genomes.

use crate::rng::Rng;

/// A quartetling's fixed role. It sets the base traits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Voice {
    Soprano,
    Alto,
    Tenor,
    Bass,
}

impl Voice {
    pub const ALL: [Voice; 4] = [Voice::Soprano, Voice::Alto, Voice::Tenor, Voice::Bass];

    /// Index in `ALL`, also the one-hot position in observations.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The voice for agent slot `slot`. Slots cycle through the voices, so a
    /// world with a multiple of four slots has equal numbers of each.
    pub fn for_slot(slot: usize) -> Voice {
        Voice::ALL[slot % 4]
    }

    /// Base traits. Sopranos are fast, hear far, and are small; basses are slow,
    /// hear less, and are large.
    pub fn base(self) -> Genome {
        match self {
            Voice::Soprano => Genome {
                tempo: 1.6,
                pitch: 18.0,
                dynamics: 0.7,
            },
            Voice::Alto => Genome {
                tempo: 1.35,
                pitch: 15.5,
                dynamics: 0.9,
            },
            Voice::Tenor => Genome {
                tempo: 1.15,
                pitch: 13.5,
                dynamics: 1.1,
            },
            Voice::Bass => Genome {
                tempo: 0.95,
                pitch: 11.5,
                dynamics: 1.3,
            },
        }
    }
}

/// Per-quartetling traits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Genome {
    /// Max speed in world units per beat. Moving fast costs more energy.
    pub tempo: f32,
    /// Hearing radius: how far it perceives others, motes, and static.
    pub pitch: f32,
    /// Body size: radius for eating, and a multiplier on energy costs.
    pub dynamics: f32,
}

/// Genomes vary by up to this fraction around their voice's base traits.
/// Policies are trained on this whole range (see README, Evolution).
pub const GENOME_SPREAD: f32 = 0.2;

/// Standard deviation of an offspring's relative change in each trait.
pub const MUTATION: f32 = 0.03;

impl Genome {
    /// The trained range of each trait for `voice`: `(min, max)` genomes.
    pub fn bounds(voice: Voice) -> (Genome, Genome) {
        let b = voice.base();
        let scale = |f: f32| Genome {
            tempo: b.tempo * f,
            pitch: b.pitch * f,
            dynamics: b.dynamics * f,
        };
        (scale(1.0 - GENOME_SPREAD), scale(1.0 + GENOME_SPREAD))
    }

    /// An offspring's genome: each trait changes by a relative Gaussian step
    /// of `MUTATION`, clipped to the range policies were trained on.
    pub fn mutate(&self, voice: Voice, rng: &mut Rng) -> Genome {
        let (lo, hi) = Genome::bounds(voice);
        let mut step =
            |v: f32, lo: f32, hi: f32| (v * (1.0 + MUTATION * rng.normal())).clamp(lo, hi);
        Genome {
            tempo: step(self.tempo, lo.tempo, hi.tempo),
            pitch: step(self.pitch, lo.pitch, hi.pitch),
            dynamics: step(self.dynamics, lo.dynamics, hi.dynamics),
        }
    }

    /// A random genome for `voice`, uniform within `GENOME_SPREAD`.
    pub fn sample(voice: Voice, rng: &mut Rng) -> Genome {
        let base = voice.base();
        let mut vary = |v: f32| v * rng.range(1.0 - GENOME_SPREAD, 1.0 + GENOME_SPREAD);
        Genome {
            tempo: vary(base.tempo),
            pitch: vary(base.pitch),
            dynamics: vary(base.dynamics),
        }
    }
}
