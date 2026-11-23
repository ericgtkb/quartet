//! quartet-harmony-server: gRPC server for Quartet's world engine.
//!
//! Configuration (environment):
//!
//! - `QUARTET_HARMONY_ADDR`: listen address (default `127.0.0.1:50051`)
//! - `RUST_LOG`: log filter (default `info`)

mod convert;
mod rehearsal;
mod service;
mod stage;

mod generated {
    pub mod quartet {
        pub mod score {
            tonic::include_proto!("quartet.score");
        }
    }
}

use std::error::Error;
use std::io::IsTerminal;
use std::net::SocketAddr;

use tokio::signal::unix::{SignalKind, signal};
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;

use generated::quartet::score::harmony_service_server::HarmonyServiceServer;
use service::Harmony;

const DEFAULT_ADDR: &str = "127.0.0.1:50051";
/// Large enough for a rehearsal of 2^17 agents' observations (~40 MiB).
const MAX_MESSAGE_BYTES: usize = 64 << 20;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    let raw = std::env::var("QUARTET_HARMONY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
    let addr: SocketAddr = raw
        .parse()
        .map_err(|e| format!("invalid QUARTET_HARMONY_ADDR {raw:?}: {e}"))?;

    // The health reporter starts with the overall service ("") SERVING.
    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<HarmonyServiceServer<Harmony>>()
        .await;
    let harmony = HarmonyServiceServer::new(Harmony::default())
        .max_decoding_message_size(MAX_MESSAGE_BYTES)
        .max_encoding_message_size(MAX_MESSAGE_BYTES);

    tracing::info!(%addr, "quartet-harmony-server listening");
    Server::builder()
        .add_service(health_service)
        .add_service(harmony)
        .serve_with_shutdown(addr, shutdown_signal())
        .await?;
    tracing::info!("shut down");
    Ok(())
}

/// Resolves on SIGINT or SIGTERM.
async fn shutdown_signal() {
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}
