//! The quartetling world: one arena, a fixed number of agent slots, motes,
//! static fields, and the chord mechanic.
//!
//! A step is deterministic given the world's seed and the actions. Dead or
//! too-old quartetlings are replaced in the same slot (same voice) within the
//! step, so every slot always holds a living quartetling and tensor shapes never
//! change.

use std::error::Error;
use std::f32::consts::{PI, TAU};
use std::fmt;

use crate::rng::Rng;
use crate::voice::{Genome, Voice};

/// Max heading change per beat, in radians, at `turn = ±1`.
pub const MAX_TURN: f32 = 0.35;
/// Energy a new quartetling starts with.
pub const START_ENERGY: f32 = 1.0;
/// Energy cap.
pub const MAX_ENERGY: f32 = 2.0;
/// Energy spent per beat just by existing, times `dynamics`. High enough that a
/// stationary 3-voice chord can't sustain itself; a full 4-voice one can.
pub const BASE_METABOLISM: f32 = 0.004;
/// Energy spent per beat at full speed, times `dynamics`. Scales with speed².
pub const MOVE_COST: f32 = 0.004;
/// Energy spent per beat of singing.
pub const SING_COST: f32 = 0.004;
/// Breath used per beat of singing. A quartetling can't sing without breath.
pub const BREATH_USE: f32 = 0.04;
/// Breath recovered per beat of not singing. With `BREATH_USE`, a quartetling
/// can sing about a third of the time, so chords need synchronized breathing.
pub const BREATH_RECOVER: f32 = 0.02;
/// Singers within this distance of each other can form a chord.
pub const CHORD_RADIUS: f32 = 10.0;
/// Energy bonus per beat for a singer whose chord has 3 distinct voices.
pub const CHORD_BONUS_3: f32 = 0.012;
/// Energy bonus per beat for a singer whose chord has all 4 voices.
pub const CHORD_BONUS_4: f32 = 0.024;
/// Energy gained by eating one mote.
pub const MOTE_ENERGY: f32 = 0.2;
/// Mote radius for eating; a quartetling's body radius is its `dynamics`.
pub const MOTE_RADIUS: f32 = 0.5;
/// Energy drained per beat inside a static field.
pub const STATIC_DRAIN: f32 = 0.02;
/// Static field drift speed per beat.
pub const STATIC_SPEED: f32 = 0.15;
/// Static field radius range.
pub const STATIC_RADIUS: (f32, f32) = (8.0, 14.0);
/// A quartetling older than this many beats is retired (truncated, not dead).
pub const MAX_AGE: u32 = 3000;
/// Reward per unit of energy gained.
pub const REWARD_SCALE: f32 = 10.0;
/// Extra reward on death.
pub const DEATH_PENALTY: f32 = -1.0;

// Evolution (worlds with `evolve`, i.e. performances; see README, Evolution).
/// Age, in beats, from which a quartetling can become a parent.
pub const MATURITY: u32 = 300;
/// Energy a parent needs.
pub const REPRO_ENERGY: f32 = 1.5;
/// Energy a parent spends on an offspring (which starts with `START_ENERGY`).
pub const REPRO_COST: f32 = 0.5;
/// Offspring appear within this distance of their parent.
pub const OFFSPRING_RADIUS: f32 = 3.0;

/// World parameters. Validated by [`WorldConfig::validate`].
#[derive(Clone, Debug, PartialEq)]
pub struct WorldConfig {
    pub seed: u64,
    /// Number of agent slots; a multiple of 4 so every voice is equally represented.
    pub slots: usize,
    pub width: f32,
    pub height: f32,
    /// Number of motes; eaten motes respawn immediately elsewhere.
    pub motes: usize,
    /// Number of static fields.
    pub statics: usize,
    /// Fill vacated slots with offspring of eligible parents (performances).
    /// Rehearsals never evolve, so the training distribution stays fixed.
    pub evolve: bool,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            slots: 64,
            width: 100.0,
            height: 100.0,
            motes: 48,
            statics: 3,
            evolve: false,
        }
    }
}

impl WorldConfig {
    pub const MAX_SLOTS: usize = 1024;
    pub const MAX_MOTES: usize = 4096;
    pub const MAX_STATICS: usize = 32;
    pub const SIZE_RANGE: (f32, f32) = (20.0, 1000.0);

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.slots == 0 || !self.slots.is_multiple_of(4) || self.slots > Self::MAX_SLOTS {
            return Err(ConfigError(format!(
                "slots must be a positive multiple of 4 up to {}, got {}",
                Self::MAX_SLOTS,
                self.slots
            )));
        }
        let (lo, hi) = Self::SIZE_RANGE;
        for (name, v) in [("width", self.width), ("height", self.height)] {
            if !(lo..=hi).contains(&v) {
                return Err(ConfigError(format!(
                    "{name} must be in [{lo}, {hi}], got {v}"
                )));
            }
        }
        if self.motes > Self::MAX_MOTES {
            return Err(ConfigError(format!(
                "motes must be at most {}, got {}",
                Self::MAX_MOTES,
                self.motes
            )));
        }
        if self.statics > Self::MAX_STATICS {
            return Err(ConfigError(format!(
                "statics must be at most {}, got {}",
                Self::MAX_STATICS,
                self.statics
            )));
        }
        Ok(())
    }
}

/// An invalid [`WorldConfig`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid world config: {}", self.0)
    }
}

impl Error for ConfigError {}

/// One agent's action for one beat.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Action {
    /// Turn in `[-1, 1]`, scaled by [`MAX_TURN`].
    pub turn: f32,
    /// Thrust in `[0, 1]`, scaled by the quartetling's `tempo`.
    pub thrust: f32,
    pub sing: bool,
}

/// Floats per action in the flat tensor layout: `[turn, thrust, sing]`.
pub const ACTION_DIM: usize = 3;

impl Action {
    /// Parse one action from the flat layout. `turn` and `thrust` are given in
    /// `[-1, 1]` (thrust is mapped to `[0, 1]`), out-of-range values are
    /// clamped, NaN counts as 0, and `sing` is true when its value is ≥ 0.5.
    pub fn from_flat(v: &[f32]) -> Action {
        let clean = |x: f32| if x.is_nan() { 0.0 } else { x.clamp(-1.0, 1.0) };
        Action {
            turn: clean(v[0]),
            thrust: (clean(v[1]) + 1.0) / 2.0,
            sing: v[2] >= 0.5,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quartetling {
    /// Unique within the world; a respawned slot gets a new id.
    pub id: u64,
    pub voice: Voice,
    pub genome: Genome,
    pub x: f32,
    pub y: f32,
    /// Radians in `(-π, π]`, 0 = +x.
    pub heading: f32,
    /// Current speed in world units per beat.
    pub speed: f32,
    pub energy: f32,
    pub age: u32,
    /// Whether it sang this beat (it wanted to and had breath).
    pub singing: bool,
    pub in_chord: bool,
    /// In `[0, 1]`; singing uses it, resting restores it.
    pub breath: f32,
    /// 0 for founders.
    pub parent_id: u64,
    /// 0 for founders.
    pub generation: u32,
    /// The id of the founder this quartetling descends from (its own for founders).
    pub lineage: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mote {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaticField {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub radius: f32,
}

/// What happened to each slot in the last step.
#[derive(Clone, Debug, Default)]
pub struct StepResult {
    pub rewards: Vec<f32>,
    /// The slot's quartetling died; a new one now occupies it.
    pub terminated: Vec<bool>,
    /// The slot's quartetling was retired at [`MAX_AGE`]; a new one now occupies it.
    pub truncated: Vec<bool>,
    pub stats: BeatStats,
}

/// Counters for the last step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BeatStats {
    pub motes_eaten: u32,
    pub deaths: u32,
    /// Quartetlings that were part of a chord.
    pub chord_members: u32,
    /// Energy summed over every slot after the step (newcomers included).
    pub energy: f32,
    /// Slots refilled by an offspring rather than a founder.
    pub births: u32,
}

/// An error from [`World::step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepError(pub String);

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid step: {}", self.0)
    }
}

impl Error for StepError {}

#[derive(Clone, Debug)]
pub struct World {
    config: WorldConfig,
    rng: Rng,
    beat: u64,
    next_id: u64,
    quartetlings: Vec<Quartetling>,
    motes: Vec<Mote>,
    statics: Vec<StaticField>,
    result: StepResult,
    energy_before: Vec<f32>,
    /// Scratch for chord detection: indices of this beat's singers.
    singers: Vec<usize>,
    /// Scratch: slots vacated this beat.
    vacated: Vec<usize>,
}

impl World {
    pub fn new(config: WorldConfig) -> Result<World, ConfigError> {
        config.validate()?;
        let mut rng = Rng::new(config.seed);
        let mut world = World {
            rng: rng.split(),
            beat: 0,
            next_id: 1,
            quartetlings: Vec::with_capacity(config.slots),
            motes: Vec::with_capacity(config.motes),
            statics: Vec::with_capacity(config.statics),
            result: StepResult {
                rewards: vec![0.0; config.slots],
                terminated: vec![false; config.slots],
                truncated: vec![false; config.slots],
                stats: BeatStats::default(),
            },
            energy_before: vec![0.0; config.slots],
            singers: Vec::with_capacity(config.slots),
            vacated: Vec::with_capacity(config.slots),
            config,
        };
        for slot in 0..world.config.slots {
            let mut q = world.spawn(Voice::for_slot(slot));
            // Stagger founders' ages so they don't all retire on the same beat.
            q.age = (world.rng.unit() * MAX_AGE as f32 / 2.0) as u32;
            world.quartetlings.push(q);
        }
        for _ in 0..world.config.motes {
            let m = world.random_mote();
            world.motes.push(m);
        }
        for _ in 0..world.config.statics {
            let s = world.random_static();
            world.statics.push(s);
        }
        Ok(world)
    }

    pub fn config(&self) -> &WorldConfig {
        &self.config
    }

    pub fn beat(&self) -> u64 {
        self.beat
    }

    pub fn quartetlings(&self) -> &[Quartetling] {
        &self.quartetlings
    }

    pub fn motes(&self) -> &[Mote] {
        &self.motes
    }

    pub fn statics(&self) -> &[StaticField] {
        &self.statics
    }

    /// The outcome of the last step (all zeros before the first step).
    pub fn last_result(&self) -> &StepResult {
        &self.result
    }

    /// Advance one beat. `actions` has one entry per slot.
    pub fn step(&mut self, actions: &[Action]) -> Result<&StepResult, StepError> {
        let slots = self.config.slots;
        if actions.len() != slots {
            return Err(StepError(format!(
                "expected {slots} actions, got {}",
                actions.len()
            )));
        }
        let (width, height) = (self.config.width, self.config.height);
        self.result.stats = BeatStats::default();

        // 1. Move, and pay for existing, moving, and singing.
        for (i, (q, a)) in self.quartetlings.iter_mut().zip(actions).enumerate() {
            self.energy_before[i] = q.energy;
            q.heading = wrap_angle(q.heading + a.turn * MAX_TURN);
            q.speed = a.thrust * q.genome.tempo;
            q.x = (q.x + q.speed * q.heading.cos()).clamp(0.0, width);
            q.y = (q.y + q.speed * q.heading.sin()).clamp(0.0, height);
            q.singing = a.sing && q.breath >= BREATH_USE;
            q.in_chord = false;
            let effort = a.thrust * a.thrust;
            q.energy -= q.genome.dynamics * (BASE_METABOLISM + MOVE_COST * effort);
            if q.singing {
                q.energy -= SING_COST;
                q.breath = (q.breath - BREATH_USE).max(0.0);
            } else {
                q.breath = (q.breath + BREATH_RECOVER).min(1.0);
            }
            q.age += 1;
        }

        // 2. Eat motes. Each mote goes to the first overlapping slot, in slot order.
        for m in 0..self.motes.len() {
            let mote = self.motes[m];
            let eater = self.quartetlings.iter_mut().find(|q| {
                let reach = q.genome.dynamics + MOTE_RADIUS;
                dist2(q.x, q.y, mote.x, mote.y) < reach * reach
            });
            if let Some(q) = eater {
                q.energy += MOTE_ENERGY;
                self.result.stats.motes_eaten += 1;
                self.motes[m] = self.random_mote();
            }
        }

        // 3. Static fields drift, bounce off the walls, and drain energy.
        for s in &mut self.statics {
            s.x += s.vx;
            s.y += s.vy;
            if s.x < 0.0 || s.x > width {
                s.vx = -s.vx;
                s.x = s.x.clamp(0.0, width);
            }
            if s.y < 0.0 || s.y > height {
                s.vy = -s.vy;
                s.y = s.y.clamp(0.0, height);
            }
        }
        for q in &mut self.quartetlings {
            if self
                .statics
                .iter()
                .any(|s| dist2(q.x, q.y, s.x, s.y) < s.radius * s.radius)
            {
                q.energy -= STATIC_DRAIN;
            }
        }

        // 4. Chords: a singer is in a chord when singers within CHORD_RADIUS
        //    (itself included) cover at least 3 distinct voices.
        self.singers.clear();
        self.singers
            .extend((0..slots).filter(|&i| self.quartetlings[i].singing));
        for &i in &self.singers {
            let qi = &self.quartetlings[i];
            let mut voices = [false; 4];
            for &j in &self.singers {
                let qj = &self.quartetlings[j];
                if dist2(qi.x, qi.y, qj.x, qj.y) <= CHORD_RADIUS * CHORD_RADIUS {
                    voices[qj.voice.index()] = true;
                }
            }
            let distinct = voices.iter().filter(|&&v| v).count();
            let bonus = match distinct {
                4 => CHORD_BONUS_4,
                3 => CHORD_BONUS_3,
                _ => continue,
            };
            let q = &mut self.quartetlings[i];
            q.energy += bonus;
            q.in_chord = true;
            self.result.stats.chord_members += 1;
        }

        // 5. Rewards, deaths, retirements, and respawns.
        self.vacated.clear();
        for i in 0..slots {
            let q = &mut self.quartetlings[i];
            q.energy = q.energy.min(MAX_ENERGY);
            let died = q.energy <= 0.0;
            let retired = !died && q.age >= MAX_AGE;
            let gained = q.energy.max(0.0) - self.energy_before[i];
            self.result.rewards[i] = REWARD_SCALE * gained + if died { DEATH_PENALTY } else { 0.0 };
            self.result.terminated[i] = died;
            self.result.truncated[i] = retired;
            if died {
                self.result.stats.deaths += 1;
            }
            if died || retired {
                self.vacated.push(i);
            }
        }
        // Refill after every slot is scored, so a parent is chosen from the
        // quartetlings that lived through this beat. Without `evolve`, the
        // random stream is used exactly as before evolution existed.
        for k in 0..self.vacated.len() {
            let i = self.vacated[k];
            let voice = self.quartetlings[i].voice;
            let born = if self.config.evolve {
                self.offspring(voice)
            } else {
                None
            };
            self.result.stats.births += born.is_some() as u32;
            self.quartetlings[i] = born.unwrap_or_else(|| self.spawn(voice));
        }

        self.result.stats.energy = self.quartetlings.iter().map(|q| q.energy).sum();
        self.beat += 1;
        Ok(&self.result)
    }

    /// An offspring of a random eligible parent of `voice`, or `None` if no
    /// quartetling of that voice is mature, not retiring, and well fed. The
    /// parent pays [`REPRO_COST`].
    fn offspring(&mut self, voice: Voice) -> Option<Quartetling> {
        let eligible = |q: &Quartetling| {
            q.voice == voice && (MATURITY..MAX_AGE).contains(&q.age) && q.energy >= REPRO_ENERGY
        };
        let n = self.quartetlings.iter().filter(|q| eligible(q)).count();
        if n == 0 {
            return None;
        }
        let pick = ((self.rng.unit() * n as f32) as usize).min(n - 1);
        let p = self
            .quartetlings
            .iter()
            .enumerate()
            .filter(|(_, q)| eligible(q))
            .nth(pick)
            .map(|(i, _)| i)?;
        self.quartetlings[p].energy -= REPRO_COST;
        let parent = self.quartetlings[p].clone();
        let angle = self.rng.range(-PI, PI);
        let dist = self.rng.range(0.0, OFFSPRING_RADIUS);
        let id = self.next_id;
        self.next_id += 1;
        Some(Quartetling {
            id,
            voice,
            genome: parent.genome.mutate(voice, &mut self.rng),
            x: (parent.x + dist * angle.cos()).clamp(0.0, self.config.width),
            y: (parent.y + dist * angle.sin()).clamp(0.0, self.config.height),
            heading: self.rng.range(-PI, PI),
            speed: 0.0,
            energy: START_ENERGY,
            age: 0,
            singing: false,
            in_chord: false,
            breath: 1.0,
            parent_id: parent.id,
            generation: parent.generation + 1,
            lineage: parent.lineage,
        })
    }

    fn spawn(&mut self, voice: Voice) -> Quartetling {
        let id = self.next_id;
        self.next_id += 1;
        Quartetling {
            id,
            voice,
            genome: Genome::sample(voice, &mut self.rng),
            x: self.rng.range(0.0, self.config.width),
            y: self.rng.range(0.0, self.config.height),
            heading: self.rng.range(-PI, PI),
            speed: 0.0,
            energy: START_ENERGY,
            age: 0,
            singing: false,
            in_chord: false,
            breath: 1.0,
            parent_id: 0,
            generation: 0,
            lineage: id,
        }
    }

    fn random_mote(&mut self) -> Mote {
        Mote {
            x: self.rng.range(0.0, self.config.width),
            y: self.rng.range(0.0, self.config.height),
        }
    }

    fn random_static(&mut self) -> StaticField {
        let angle = self.rng.range(-PI, PI);
        StaticField {
            x: self.rng.range(0.0, self.config.width),
            y: self.rng.range(0.0, self.config.height),
            vx: STATIC_SPEED * angle.cos(),
            vy: STATIC_SPEED * angle.sin(),
            radius: self.rng.range(STATIC_RADIUS.0, STATIC_RADIUS.1),
        }
    }

    #[cfg(test)]
    pub(crate) fn quartetlings_mut(&mut self) -> &mut Vec<Quartetling> {
        &mut self.quartetlings
    }

    #[cfg(test)]
    pub(crate) fn motes_mut(&mut self) -> &mut Vec<Mote> {
        &mut self.motes
    }

    #[cfg(test)]
    pub(crate) fn statics_mut(&mut self) -> &mut Vec<StaticField> {
        &mut self.statics
    }
}

pub(crate) fn dist2(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    dx * dx + dy * dy
}

/// Wrap an angle into `(-π, π]`.
pub(crate) fn wrap_angle(a: f32) -> f32 {
    let mut a = (a + PI).rem_euclid(TAU) - PI;
    if a <= -PI {
        a += TAU;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle(slots: usize) -> Vec<Action> {
        vec![Action::default(); slots]
    }

    fn quiet_world(slots: usize) -> World {
        let mut w = World::new(WorldConfig {
            slots,
            motes: 0,
            statics: 0,
            ..WorldConfig::default()
        })
        .unwrap();
        // Spread everyone out so nothing interacts unless a test arranges it.
        for (i, q) in w.quartetlings_mut().iter_mut().enumerate() {
            q.x = 5.0 + 20.0 * (i % 4) as f32;
            q.y = 5.0 + 20.0 * (i / 4) as f32;
        }
        w
    }

    #[test]
    fn config_validation() {
        assert!(WorldConfig::default().validate().is_ok());
        for bad in [
            WorldConfig {
                slots: 0,
                ..Default::default()
            },
            WorldConfig {
                slots: 6,
                ..Default::default()
            },
            WorldConfig {
                slots: 2048,
                ..Default::default()
            },
            WorldConfig {
                width: 5.0,
                ..Default::default()
            },
            WorldConfig {
                height: f32::NAN,
                ..Default::default()
            },
            WorldConfig {
                motes: 100_000,
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn same_seed_same_trajectory() {
        let cfg = WorldConfig {
            seed: 42,
            ..Default::default()
        };
        let (mut a, mut b) = (World::new(cfg.clone()).unwrap(), World::new(cfg).unwrap());
        let actions: Vec<Action> = (0..64)
            .map(|i| Action {
                turn: 0.3,
                thrust: (i % 3) as f32 / 2.0,
                sing: i % 5 == 0,
            })
            .collect();
        for _ in 0..200 {
            a.step(&actions).unwrap();
            b.step(&actions).unwrap();
        }
        assert_eq!(a.quartetlings(), b.quartetlings());
        assert_eq!(a.motes(), b.motes());
    }

    #[test]
    fn different_seeds_differ() {
        let a = World::new(WorldConfig {
            seed: 1,
            ..Default::default()
        })
        .unwrap();
        let b = World::new(WorldConfig {
            seed: 2,
            ..Default::default()
        })
        .unwrap();
        assert_ne!(a.quartetlings()[0].x, b.quartetlings()[0].x);
    }

    #[test]
    fn rejects_wrong_action_count() {
        let mut w = quiet_world(8);
        assert!(w.step(&idle(4)).is_err());
    }

    #[test]
    fn slots_cycle_voices() {
        let w = quiet_world(8);
        let voices: Vec<Voice> = w.quartetlings().iter().map(|q| q.voice).collect();
        assert_eq!(&voices[..4], &Voice::ALL);
        assert_eq!(&voices[4..], &Voice::ALL);
    }

    #[test]
    fn idling_costs_energy_and_gives_negative_reward() {
        let mut w = quiet_world(4);
        let result = w.step(&idle(4)).unwrap();
        assert!(result.rewards.iter().all(|&r| r < 0.0));
        assert!(w.quartetlings().iter().all(|q| q.energy < START_ENERGY));
    }

    #[test]
    fn eating_a_mote_adds_energy() {
        let mut w = quiet_world(4);
        let (x, y) = (w.quartetlings()[0].x, w.quartetlings()[0].y);
        w.motes_mut().push(Mote { x, y });
        let result = w.step(&idle(4)).unwrap().clone();
        assert_eq!(result.stats.motes_eaten, 1);
        assert!(result.rewards[0] > 0.0);
        assert!(w.quartetlings()[0].energy > START_ENERGY);
        // The mote respawned somewhere else.
        assert_eq!(w.motes().len(), 1);
    }

    #[test]
    fn static_drains_energy() {
        let mut w = quiet_world(4);
        let (x, y) = (w.quartetlings()[0].x, w.quartetlings()[0].y);
        w.statics_mut().push(StaticField {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            radius: 3.0,
        });
        w.step(&idle(4)).unwrap();
        let drained = w.quartetlings()[0].energy;
        let normal = w.quartetlings()[1].energy;
        assert!(drained < normal - STATIC_DRAIN * 0.5);
    }

    #[test]
    fn chord_needs_three_distinct_voices_singing_together() {
        let mut w = quiet_world(8);
        // Slots 0, 1, 2 are soprano, alto, tenor. Put them together.
        for i in 0..3 {
            let q = &mut w.quartetlings_mut()[i];
            q.x = 50.0 + i as f32;
            q.y = 50.0;
        }
        let mut actions = idle(8);
        actions[0].sing = true;
        actions[1].sing = true;
        let result = w.step(&actions).unwrap();
        assert_eq!(result.stats.chord_members, 0, "two voices are not a chord");
        // Slot 3 (bass) joins: a full quartet earns the bigger bonus.

        actions[2].sing = true;
        let result = w.step(&actions).unwrap().clone();
        assert_eq!(result.stats.chord_members, 3);
        assert!(w.quartetlings()[..3].iter().all(|q| q.in_chord));
        let three = result.rewards[0];

        let q = &mut w.quartetlings_mut()[3];
        q.x = 50.0;
        q.y = 51.0;
        actions[3].sing = true;
        let result = w.step(&actions).unwrap().clone();
        assert_eq!(result.stats.chord_members, 4);
        // A full quartet's bonus outweighs singing and living costs.
        assert!(result.rewards[0] > 0.0);
        assert!(result.rewards[0] > three);
    }

    #[test]
    fn two_sopranos_and_an_alto_are_not_a_chord() {
        let mut w = quiet_world(8);
        for (k, i) in [0usize, 4, 1].into_iter().enumerate() {
            let q = &mut w.quartetlings_mut()[i];
            q.x = 50.0 + k as f32;
            q.y = 50.0;
        }
        let mut actions = idle(8);
        for i in [0, 4, 1] {
            actions[i].sing = true;
        }
        assert_eq!(w.step(&actions).unwrap().stats.chord_members, 0);
    }

    #[test]
    fn singing_needs_breath() {
        let mut w = quiet_world(4);
        let mut actions = idle(4);
        actions[0].sing = true;
        let mut sang = 0;
        for _ in 0..100 {
            w.step(&actions).unwrap();
            sang += w.quartetlings()[0].singing as u32;
        }
        // Full breath lasts 25 beats; afterwards it sings 1 beat in 3.
        assert!((40..=60).contains(&sang), "sang {sang} of 100 beats");
    }

    #[test]
    fn death_terminates_and_respawns_same_voice() {
        let mut w = quiet_world(4);
        w.quartetlings_mut()[2].energy = 0.001;
        let old_id = w.quartetlings()[2].id;
        let result = w.step(&idle(4)).unwrap().clone();
        assert!(result.terminated[2]);
        assert!(!result.truncated[2]);
        assert!(result.rewards[2] < DEATH_PENALTY * 0.5);
        assert_eq!(result.stats.deaths, 1);
        let q = &w.quartetlings()[2];
        assert_ne!(q.id, old_id);
        assert_eq!(q.voice, Voice::Tenor);
        assert_eq!(q.energy, START_ENERGY);
    }

    #[test]
    fn old_age_truncates() {
        let mut w = quiet_world(4);
        w.quartetlings_mut()[1].age = MAX_AGE - 1;
        let result = w.step(&idle(4)).unwrap();
        assert!(result.truncated[1]);
        assert!(!result.terminated[1]);
    }

    #[test]
    fn movement_follows_heading_and_stays_in_bounds() {
        let mut w = quiet_world(4);
        {
            let q = &mut w.quartetlings_mut()[0];
            q.x = 50.0;
            q.y = 50.0;
            q.heading = 0.0;
        }
        let mut actions = idle(4);
        actions[0] = Action {
            turn: 0.0,
            thrust: 1.0,
            sing: false,
        };
        w.step(&actions).unwrap();
        let q = &w.quartetlings()[0];
        assert!((q.x - (50.0 + q.genome.tempo)).abs() < 1e-4);
        for _ in 0..500 {
            w.step(&actions).unwrap();
        }
        let q = &w.quartetlings()[0];
        assert!(q.x <= w.config().width && q.x >= 0.0);
    }

    #[test]
    fn action_parsing_clamps_and_maps() {
        let a = Action::from_flat(&[5.0, -1.0, 0.7]);
        assert_eq!(
            a,
            Action {
                turn: 1.0,
                thrust: 0.0,
                sing: true
            }
        );
        let a = Action::from_flat(&[f32::NAN, 1.0, 0.2]);
        assert_eq!(
            a,
            Action {
                turn: 0.0,
                thrust: 1.0,
                sing: false
            }
        );
    }

    /// A quiet evolving world where slot 0 (soprano) is an eligible parent
    /// and slot 4 (also soprano) is about to retire.
    fn evolving_world() -> World {
        let mut w = quiet_world(8);
        w.config.evolve = true;
        for q in w.quartetlings_mut() {
            q.age = 0; // nobody else is mature
        }
        let qs = w.quartetlings_mut();
        qs[0].age = MATURITY;
        qs[0].energy = REPRO_ENERGY + 0.2;
        qs[4].age = MAX_AGE - 1;
        w
    }

    #[test]
    fn evolving_worlds_refill_with_offspring() {
        let mut w = evolving_world();
        let parent = w.quartetlings()[0].clone();
        let result = w.step(&idle(8)).unwrap().clone();
        assert!(result.truncated[4]);
        assert_eq!(result.stats.births, 1);
        let child = &w.quartetlings()[4];
        assert_eq!(child.voice, Voice::Soprano);
        assert_eq!(child.parent_id, parent.id);
        assert_eq!(child.generation, 1);
        assert_eq!(child.lineage, parent.lineage);
        assert!(
            dist2(child.x, child.y, parent.x, parent.y)
                <= OFFSPRING_RADIUS * OFFSPRING_RADIUS + 1e-3
        );
        let (lo, hi) = Genome::bounds(Voice::Soprano);
        assert!((lo.tempo..=hi.tempo).contains(&child.genome.tempo));
        assert!(child.genome != parent.genome, "the genome mutated");
        // The parent paid for it (beyond this beat's metabolism).
        assert!(w.quartetlings()[0].energy < parent.energy - REPRO_COST + 0.01);
    }

    #[test]
    fn no_eligible_parent_means_a_founder() {
        let mut w = evolving_world();
        w.quartetlings_mut()[0].energy = REPRO_ENERGY - 0.1;
        let result = w.step(&idle(8)).unwrap().clone();
        assert_eq!(result.stats.births, 0);
        let q = &w.quartetlings()[4];
        assert_eq!((q.generation, q.parent_id), (0, 0));
        assert_eq!(q.lineage, q.id);
    }

    #[test]
    fn evolution_off_keeps_the_old_random_stream() {
        // Same seed with and without an eligible parent present: a
        // non-evolving world must not consume extra randomness.
        let run = |boost: bool| {
            let mut w = quiet_world(8);
            if boost {
                w.quartetlings_mut()[0].age = MATURITY;
                w.quartetlings_mut()[0].energy = MAX_ENERGY;
            }
            w.quartetlings_mut()[4].age = MAX_AGE - 1;
            w.step(&idle(8)).unwrap();
            w.quartetlings()[4].clone()
        };
        assert_eq!(run(false).genome, run(true).genome);
    }

    #[test]
    fn mutation_stays_in_the_trained_range() {
        let mut rng = Rng::new(9);
        for voice in Voice::ALL {
            let (lo, hi) = Genome::bounds(voice);
            let mut g = hi;
            for _ in 0..1000 {
                g = g.mutate(voice, &mut rng);
                assert!((lo.pitch..=hi.pitch).contains(&g.pitch));
                assert!((lo.dynamics..=hi.dynamics).contains(&g.dynamics));
            }
        }
    }

    #[test]
    fn wrap_angle_range() {
        for a in [-10.0, -PI, 0.0, PI, 3.5, 100.0] {
            let w = wrap_angle(a);
            assert!(w > -PI - 1e-6 && w <= PI + 1e-6, "{a} -> {w}");
        }
    }
}
