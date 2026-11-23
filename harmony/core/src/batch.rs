//! Many independent worlds stepped in parallel, for rehearsals (training).
//!
//! The batch is flat: agent `k` is slot `k % slots` of world `k / slots`.
//! Worlds are independent, so the result does not depend on thread count.

use rayon::prelude::*;

use crate::observe::{OBS_DIM, ObserveScratch, observe};
use crate::rng::Rng;
use crate::world::{ACTION_DIM, Action, BeatStats, ConfigError, StepError, World, WorldConfig};

/// Upper bound on `worlds × slots`, to keep one rehearsal's memory bounded.
pub const MAX_AGENTS: usize = 1 << 17;

/// Per-agent outputs of one batch step, reused across steps.
#[derive(Clone, Debug, Default)]
pub struct BatchOutputs {
    /// `agents × OBS_DIM`.
    pub observations: Vec<f32>,
    pub rewards: Vec<f32>,
    pub terminated: Vec<u8>,
    pub truncated: Vec<u8>,
    pub stats: BeatStats,
}

struct Lane {
    world: World,
    scratch: ObserveScratch,
    actions: Vec<Action>,
}

pub struct WorldBatch {
    lanes: Vec<Lane>,
    slots: usize,
}

impl WorldBatch {
    /// `worlds` worlds like `base`, each with its own seed derived from `base.seed`.
    pub fn new(base: &WorldConfig, worlds: usize) -> Result<WorldBatch, ConfigError> {
        base.validate()?;
        if base.evolve {
            return Err(ConfigError(
                "rehearsals don't evolve: the training distribution must stay fixed".into(),
            ));
        }
        if worlds == 0 || worlds * base.slots > MAX_AGENTS {
            return Err(ConfigError(format!(
                "worlds × slots must be in 1..={MAX_AGENTS}, got {worlds} × {}",
                base.slots
            )));
        }
        let mut seeds = Rng::new(base.seed);
        let lanes = (0..worlds)
            .map(|_| {
                let config = WorldConfig {
                    seed: seeds.next_u64(),
                    ..base.clone()
                };
                Ok(Lane {
                    world: World::new(config)?,
                    scratch: ObserveScratch::default(),
                    actions: Vec::with_capacity(base.slots),
                })
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
        Ok(WorldBatch {
            lanes,
            slots: base.slots,
        })
    }

    pub fn worlds(&self) -> usize {
        self.lanes.len()
    }

    pub fn agents(&self) -> usize {
        self.lanes.len() * self.slots
    }

    /// Size the output buffers and fill in the current observations, with zero
    /// rewards and flags.
    pub fn reset_outputs(&mut self, out: &mut BatchOutputs) {
        let agents = self.agents();
        out.observations.resize(agents * OBS_DIM, 0.0);
        out.rewards.clear();
        out.rewards.resize(agents, 0.0);
        out.terminated.clear();
        out.terminated.resize(agents, 0);
        out.truncated.clear();
        out.truncated.resize(agents, 0);
        out.stats = BeatStats::default();
        self.lanes
            .par_iter_mut()
            .zip(out.observations.par_chunks_mut(self.slots * OBS_DIM))
            .for_each(|(lane, obs)| observe(&lane.world, &mut lane.scratch, obs));
    }

    /// Step every world with `actions` (`agents × ACTION_DIM`, flat layout, see
    /// [`Action::from_flat`]) and write the results into `out`, which must have
    /// been sized by [`WorldBatch::reset_outputs`].
    pub fn step(&mut self, actions: &[f32], out: &mut BatchOutputs) -> Result<(), StepError> {
        let agents = self.agents();
        if actions.len() != agents * ACTION_DIM {
            return Err(StepError(format!(
                "expected {} action values ({agents} agents × {ACTION_DIM}), got {}",
                agents * ACTION_DIM,
                actions.len()
            )));
        }
        if out.rewards.len() != agents {
            self.reset_outputs(out);
        }
        let slots = self.slots;
        let stats = self
            .lanes
            .par_iter_mut()
            .zip(actions.par_chunks(slots * ACTION_DIM))
            .zip(out.observations.par_chunks_mut(slots * OBS_DIM))
            .zip(out.rewards.par_chunks_mut(slots))
            .zip(out.terminated.par_chunks_mut(slots))
            .zip(out.truncated.par_chunks_mut(slots))
            .map(|(((((lane, act), obs), rew), term), trunc)| {
                lane.actions.clear();
                lane.actions.extend(
                    act.as_chunks::<ACTION_DIM>()
                        .0
                        .iter()
                        .map(|a| Action::from_flat(a)),
                );
                let result = lane.world.step(&lane.actions)?;
                rew.copy_from_slice(&result.rewards);
                for (dst, &src) in term.iter_mut().zip(&result.terminated) {
                    *dst = src as u8;
                }
                for (dst, &src) in trunc.iter_mut().zip(&result.truncated) {
                    *dst = src as u8;
                }
                let stats = result.stats;
                observe(&lane.world, &mut lane.scratch, obs);
                Ok(stats)
            })
            .collect::<Result<Vec<BeatStats>, StepError>>()?;
        out.stats = stats
            .into_iter()
            .fold(BeatStats::default(), |acc, s| BeatStats {
                motes_eaten: acc.motes_eaten + s.motes_eaten,
                deaths: acc.deaths + s.deaths,
                chord_members: acc.chord_members + s.chord_members,
                energy: acc.energy + s.energy,
                births: acc.births + s.births,
            });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> WorldConfig {
        WorldConfig {
            seed: 123,
            slots: 16,
            ..Default::default()
        }
    }

    #[test]
    fn rejects_bad_sizes() {
        assert!(WorldBatch::new(&base(), 0).is_err());
        assert!(WorldBatch::new(&base(), MAX_AGENTS).is_err());
        let evolving = WorldConfig {
            evolve: true,
            ..base()
        };
        assert!(WorldBatch::new(&evolving, 1).is_err());
        let mut batch = WorldBatch::new(&base(), 2).unwrap();
        let mut out = BatchOutputs::default();
        batch.reset_outputs(&mut out);
        assert!(batch.step(&[0.0; 5], &mut out).is_err());
    }

    #[test]
    fn matches_stepping_worlds_one_by_one() {
        let worlds = 4;
        let mut batch = WorldBatch::new(&base(), worlds).unwrap();
        let mut out = BatchOutputs::default();
        batch.reset_outputs(&mut out);

        let mut seeds = Rng::new(base().seed);
        let mut singles: Vec<World> = (0..worlds)
            .map(|_| {
                World::new(WorldConfig {
                    seed: seeds.next_u64(),
                    ..base()
                })
                .unwrap()
            })
            .collect();

        let agents = batch.agents();
        let flat: Vec<f32> = (0..agents * ACTION_DIM)
            .map(|i| ((i * 7 % 11) as f32 / 5.0) - 1.0)
            .collect();
        for _ in 0..30 {
            batch.step(&flat, &mut out).unwrap();
            for (w, world) in singles.iter_mut().enumerate() {
                let start = w * 16 * ACTION_DIM;
                let actions: Vec<Action> = flat[start..start + 16 * ACTION_DIM]
                    .as_chunks::<ACTION_DIM>()
                    .0
                    .iter()
                    .map(|a| Action::from_flat(a))
                    .collect();
                let result = world.step(&actions).unwrap();
                assert_eq!(&out.rewards[w * 16..(w + 1) * 16], &result.rewards[..]);
            }
        }
        let mut expected = vec![0.0; 16 * OBS_DIM];
        observe(&singles[3], &mut ObserveScratch::default(), &mut expected);
        assert_eq!(&out.observations[3 * 16 * OBS_DIM..], &expected[..]);
    }
}
