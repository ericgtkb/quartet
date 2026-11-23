//! Live worlds (performances), each stepped one beat at a time by rhythm.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use harmony::census::{CensusRecorder, census};
use harmony::observe::{OBS_DIM, ObserveScratch, observe};
use harmony::policy::act;
use harmony::rng::Rng;
use harmony::world::{ACTION_DIM, Action, World};
use tonic::Status;

use crate::convert::{
    builtin_from_proto, census_to_proto, config_from_proto, decode_f32, f32_tensor, snapshot,
};
use crate::generated::quartet::score as pb;
use crate::generated::quartet::score::step_world_request::Control;

/// Max live worlds at once, to bound memory.
pub const MAX_WORLDS: usize = 64;

struct StageWorld {
    world: World,
    policy_rng: Rng,
    actions: Vec<Action>,
    flat: Vec<f32>,
    scratch: ObserveScratch,
    observations: Vec<f32>,
    census: CensusRecorder,
}

pub struct Stage {
    worlds: RwLock<HashMap<String, Arc<Mutex<StageWorld>>>>,
    next: AtomicU64,
    /// Distinguishes ids across server restarts.
    epoch: u64,
}

impl Default for Stage {
    fn default() -> Self {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Stage {
            worlds: RwLock::default(),
            next: AtomicU64::new(1),
            epoch,
        }
    }
}

fn internal<E: std::fmt::Display>(e: E) -> Status {
    Status::internal(e.to_string())
}

impl Stage {
    pub fn create(&self, req: &pb::CreateWorldRequest) -> Result<pb::CreateWorldResponse, Status> {
        let config = config_from_proto(req.config.as_ref());
        let seed = config.seed;
        let world = World::new(config).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let mut worlds = self.worlds.write().map_err(internal)?;
        if worlds.len() >= MAX_WORLDS {
            return Err(Status::resource_exhausted(format!(
                "at most {MAX_WORLDS} live worlds"
            )));
        }
        let id = format!(
            "w{:x}-{}",
            self.epoch,
            self.next.fetch_add(1, Ordering::Relaxed)
        );
        let snap = snapshot(&world);
        let slots = world.config().slots;
        let mut sw = StageWorld {
            world,
            // Independent of the world's own stream, but still seeded.
            policy_rng: Rng::new(seed ^ 0x5EED_0FB1_7A11),
            actions: Vec::with_capacity(slots),
            flat: Vec::new(),
            scratch: ObserveScratch::default(),
            observations: vec![0.0; slots * OBS_DIM],
            census: CensusRecorder::default(),
        };
        let observations = req.include_observations.then(|| {
            observe(&sw.world, &mut sw.scratch, &mut sw.observations);
            f32_tensor(&[slots, OBS_DIM], &sw.observations)
        });
        worlds.insert(id.clone(), Arc::new(Mutex::new(sw)));
        Ok(pb::CreateWorldResponse {
            world_id: id,
            snapshot: Some(snap),
            observations,
            observation_version: harmony::OBSERVATION_VERSION,
        })
    }

    fn get(&self, world_id: &str) -> Result<Arc<Mutex<StageWorld>>, Status> {
        self.worlds
            .read()
            .map_err(internal)?
            .get(world_id)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("no world {world_id:?}")))
    }

    pub fn step(&self, req: &pb::StepWorldRequest) -> Result<pb::StepWorldResponse, Status> {
        let entry = self.get(&req.world_id)?;
        let mut guard = entry.lock().map_err(internal)?;
        let sw = &mut *guard;
        let slots = sw.world.config().slots;
        match &req.control {
            Some(Control::Builtin(policy)) => {
                act(
                    builtin_from_proto(*policy)?,
                    &sw.world,
                    &mut sw.policy_rng,
                    &mut sw.actions,
                );
            }
            Some(Control::Actions(tensor)) => {
                decode_f32(tensor, &[slots, ACTION_DIM], &mut sw.flat)?;
                sw.actions.clear();
                sw.actions.extend(
                    sw.flat
                        .as_chunks::<ACTION_DIM>()
                        .0
                        .iter()
                        .map(|a| Action::from_flat(a)),
                );
            }
            None => {
                return Err(Status::invalid_argument(
                    "step needs actions or a builtin policy",
                ));
            }
        }
        sw.world
            .step(&sw.actions)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        sw.census.record(&sw.world);
        let observations = if req.include_observations {
            observe(&sw.world, &mut sw.scratch, &mut sw.observations);
            Some(f32_tensor(&[slots, OBS_DIM], &sw.observations))
        } else {
            None
        };
        Ok(pb::StepWorldResponse {
            snapshot: Some(snapshot(&sw.world)),
            observations,
        })
    }

    pub fn census(&self, world_id: &str) -> Result<pb::Census, Status> {
        let entry = self.get(world_id)?;
        let sw = entry.lock().map_err(internal)?;
        Ok(census_to_proto(&census(&sw.world), sw.census.history()))
    }

    pub fn delete(&self, world_id: &str) -> Result<(), Status> {
        self.worlds
            .write()
            .map_err(internal)?
            .remove(world_id)
            .map(|_| ())
            .ok_or_else(|| Status::not_found(format!("no world {world_id:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::f32_tensor;

    fn create(stage: &Stage) -> String {
        let req = pb::CreateWorldRequest {
            config: Some(pb::WorldConfig {
                seed: 1,
                slots: 8,
                ..Default::default()
            }),
            include_observations: true,
        };
        let resp = stage.create(&req).unwrap();
        assert_eq!(resp.snapshot.unwrap().quartetlings.len(), 8);
        assert_eq!(resp.observations.unwrap().shape, vec![8, OBS_DIM as u32]);
        assert_eq!(resp.observation_version, harmony::OBSERVATION_VERSION);
        resp.world_id
    }

    #[test]
    fn census_follows_the_world() {
        let stage = Stage::default();
        let id = create(&stage);
        for _ in 0..25 {
            stage
                .step(&pb::StepWorldRequest {
                    world_id: id.clone(),
                    control: Some(Control::Builtin(pb::BuiltinPolicy::Choir.into())),
                    include_observations: false,
                })
                .unwrap();
        }
        let c = stage.census(&id).unwrap();
        assert_eq!((c.beat, c.slots), (25, 8));
        assert_eq!(c.voices.len(), 4);
        assert_eq!(c.voices[1].voice, i32::from(pb::Voice::Alto));
        assert_eq!(c.density.unwrap().counts.iter().sum::<u32>(), 8);
        assert_eq!(c.history.len(), 2, "two full 10-beat windows");
        assert_eq!(c.history[1].mean_energy.len(), 4);
        assert_eq!(
            stage.census("nope").unwrap_err().code(),
            tonic::Code::NotFound
        );
    }

    #[test]
    fn create_step_delete() {
        let stage = Stage::default();
        let id = create(&stage);

        let resp = stage
            .step(&pb::StepWorldRequest {
                world_id: id.clone(),
                control: Some(Control::Builtin(pb::BuiltinPolicy::Forage.into())),
                include_observations: true,
            })
            .unwrap();
        assert_eq!(resp.snapshot.unwrap().beat, 1);
        let obs = resp.observations.unwrap();
        assert_eq!(obs.shape, vec![8, OBS_DIM as u32]);

        let resp = stage
            .step(&pb::StepWorldRequest {
                world_id: id.clone(),
                control: Some(Control::Actions(f32_tensor(&[8, ACTION_DIM], &[0.0; 24]))),
                include_observations: false,
            })
            .unwrap();
        assert_eq!(resp.snapshot.unwrap().beat, 2);
        assert!(resp.observations.is_none());

        stage.delete(&id).unwrap();
        assert_eq!(stage.delete(&id).unwrap_err().code(), tonic::Code::NotFound);
    }

    #[test]
    fn invalid_requests_are_rejected() {
        let stage = Stage::default();
        let bad = pb::CreateWorldRequest {
            config: Some(pb::WorldConfig {
                slots: 6,
                ..Default::default()
            }),
            include_observations: false,
        };
        assert_eq!(
            stage.create(&bad).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );

        let id = create(&stage);
        let step = |control| pb::StepWorldRequest {
            world_id: id.clone(),
            control,
            include_observations: false,
        };
        assert_eq!(
            stage.step(&step(None)).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
        let wrong_shape = Control::Actions(f32_tensor(&[4, ACTION_DIM], &[0.0; 12]));
        assert_eq!(
            stage.step(&step(Some(wrong_shape))).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
        let unknown = pb::StepWorldRequest {
            world_id: "nope".into(),
            control: None,
            include_observations: false,
        };
        assert_eq!(
            stage.step(&unknown).unwrap_err().code(),
            tonic::Code::NotFound
        );
    }
}
