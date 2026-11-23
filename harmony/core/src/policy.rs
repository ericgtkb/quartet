//! Built-in baseline policies. They read the world state directly (not
//! observations) and let the stack run before any training. They are also a
//! reference point for learned behavior.

use std::fmt;
use std::str::FromStr;

use crate::rng::Rng;
use crate::world::{Action, CHORD_RADIUS, MAX_TURN, Quartetling, World, dist2, wrap_angle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    /// Random turns and thrust; sings 10% of beats.
    Random,
    /// Head for the nearest mote, flee static, wander otherwise. Never sings.
    Forage,
    /// Forage, but stop and sing when two other voices are within chord range
    /// (the world ignores `sing` while out of breath), and seek other voices
    /// when well fed.
    Choir,
}

impl Builtin {
    pub const ALL: [Builtin; 3] = [Builtin::Random, Builtin::Forage, Builtin::Choir];

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Random => "random",
            Builtin::Forage => "forage",
            Builtin::Choir => "choir",
        }
    }
}

impl fmt::Display for Builtin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Builtin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Builtin::ALL
            .into_iter()
            .find(|b| b.name() == s)
            .ok_or_else(|| format!("unknown builtin policy {s:?}"))
    }
}

/// Compute one action per slot into `out`.
pub fn act(policy: Builtin, world: &World, rng: &mut Rng, out: &mut Vec<Action>) {
    out.clear();
    for q in world.quartetlings() {
        let action = match policy {
            Builtin::Random => Action {
                turn: rng.range(-1.0, 1.0),
                thrust: rng.unit(),
                sing: rng.unit() < 0.1,
            },
            Builtin::Forage => forage(q, world, rng),
            Builtin::Choir => choir(q, world, rng),
        };
        out.push(action);
    }
}

fn forage(q: &Quartetling, world: &World, rng: &mut Rng) -> Action {
    // Flee static: steer directly away from the field's center.
    if let Some(s) = world
        .statics()
        .iter()
        .find(|s| dist2(q.x, q.y, s.x, s.y) < s.radius * s.radius)
    {
        return steer(q, 2.0 * q.x - s.x, 2.0 * q.y - s.y, 1.0);
    }
    let r2 = q.genome.pitch * q.genome.pitch;
    let nearest = world
        .motes()
        .iter()
        .map(|m| (dist2(q.x, q.y, m.x, m.y), m))
        .filter(|&(d2, _)| d2 <= r2)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, m)) = nearest {
        return steer(q, m.x, m.y, 1.0);
    }
    // Wander, turning back toward the center near the walls.
    let cfg = world.config();
    let margin = 5.0;
    if q.x < margin || q.y < margin || q.x > cfg.width - margin || q.y > cfg.height - margin {
        return steer(q, cfg.width / 2.0, cfg.height / 2.0, 0.6);
    }
    Action {
        turn: rng.range(-0.3, 0.3),
        thrust: 0.6,
        sing: false,
    }
}

fn choir(q: &Quartetling, world: &World, rng: &mut Rng) -> Action {
    let mut others = [false; 4];
    for n in world.quartetlings() {
        if n.voice != q.voice && dist2(q.x, q.y, n.x, n.y) <= CHORD_RADIUS * CHORD_RADIUS {
            others[n.voice.index()] = true;
        }
    }
    let other_voices = others.iter().filter(|&&v| v).count();
    if other_voices >= 2 && q.energy > 0.4 {
        return Action {
            turn: 0.0,
            thrust: 0.0,
            sing: true,
        };
    }
    if q.energy > 1.2 {
        let r2 = q.genome.pitch * q.genome.pitch;
        let partner = world
            .quartetlings()
            .iter()
            .filter(|n| n.voice != q.voice)
            .map(|n| (dist2(q.x, q.y, n.x, n.y), n))
            .filter(|&(d2, _)| d2 <= r2)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, n)) = partner {
            return steer(q, n.x, n.y, 0.7);
        }
    }
    forage(q, world, rng)
}

/// Turn toward `(x, y)` as fast as allowed, at the given thrust.
fn steer(q: &Quartetling, x: f32, y: f32, thrust: f32) -> Action {
    let desired = (y - q.y).atan2(x - q.x);
    let turn = (wrap_angle(desired - q.heading) / MAX_TURN).clamp(-1.0, 1.0);
    Action {
        turn,
        thrust,
        sing: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::WorldConfig;

    #[test]
    fn names_round_trip() {
        for b in Builtin::ALL {
            assert_eq!(b.name().parse::<Builtin>().unwrap(), b);
        }
        assert!("melody".parse::<Builtin>().is_err());
    }

    #[test]
    fn every_policy_produces_valid_actions_for_every_slot() {
        let mut world = World::new(WorldConfig {
            seed: 5,
            ..Default::default()
        })
        .unwrap();
        let mut rng = Rng::new(1);
        let mut actions = Vec::new();
        for policy in Builtin::ALL {
            for _ in 0..20 {
                act(policy, &world, &mut rng, &mut actions);
                assert_eq!(actions.len(), 64);
                assert!(
                    actions
                        .iter()
                        .all(|a| (-1.0..=1.0).contains(&a.turn) && (0.0..=1.0).contains(&a.thrust))
                );
                world.step(&actions).unwrap();
            }
        }
    }

    #[test]
    fn foraging_eats_more_than_random() {
        let eaten = |policy| {
            let mut world = World::new(WorldConfig {
                seed: 11,
                ..Default::default()
            })
            .unwrap();
            let mut rng = Rng::new(2);
            let mut actions = Vec::new();
            let mut total = 0;
            for _ in 0..300 {
                act(policy, &world, &mut rng, &mut actions);
                total += world.step(&actions).unwrap().stats.motes_eaten;
            }
            total
        };
        assert!(eaten(Builtin::Forage) > eaten(Builtin::Random));
    }
}
