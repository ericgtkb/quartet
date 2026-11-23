//! Egocentric, fixed-size observations.
//!
//! Layout (version [`crate::OBSERVATION_VERSION`]), `OBS_DIM` floats per slot.
//! Relative positions are rotated into the agent's frame (+x = ahead,
//! +y = left) and divided by its hearing radius (`pitch`), so they lie in
//! `[-1, 1]`. Missing entries are all zeros, with `present = 0`.
//!
//! | Offset | Size      | Content                                                         |
//! | ------ | --------- | --------------------------------------------------------------- |
//! | 0      | 4         | own voice, one-hot                                              |
//! | 4      | 1         | energy / MAX_ENERGY                                             |
//! | 5      | 3         | genome: tempo / 2, pitch / 25, dynamics / 2                      |
//! | 8      | 2         | position, scaled to `[-1, 1]` across the arena                   |
//! | 10     | 2         | heading: cos, sin                                               |
//! | 12     | 3         | singing, in_chord (last beat), breath                           |
//! | 15     | 6 × 8     | nearest others within pitch: present, rx, ry, voice one-hot (4), singing |
//! | 63     | 4 × 3     | nearest motes within pitch: present, rx, ry                     |
//! | 75     | 4         | nearest static field: present, rx, ry, edge distance / pitch    |
//!
//! Selection is brute force over all slots and motes. That is fine at the
//! planned scale (≤ 256 slots per world); add a spatial grid only after
//! measuring that observation building dominates.

use crate::world::{MAX_ENERGY, World, dist2};

pub const NEIGHBORS: usize = 6;
pub const NEIGHBOR_DIM: usize = 8;
pub const MOTES: usize = 4;
pub const MOTE_DIM: usize = 3;
pub const SELF_DIM: usize = 15;
pub const STATIC_DIM: usize = 4;
pub const OBS_DIM: usize = SELF_DIM + NEIGHBORS * NEIGHBOR_DIM + MOTES * MOTE_DIM + STATIC_DIM;

const NEIGHBOR_OFFSET: usize = SELF_DIM;
const MOTE_OFFSET: usize = NEIGHBOR_OFFSET + NEIGHBORS * NEIGHBOR_DIM;
const STATIC_OFFSET: usize = MOTE_OFFSET + MOTES * MOTE_DIM;

/// Reusable buffers for [`observe`], so steady-state observation building
/// does not allocate.
#[derive(Clone, Debug, Default)]
pub struct ObserveScratch {
    candidates: Vec<(f32, usize)>,
}

/// Write observations for every slot of `world` into `out`
/// (`slots × OBS_DIM`, row-major).
pub fn observe(world: &World, scratch: &mut ObserveScratch, out: &mut [f32]) {
    let qs = world.quartetlings();
    let motes = world.motes();
    let cfg = world.config();
    assert_eq!(
        out.len(),
        qs.len() * OBS_DIM,
        "observation buffer has the wrong size"
    );
    out.fill(0.0);

    for (i, (q, o)) in qs.iter().zip(out.as_chunks_mut::<OBS_DIM>().0).enumerate() {
        let (cos, sin) = (q.heading.cos(), q.heading.sin());
        let r = q.genome.pitch;
        // World offset → agent frame, scaled by hearing radius.
        let rel = |x: f32, y: f32| {
            let (dx, dy) = (x - q.x, y - q.y);
            ((dx * cos + dy * sin) / r, (-dx * sin + dy * cos) / r)
        };

        o[q.voice.index()] = 1.0;
        o[4] = q.energy / MAX_ENERGY;
        o[5] = q.genome.tempo / 2.0;
        o[6] = q.genome.pitch / 25.0;
        o[7] = q.genome.dynamics / 2.0;
        o[8] = q.x / cfg.width * 2.0 - 1.0;
        o[9] = q.y / cfg.height * 2.0 - 1.0;
        o[10] = cos;
        o[11] = sin;
        o[12] = q.singing as u8 as f32;
        o[13] = q.in_chord as u8 as f32;
        o[14] = q.breath;

        // Nearest others. Ties break by slot so the result is deterministic.
        let c = &mut scratch.candidates;
        c.clear();
        c.extend(
            qs.iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(j, n)| (dist2(q.x, q.y, n.x, n.y), j))
                .filter(|&(d2, _)| d2 <= r * r),
        );
        c.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (k, &(_, j)) in c.iter().take(NEIGHBORS).enumerate() {
            let n = &qs[j];
            let base = NEIGHBOR_OFFSET + k * NEIGHBOR_DIM;
            let (rx, ry) = rel(n.x, n.y);
            o[base] = 1.0;
            o[base + 1] = rx;
            o[base + 2] = ry;
            o[base + 3 + n.voice.index()] = 1.0;
            o[base + 7] = n.singing as u8 as f32;
        }

        // Nearest motes.
        c.clear();
        c.extend(
            motes
                .iter()
                .enumerate()
                .map(|(j, m)| (dist2(q.x, q.y, m.x, m.y), j))
                .filter(|&(d2, _)| d2 <= r * r),
        );
        c.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (k, &(_, j)) in c.iter().take(MOTES).enumerate() {
            let base = MOTE_OFFSET + k * MOTE_DIM;
            let (rx, ry) = rel(motes[j].x, motes[j].y);
            o[base] = 1.0;
            o[base + 1] = rx;
            o[base + 2] = ry;
        }

        // Nearest static field by edge distance, if its edge is within hearing.
        let nearest = world
            .statics()
            .iter()
            .map(|s| (dist2(q.x, q.y, s.x, s.y).sqrt() - s.radius, s))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((edge, s)) = nearest
            && edge <= r
        {
            let (rx, ry) = rel(s.x, s.y);
            o[STATIC_OFFSET] = 1.0;
            o[STATIC_OFFSET + 1] = rx.clamp(-1.0, 1.0);
            o[STATIC_OFFSET + 2] = ry.clamp(-1.0, 1.0);
            o[STATIC_OFFSET + 3] = (edge / r).clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Action, Mote, WorldConfig};

    fn world() -> World {
        World::new(WorldConfig {
            seed: 3,
            slots: 8,
            motes: 0,
            statics: 0,
            ..Default::default()
        })
        .unwrap()
    }

    fn obs(w: &World) -> Vec<f32> {
        let mut out = vec![0.0; w.config().slots * OBS_DIM];
        observe(w, &mut ObserveScratch::default(), &mut out);
        out
    }

    #[test]
    fn layout_size_is_stable() {
        assert_eq!(
            OBS_DIM, 79,
            "changing the layout requires bumping OBSERVATION_VERSION"
        );
    }

    #[test]
    fn values_are_bounded() {
        let mut w = World::new(WorldConfig {
            seed: 9,
            ..Default::default()
        })
        .unwrap();
        let actions = vec![
            Action {
                turn: 0.5,
                thrust: 1.0,
                sing: true
            };
            64
        ];
        for _ in 0..50 {
            w.step(&actions).unwrap();
        }
        assert!(
            obs(&w)
                .iter()
                .all(|v| v.is_finite() && (-1.0..=1.0).contains(v))
        );
    }

    #[test]
    fn neighbor_ahead_is_positive_x_in_agent_frame() {
        let mut w = world();
        for (i, q) in w.quartetlings_mut().iter_mut().enumerate() {
            q.x = 10.0 + 12.0 * i as f32;
            q.y = 90.0;
        }
        // Slot 0 at (50, 50) facing +y; slot 1 (alto) 5 units in front of it.
        let qs = w.quartetlings_mut();
        qs[0].x = 50.0;
        qs[0].y = 50.0;
        qs[0].heading = std::f32::consts::FRAC_PI_2;
        qs[1].x = 50.0;
        qs[1].y = 55.0;
        let pitch = w.quartetlings()[0].genome.pitch;

        let o = obs(&w);
        let n = &o[NEIGHBOR_OFFSET..NEIGHBOR_OFFSET + NEIGHBOR_DIM];
        assert_eq!(n[0], 1.0, "present");
        assert!((n[1] - 5.0 / pitch).abs() < 1e-5, "ahead: {}", n[1]);
        assert!(n[2].abs() < 1e-5, "not to the side: {}", n[2]);
        assert_eq!(&n[3..7], &[0.0, 1.0, 0.0, 0.0], "alto one-hot");
        // Nobody else is within hearing range.
        assert_eq!(o[NEIGHBOR_OFFSET + NEIGHBOR_DIM], 0.0);
    }

    #[test]
    fn nearest_mote_comes_first() {
        let mut w = world();
        let (x, y) = (w.quartetlings()[0].x, w.quartetlings()[0].y);
        w.motes_mut()
            .extend([Mote { x: x + 4.0, y }, Mote { x: x + 1.0, y }]);
        let o = obs(&w);
        let first = &o[MOTE_OFFSET..MOTE_OFFSET + MOTE_DIM];
        let second = &o[MOTE_OFFSET + MOTE_DIM..MOTE_OFFSET + 2 * MOTE_DIM];
        assert_eq!(first[0], 1.0);
        assert_eq!(second[0], 1.0);
        let len = |m: &[f32]| (m[1] * m[1] + m[2] * m[2]).sqrt();
        assert!(len(first) < len(second));
    }
}
