//! gRPC implementation of `quartet.score.HarmonyService`.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use crate::generated::quartet::score as pb;
use crate::generated::quartet::score::harmony_service_server::HarmonyService;
use crate::rehearsal;
use crate::stage::Stage;

/// The harmony gRPC service. Cheap to clone; clones share the stage.
#[derive(Clone, Default)]
pub struct Harmony {
    stage: Arc<Stage>,
}

type RehearseStream = Pin<Box<dyn Stream<Item = rehearsal::Reply> + Send>>;

#[tonic::async_trait]
impl HarmonyService for Harmony {
    async fn get_service_info(
        &self,
        _request: Request<pb::GetServiceInfoRequest>,
    ) -> Result<Response<pb::GetServiceInfoResponse>, Status> {
        Ok(Response::new(pb::GetServiceInfoResponse {
            info: Some(service_info()),
        }))
    }

    async fn create_world(
        &self,
        request: Request<pb::CreateWorldRequest>,
    ) -> Result<Response<pb::CreateWorldResponse>, Status> {
        let resp = self.stage.create(request.get_ref())?;
        tracing::info!(world_id = %resp.world_id, "world created");
        Ok(Response::new(resp))
    }

    async fn step_world(
        &self,
        request: Request<pb::StepWorldRequest>,
    ) -> Result<Response<pb::StepWorldResponse>, Status> {
        Ok(Response::new(self.stage.step(request.get_ref())?))
    }

    async fn delete_world(
        &self,
        request: Request<pb::DeleteWorldRequest>,
    ) -> Result<Response<pb::DeleteWorldResponse>, Status> {
        let id = &request.get_ref().world_id;
        self.stage.delete(id)?;
        tracing::info!(world_id = %id, "world deleted");
        Ok(Response::new(pb::DeleteWorldResponse {}))
    }

    async fn get_census(
        &self,
        request: Request<pb::GetCensusRequest>,
    ) -> Result<Response<pb::GetCensusResponse>, Status> {
        let census = self.stage.census(&request.get_ref().world_id)?;
        Ok(Response::new(pb::GetCensusResponse {
            census: Some(census),
        }))
    }

    type RehearseStream = RehearseStream;

    async fn rehearse(
        &self,
        request: Request<Streaming<pb::RehearseRequest>>,
    ) -> Result<Response<Self::RehearseStream>, Status> {
        let inbound = request.into_inner();
        // A small buffer: the client sends one request per response.
        let (tx, rx) = mpsc::channel(2);
        tokio::spawn(async move {
            if let Err(status) = rehearsal::run(inbound, tx.clone()).await {
                tracing::warn!(%status, "rehearsal failed");
                let _ = tx.send(Err(status)).await;
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

fn service_info() -> pb::ServiceInfo {
    pb::ServiceInfo {
        name: "harmony".to_owned(),
        version: harmony::VERSION.to_owned(),
        runtime: format!("Rust {}", env!("HARMONY_RUSTC_VERSION")),
        details: HashMap::from([(
            "observationVersion".to_owned(),
            harmony::OBSERVATION_VERSION.to_string(),
        )]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_service_info_describes_harmony() {
        let response = Harmony::default()
            .get_service_info(Request::new(pb::GetServiceInfoRequest {}))
            .await
            .expect("GetServiceInfo succeeds");
        let info = response.into_inner().info.expect("info is set");

        assert_eq!(info.name, "harmony");
        assert_eq!(info.version, harmony::VERSION);
        assert!(info.runtime.starts_with("Rust "));
        assert_eq!(
            info.details["observationVersion"],
            harmony::OBSERVATION_VERSION.to_string()
        );
    }
}
