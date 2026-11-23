//! Watch evolution in performances: how traits drift under selection.
//!
//! ```sh
//! cargo run --release -p quartet-harmony --example evolution [policy] [beats]
//! ```
//!
//! Runs `SEEDS` evolving default 64-slot worlds with a builtin policy
//! (default `forage`) and prints per-voice trait means at the start and end,
//! generation depth, births, and surviving founder lineages.

use harmony::census::census;
use harmony::policy::{Builtin, act};
use harmony::rng::Rng;
use harmony::voice::Voice;
use harmony::world::{World, WorldConfig};

const SEEDS: u64 = 8;

fn main() {
    let mut args = std::env::args().skip(1);
    let policy: Builtin = args
        .next()
        .map_or(Ok(Builtin::Forage), |s| s.parse())
        .expect("policy is random, forage, or choir");
    let beats: u32 = args
        .next()
        .map_or(Ok(20_000), |s| s.parse())
        .expect("beats is a number");

    // [voice][trait] sums at start and end, over seeds.
    let (mut start, mut end) = ([[0f32; 3]; 4], [[0f32; 3]; 4]);
    let (mut births, mut deaths, mut max_gen, mut mean_gen, mut lineages) =
        (0u64, 0u64, 0u32, 0f32, 0u32);
    for seed in 0..SEEDS {
        let mut world = World::new(WorldConfig {
            seed,
            evolve: true,
            ..Default::default()
        })
        .expect("default config is valid");
        let mut rng = Rng::new(seed ^ 0xE70);
        let mut actions = Vec::new();
        let traits = |w: &World| {
            census(w)
                .voices
                .map(|v| [v.tempo.mean, v.pitch.mean, v.dynamics.mean])
        };
        let first = traits(&world);
        for _ in 0..beats {
            act(policy, &world, &mut rng, &mut actions);
            let stats = world.step(&actions).expect("one action per slot").stats;
            births += stats.births as u64;
            deaths += stats.deaths as u64;
        }
        let last = traits(&world);
        for v in 0..4 {
            for t in 0..3 {
                start[v][t] += first[v][t] / SEEDS as f32;
                end[v][t] += last[v][t] / SEEDS as f32;
            }
        }
        let c = census(&world);
        max_gen = max_gen.max(c.voices.iter().map(|v| v.max_generation).max().unwrap_or(0));
        mean_gen += c.voices.iter().map(|v| v.mean_generation).sum::<f32>() / 4.0 / SEEDS as f32;
        lineages += c.lineages;
    }
    println!(
        "{policy}, {SEEDS} evolving worlds × 64 slots × {beats} beats\n\
         births {:.1} and deaths {:.1} per 1k beats; generation mean {mean_gen:.1}, max {max_gen}; \
         founder lineages alive {:.1} of 64+ (mean per world)\n",
        births as f64 / (SEEDS * beats as u64) as f64 * 1000.0,
        deaths as f64 / (SEEDS * beats as u64) as f64 * 1000.0,
        lineages as f32 / SEEDS as f32,
    );
    println!(
        "{:<8} {:>15} {:>15} {:>15}",
        "voice", "tempo", "pitch", "dynamics"
    );
    for (v, voice) in Voice::ALL.iter().enumerate() {
        let cell = |t: usize| format!("{:.3} → {:.3}", start[v][t], end[v][t]);
        println!(
            "{:<8} {:>15} {:>15} {:>15}",
            format!("{voice:?}").to_lowercase(),
            cell(0),
            cell(1),
            cell(2)
        );
    }
}
