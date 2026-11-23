//! harmony: Quartet's world engine.
//!
//! This crate is the single implementation of the quartetling world. It is
//! deterministic and does no I/O; networking lives in `quartet-harmony-server`.
//!
//! - [`world`]: one arena, its rules, and stepping.
//! - [`observe`]: the observation layout used for learning.
//! - [`policy`]: built-in baseline policies.
//! - [`batch`]: many worlds stepped in parallel, for rehearsals.
//! - [`census`]: aggregated statistics of a world and its recent history.

pub mod batch;
pub mod census;
pub mod observe;
pub mod policy;
pub mod rng;
pub mod voice;
pub mod world;

/// Version of the harmony engine.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version of the observation and action tensor layout. Bump it whenever the
/// layout's size or meaning changes; melody refuses checkpoints trained on a
/// different version.
pub const OBSERVATION_VERSION: u32 = 1;

pub use observe::OBS_DIM;
pub use world::ACTION_DIM;
