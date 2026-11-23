//! The `Rehearse` stream: a batch of worlds for training, owned by one stream.

use harmony::batch::{BatchOutputs, WorldBatch};
use harmony::observe::OBS_DIM;
use harmony::world::ACTION_DIM;
use tokio::sync::mpsc;
use tonic::{Status, Streaming};

use crate::convert::{config_from_proto, decode_f32, f32_tensor, stats, u8_tensor};
use crate::generated::quartet::score as pb;
use crate::generated::quartet::score::rehearse_request::Message;

pub type Reply = Result<pb::RehearseResponse, Status>;

/// Run one rehearsal until the client closes its side of the stream.
/// Responses go to `tx`; returning an error ends the stream with that status.
pub async fn run(
    mut inbound: Streaming<pb::RehearseRequest>,
    tx: mpsc::Sender<Reply>,
) -> Result<(), Status> {
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| Status::invalid_argument("empty rehearsal stream"))?;
    let Some(Message::Config(config)) = first.message else {
        return Err(Status::invalid_argument(
            "the first rehearsal message must be a config",
        ));
    };
    let base = config_from_proto(config.world.as_ref());
    let mut batch = WorldBatch::new(&base, config.worlds as usize)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    let agents = batch.agents();
    tracing::info!(worlds = batch.worlds(), agents, "rehearsal started");

    let mut out = BatchOutputs::default();
    batch.reset_outputs(&mut out);
    let mut first_reply = response(&out, agents);
    first_reply.observation_version = harmony::OBSERVATION_VERSION;
    first_reply.obs_dim = OBS_DIM as u32;
    first_reply.action_dim = ACTION_DIM as u32;
    if tx.send(Ok(first_reply)).await.is_err() {
        return Ok(()); // client went away
    }

    let mut actions = Vec::with_capacity(agents * ACTION_DIM);
    let mut steps: u64 = 0;
    while let Some(req) = inbound.message().await? {
        let Some(Message::Actions(tensor)) = req.message else {
            return Err(Status::invalid_argument("expected an actions message"));
        };
        decode_f32(&tensor, &[agents, ACTION_DIM], &mut actions)?;
        // Stepping is CPU-bound and parallel (rayon); keep it off the async workers.
        tokio::task::block_in_place(|| batch.step(&actions, &mut out))
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        steps += 1;
        if tx.send(Ok(response(&out, agents))).await.is_err() {
            break;
        }
    }
    tracing::info!(steps, "rehearsal ended");
    Ok(())
}

fn response(out: &BatchOutputs, agents: usize) -> pb::RehearseResponse {
    pb::RehearseResponse {
        observations: Some(f32_tensor(&[agents, OBS_DIM], &out.observations)),
        rewards: Some(f32_tensor(&[agents], &out.rewards)),
        terminated: Some(u8_tensor(&[agents], &out.terminated)),
        truncated: Some(u8_tensor(&[agents], &out.truncated)),
        stats: Some(stats(out.stats)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::net::TcpListener;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::{Channel, Server};

    use super::*;
    use crate::generated::quartet::score::harmony_service_client::HarmonyServiceClient;
    use crate::generated::quartet::score::harmony_service_server::HarmonyServiceServer;
    use crate::service::Harmony;

    async fn client() -> HarmonyServiceClient<Channel> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(
            Server::builder()
                .add_service(HarmonyServiceServer::new(Harmony::default()))
                .serve_with_incoming(TcpListenerStream::new(listener)),
        );
        HarmonyServiceClient::connect(format!("http://{addr}"))
            .await
            .unwrap()
    }

    fn config(worlds: u32, slots: u32) -> pb::RehearseRequest {
        pb::RehearseRequest {
            message: Some(Message::Config(pb::RehearsalConfig {
                worlds,
                world: Some(pb::WorldConfig {
                    seed: 1,
                    slots,
                    ..Default::default()
                }),
            })),
        }
    }

    fn actions(agents: usize) -> pb::RehearseRequest {
        let values = vec![0.5; agents * ACTION_DIM];
        pb::RehearseRequest {
            message: Some(Message::Actions(f32_tensor(&[agents, ACTION_DIM], &values))),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn config_then_steps() {
        let mut client = client().await;
        let agents = 3 * 8;
        let requests = tokio_stream::iter(vec![config(3, 8), actions(agents), actions(agents)]);
        let mut responses = client.rehearse(requests).await.unwrap().into_inner();

        let first = responses.message().await.unwrap().unwrap();
        assert_eq!(first.observation_version, harmony::OBSERVATION_VERSION);
        assert_eq!(first.obs_dim as usize, OBS_DIM);
        assert_eq!(first.action_dim as usize, ACTION_DIM);
        let obs = first.observations.unwrap();
        assert_eq!(obs.shape, vec![agents as u32, OBS_DIM as u32]);
        assert_eq!(obs.data.len(), agents * OBS_DIM * 4);

        for _ in 0..2 {
            let step = responses.message().await.unwrap().unwrap();
            assert_eq!(step.rewards.unwrap().data.len(), agents * 4);
            assert_eq!(step.terminated.unwrap().data.len(), agents);
            assert_eq!(
                step.observation_version, 0,
                "only the first response carries metadata"
            );
        }
        assert!(
            responses.message().await.unwrap().is_none(),
            "stream ends when the client closes"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rejects_bad_streams() {
        let mut client = client().await;
        let cases = [
            vec![actions(8)],                 // no config first
            vec![config(0, 8)],               // zero worlds
            vec![config(1, 8), actions(4)],   // wrong action count
            vec![config(1, 8), config(1, 8)], // config twice
        ];
        for requests in cases {
            let result = async {
                let mut responses = client
                    .rehearse(tokio_stream::iter(requests))
                    .await?
                    .into_inner();
                while responses.message().await?.is_some() {}
                Ok::<(), Status>(())
            }
            .await;
            assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        }
    }
}
