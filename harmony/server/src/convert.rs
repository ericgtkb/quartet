//! Conversions between harmony core types and `quartet.score` protos,
//! including packed tensors.

use harmony::census::{Census, Histogram, Sample, TraitStats};
use harmony::policy::Builtin;
use harmony::voice::Voice;
use harmony::world::{BeatStats, World, WorldConfig};
use tonic::Status;

use crate::generated::quartet::score as pb;

/// A proto config with zero values replaced by defaults. Not yet validated.
pub fn config_from_proto(c: Option<&pb::WorldConfig>) -> WorldConfig {
    let d = WorldConfig::default();
    let Some(c) = c else { return d };
    let or = |v: f32, default: f32| if v == 0.0 { default } else { v };
    WorldConfig {
        seed: c.seed,
        slots: if c.slots == 0 {
            d.slots
        } else {
            c.slots as usize
        },
        width: or(c.width, d.width),
        height: or(c.height, d.height),
        motes: c.motes.map_or(d.motes, |m| m as usize),
        statics: c.statics.map_or(d.statics, |s| s as usize),
        evolve: c.evolve,
    }
}

pub fn builtin_from_proto(value: i32) -> Result<Builtin, Status> {
    match pb::BuiltinPolicy::try_from(value) {
        Ok(pb::BuiltinPolicy::Random) => Ok(Builtin::Random),
        Ok(pb::BuiltinPolicy::Forage) => Ok(Builtin::Forage),
        Ok(pb::BuiltinPolicy::Choir) => Ok(Builtin::Choir),
        _ => Err(Status::invalid_argument(format!(
            "unknown builtin policy {value}"
        ))),
    }
}

fn voice(v: Voice) -> pb::Voice {
    match v {
        Voice::Soprano => pb::Voice::Soprano,
        Voice::Alto => pb::Voice::Alto,
        Voice::Tenor => pb::Voice::Tenor,
        Voice::Bass => pb::Voice::Bass,
    }
}

pub fn stats(s: BeatStats) -> pb::BeatStats {
    pb::BeatStats {
        motes_eaten: s.motes_eaten,
        deaths: s.deaths,
        chord_members: s.chord_members,
        energy: s.energy,
        births: s.births,
    }
}

pub fn snapshot(world: &World) -> pb::WorldSnapshot {
    let cfg = world.config();
    pb::WorldSnapshot {
        beat: world.beat(),
        width: cfg.width,
        height: cfg.height,
        quartetlings: world
            .quartetlings()
            .iter()
            .enumerate()
            .map(|(slot, q)| pb::Quartetling {
                slot: slot as u32,
                id: q.id,
                voice: voice(q.voice).into(),
                genome: Some(pb::Genome {
                    tempo: q.genome.tempo,
                    pitch: q.genome.pitch,
                    dynamics: q.genome.dynamics,
                }),
                x: q.x,
                y: q.y,
                heading: q.heading,
                speed: q.speed,
                energy: q.energy,
                breath: q.breath,
                age: q.age,
                singing: q.singing,
                in_chord: q.in_chord,
                parent_id: q.parent_id,
                generation: q.generation,
                lineage: q.lineage,
            })
            .collect(),
        motes: world
            .motes()
            .iter()
            .map(|m| pb::Mote { x: m.x, y: m.y })
            .collect(),
        statics: world
            .statics()
            .iter()
            .map(|s| pb::StaticField {
                x: s.x,
                y: s.y,
                radius: s.radius,
            })
            .collect(),
        stats: Some(stats(world.last_result().stats)),
    }
}

fn trait_stats(t: TraitStats) -> Option<pb::TraitStats> {
    Some(pb::TraitStats {
        mean: t.mean,
        std: t.std,
    })
}

fn histogram(h: &Histogram) -> Option<pb::Histogram> {
    Some(pb::Histogram {
        min: h.min,
        max: h.max,
        bins: h.bins as u32,
        counts: h.counts.clone(),
    })
}

pub fn census_to_proto<'a>(c: &Census, history: impl Iterator<Item = &'a Sample>) -> pb::Census {
    pb::Census {
        beat: c.beat,
        slots: c.slots as u32,
        voices: c
            .voices
            .iter()
            .map(|v| pb::VoiceCensus {
                voice: voice(v.voice).into(),
                population: v.population,
                mean_energy: v.mean_energy,
                mean_age: v.mean_age,
                tempo: trait_stats(v.tempo),
                pitch: trait_stats(v.pitch),
                dynamics: trait_stats(v.dynamics),
                singing: v.singing,
                in_chord: v.in_chord,
                mean_generation: v.mean_generation,
                max_generation: v.max_generation,
            })
            .collect(),
        tempo: histogram(&c.tempo),
        pitch: histogram(&c.pitch),
        dynamics: histogram(&c.dynamics),
        energy: histogram(&c.energy),
        density: Some(pb::DensityGrid {
            columns: harmony::census::GRID as u32,
            rows: harmony::census::GRID as u32,
            counts: c.density.clone(),
            singing: c.singing_density.clone(),
        }),
        history: history
            .map(|s| pb::CensusSample {
                beat: s.beat,
                beats: s.beats,
                deaths: s.deaths,
                motes_eaten: s.motes_eaten,
                chord_members: s.chord_members,
                singing: s.singing,
                mean_energy: s.mean_energy.to_vec(),
                births: s.births,
                mean_tempo: s.mean_tempo.to_vec(),
                mean_pitch: s.mean_pitch.to_vec(),
                mean_dynamics: s.mean_dynamics.to_vec(),
                mean_generation: s.mean_generation.to_vec(),
                max_generation: s.max_generation,
                lineages: s.lineages,
            })
            .collect(),
        lineages: c.lineages,
    }
}

fn shape(dims: &[usize]) -> Vec<u32> {
    dims.iter().map(|&d| d as u32).collect()
}

pub fn f32_tensor(dims: &[usize], values: &[f32]) -> pb::Tensor {
    let mut data = Vec::with_capacity(values.len() * 4);
    for v in values {
        data.extend_from_slice(&v.to_le_bytes());
    }
    pb::Tensor {
        dtype: pb::Dtype::Float32.into(),
        shape: shape(dims),
        data,
    }
}

pub fn u8_tensor(dims: &[usize], values: &[u8]) -> pb::Tensor {
    pb::Tensor {
        dtype: pb::Dtype::Uint8.into(),
        shape: shape(dims),
        data: values.to_vec(),
    }
}

/// Decode a float32 tensor of exactly `dims` into `out` (reused).
pub fn decode_f32(t: &pb::Tensor, dims: &[usize], out: &mut Vec<f32>) -> Result<(), Status> {
    if t.dtype != i32::from(pb::Dtype::Float32) {
        return Err(Status::invalid_argument(format!(
            "expected a float32 tensor, got dtype {}",
            t.dtype
        )));
    }
    if t.shape != shape(dims) {
        return Err(Status::invalid_argument(format!(
            "expected tensor shape {dims:?}, got {:?}",
            t.shape
        )));
    }
    let count: usize = dims.iter().product();
    if t.data.len() != count * 4 {
        return Err(Status::invalid_argument(format!(
            "tensor data has {} bytes, expected {}",
            t.data.len(),
            count * 4
        )));
    }
    out.clear();
    out.extend(
        t.data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b)),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_round_trip() {
        let values = [1.5f32, -2.0, 0.0, 3.25, f32::MAX, -0.5];
        let t = f32_tensor(&[2, 3], &values);
        let mut out = Vec::new();
        decode_f32(&t, &[2, 3], &mut out).unwrap();
        assert_eq!(out, values);
    }

    #[test]
    fn decode_rejects_mismatches() {
        let t = f32_tensor(&[2, 3], &[0.0; 6]);
        let mut out = Vec::new();
        assert!(decode_f32(&t, &[3, 2], &mut out).is_err());
        let mut bad = t.clone();
        bad.data.pop();
        assert!(decode_f32(&bad, &[2, 3], &mut out).is_err());
        let u = u8_tensor(&[6], &[0; 6]);
        assert!(decode_f32(&u, &[6], &mut out).is_err());
    }

    #[test]
    fn zero_config_fields_take_defaults() {
        let c = config_from_proto(Some(&pb::WorldConfig {
            seed: 9,
            motes: Some(0),
            ..Default::default()
        }));
        let d = WorldConfig::default();
        assert_eq!(
            c,
            WorldConfig {
                seed: 9,
                motes: 0,
                ..d
            }
        );
    }
}
