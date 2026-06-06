use super::routing::mock_router;
use crate::openapi::ApiRoute;
use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

#[derive(Debug)]
pub struct MockServer {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

impl MockServer {
    pub async fn start(routes: Vec<ApiRoute>, port: u16) -> Result<Self> {
        let app = mock_router(routes);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("failed to bind mock server on {addr}"))?;
        let addr = listener
            .local_addr()
            .context("failed to read server address")?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });

            if let Err(error) = server.await {
                eprintln!("mock server error: {error}");
            }
        });

        Ok(Self {
            addr,
            shutdown: Some(shutdown_tx),
            handle,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }

        let _ = self.handle.await;
    }
}
