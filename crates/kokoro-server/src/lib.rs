use axum::Router;
use axum::routing::{get, post};
use kokoro_core::{CancelToken, Chunker, Engine, Pcm, VoiceId, VoiceSpec};
use std::future::IntoFuture;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

mod api;

pub use api::{ApiError, parse_voice};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8471,
        }
    }
}

pub trait SynthEngine: Send + Sync {
    fn voices(&self) -> Vec<VoiceId>;
    fn synthesize_streaming(
        &self,
        phonemes: &str,
        voice: &VoiceSpec,
        speed: f32,
        chunker: &Chunker,
        cancel: &CancelToken,
        on_chunk: &mut dyn FnMut(Pcm) -> kokoro_core::Result<()>,
    ) -> kokoro_core::Result<()>;
}

impl SynthEngine for Engine {
    fn voices(&self) -> Vec<VoiceId> {
        Engine::voices(self)
    }

    fn synthesize_streaming(
        &self,
        phonemes: &str,
        voice: &VoiceSpec,
        speed: f32,
        chunker: &Chunker,
        cancel: &CancelToken,
        on_chunk: &mut dyn FnMut(Pcm) -> kokoro_core::Result<()>,
    ) -> kokoro_core::Result<()> {
        Engine::synthesize_streaming(self, phonemes, voice, speed, chunker, cancel, on_chunk)
    }
}

pub fn router(engine: Arc<dyn SynthEngine>) -> Router {
    Router::new()
        .route("/v1/audio/speech", post(api::speech))
        .route("/v1/audio/voices", get(api::voices))
        .route("/v1/models", get(api::models))
        .route("/health", get(api::health))
        .with_state(api::AppState { engine })
}

pub struct Server {
    addr: SocketAddr,
    shutdown: watch::Sender<bool>,
    handle: JoinHandle<std::io::Result<()>>,
}

impl Server {
    pub async fn start(engine: Arc<dyn SynthEngine>, config: ServerConfig) -> anyhow::Result<Self> {
        let ServerConfig { host, port } = config;
        let listener = TcpListener::bind((host.as_str(), port)).await?;
        let addr = listener.local_addr()?;
        let (shutdown, mut rx) = watch::channel(false);
        let serve = axum::serve(listener, router(engine)).with_graceful_shutdown(async move {
            let _ = rx.changed().await;
        });
        let handle = tokio::spawn(serve.into_future());
        Ok(Self {
            addr,
            shutdown,
            handle,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub async fn stop(self) -> anyhow::Result<()> {
        let Self {
            addr: _,
            shutdown,
            handle,
        } = self;
        let _ = shutdown.send(true);
        handle.await??;
        Ok(())
    }
}
