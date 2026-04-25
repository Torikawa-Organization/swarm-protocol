use std::{io, sync::Arc};

use lum_log::{debug, info, warn};
use rustls::ServerConfig;
use thiserror::Error;
use tokio::{
    io::{BufReader, BufWriter},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::Mutex,
    task::JoinHandle,
};
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;

use crate::server::{
    agent_connection::{AgentConnection, HandshakeError},
    agent_connection_manager::{AddAgentConnectionError, AgentConnectionManager},
};

#[derive(Debug, Error)]
pub enum AgentServerCreateError {
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum AcceptConnectionError {
    #[error("TLS handshake error: {0}")]
    Tls(io::Error),

    #[error("Handshake error: {0}")]
    Handshake(#[from] HandshakeError),

    #[error("Failed to add agent connection: {0}")]
    AddAgentConnection(#[from] AddAgentConnectionError),
}

pub struct AgentServerConfig {
    max_packet_size: usize,
}

impl AgentServerConfig {
    pub fn new(max_packet_size: usize) -> Self {
        Self { max_packet_size }
    }
}

pub struct AgentServerState {
    listener: TcpListener,
    config: AgentServerConfig,
    tls_acceptor: TlsAcceptor,
    connection_manager: AgentConnectionManager,
    cancellation_token: CancellationToken,
    receiver_task: Mutex<Option<JoinHandle<()>>>,
}

pub struct AgentServer {
    state: Arc<AgentServerState>,
}

impl AgentServer {
    pub async fn bind(
        addr: impl ToSocketAddrs,
        config: AgentServerConfig,
        tls_config: Arc<ServerConfig>,
        start_accepting_connections: bool,
    ) -> Result<Self, AgentServerCreateError> {
        let listener = TcpListener::bind(addr).await?;
        info!("Starting Agent server on {}", listener.local_addr()?);

        let tls_acceptor = TlsAcceptor::from(tls_config);
        let connection_manager = AgentConnectionManager::new();
        let cancellation_token = CancellationToken::new();
        let receiver_task = Mutex::new(None);

        let server = Self {
            state: Arc::new(AgentServerState {
                listener,
                config,
                tls_acceptor,
                connection_manager,
                cancellation_token,
                receiver_task,
            }),
        };

        if start_accepting_connections {
            server.start_accepting_connections().await;
        }

        Ok(server)
    }

    pub async fn start_accepting_connections(&self) {
        let state = self.state();

        let mut lock = state.receiver_task.lock().await;
        if lock.is_some() {
            warn!(
                "start_accepting_connections() was called, but the server is already accepting connections. Ignoring this call."
            );
            return;
        }

        let state = state.clone(); // lock is bound to the original state handle, preventing move
        let receiver_task = tokio::spawn(async move {
            let state = state;
            Self::accept_connections_loop(state).await;
        });

        *lock = Some(receiver_task);
        info!("Agent server is now accepting incoming connections");
    }

    async fn accept_connections_loop(state: Arc<AgentServerState>) {
        loop {
            tokio::select! {
                biased;

                _ = state.cancellation_token.cancelled() => {
                    debug!("Server is shutting down, cancelling accept_connections_loop");
                    break;
                }

                result = state.listener.accept() => {
                    let (stream, addr) = match result {
                        Ok((stream, addr)) => (stream, addr),
                        Err(e) => {
                            warn!("Failed to accept a connection: {}", e);
                            continue;
                        }
                    };

                    info!("Accepting new connection from {}", addr);
                    match Self::accept_connection(state.clone(), stream).await {
                        Ok((identifier, connections_count)) => {
                            info!(
                                "Accepted connection from {}. Identified as {}. Total agents: {}",
                                addr, identifier, connections_count
                            );
                        }
                        Err(e) => {
                            warn!("Failed to accept connection from {}: {}", addr, e);
                        }
                    }
                }
            }
        }
    }

    async fn accept_connection(
        state: Arc<AgentServerState>,
        stream: TcpStream,
    ) -> Result<(String, usize), AcceptConnectionError> {
        let tls_stream = state
            .tls_acceptor
            .accept(stream)
            .await
            .map_err(AcceptConnectionError::Tls)?;

        let (read_half, write_half) = tokio::io::split(tls_stream);
        let reader = BufReader::new(read_half);
        let writer = BufWriter::new(write_half);

        let connection =
            AgentConnection::handshake(state.config.max_packet_size, reader, writer).await?;

        let identifier = connection.identifier().to_string();
        state.connection_manager.add(connection)?;
        let connections_count = state.connection_manager.count();

        Ok((identifier, connections_count))
    }

    pub async fn stop_accepting_connections(&self) {
        let state = self.state();

        let mut lock = state.receiver_task.lock().await;
        let task = match lock.take() {
            Some(task) => task,
            None => {
                warn!(
                    "stop_accepting_connections() was called, but the server is not currently accepting connections. Ignoring this call."
                );
                return;
            }
        };

        task.abort();
        info!("Agent server has stopped accepting incoming connections");
    }

    fn state(&self) -> Arc<AgentServerState> {
        self.state.clone()
    }
}

impl Drop for AgentServer {
    fn drop(&mut self) {
        info!("Agent server shutting down");
        self.state.cancellation_token.cancel();
    }
}
