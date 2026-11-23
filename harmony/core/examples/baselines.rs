//! Measure the builtin policies on the metrics learned policies are compared
//! with (`quartet-melody evaluate` reports the same ones).
//!
//! ```sh
//! cargo run --release -p quartet-harmony --example baselines
//! ```
//!
//! Each policy runs default 64-slot worlds for `WARMUP` beats, then is
//! measured over `BEATS` beats, averaged over `SEEDS` seeds.

use harmony::policy::{Builtin, act};
use harmony::rng::Rng;
use harmony::world::{World, WorldConfig};

const SEEDS: u64 = 8;
const WARMUP: u32 = 500;
const BEATS: u32 = 2000;

fn main() {
    println!(
        "{SEEDS} seeds × 64 slots, {WARMUP} warm-up beats, {BEATS} measured beats\n\
         {:<8} {:>18} {:>12} {:>11} {:>18}",
        "policy", "deaths/1k beats", "mean energy", "chord rate", "motes/1k beats"
    );
    for policy in Builtin::ALL {
        let (mut deaths, mut motes, mut chord, mut energy) = (0u64, 0u64, 0u64, 0f64);
        for seed in 0..SEEDS {
            let mut world = World::new(WorldConfig {
                seed,
                ..Default::default()
            })
            .expect("default config is valid");
            let mut rng = Rng::new(seed ^ 0xBA5E);
            let mut actions = Vec::new();
            for beat in 0..WARMUP + BEATS {
                act(policy, &world, &mut rng, &mut actions);
                let stats = world.step(&actions).expect("one action per slot").stats;
                if beat < WARMUP {
                    continue;
                }
                deaths += stats.deaths as u64;
                motes += stats.motes_eaten as u64;
                chord += stats.chord_members as u64;
                let qs = world.quartetlings();
                energy += qs.iter().map(|q| q.energy as f64).sum::<f64>() / qs.len() as f64;
            }
        }
        let runs = (SEEDS * BEATS as u64) as f64;
        println!(
            "{:<8} {:>18.1} {:>12.3} {:>11.4} {:>18.1}",
            policy.name(),
            deaths as f64 / runs * 1000.0,
            energy / runs,
            chord as f64 / (runs * 64.0),
            motes as f64 / runs * 1000.0,
        );
    }
}
