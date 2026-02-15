use std::{io, sync::Arc};

use lum_log::{debug, info, warn};
use thiserror::Error;
use tokio::{
    net::{
        TcpListener, TcpStream, ToSocketAddrs,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::Mutex,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::{
    agent::networking::{PacketReadError, PacketWriteError, read_packet, write_packet},
    protocol::agent::{AgentPacket, Packet, ServerPacket, packet},
};

#[derive(Debug, Error)]
pub enum AgentPacketReadError {
    #[error("Error reading packet: {0}")]
    PacketRead(#[from] PacketReadError),

    #[error("Received packet has no payload")]
    NoPayload,

    #[error("Received packet is not an AgentPacket")]
    NoAgentPacket,
}

pub async fn read_agent_packet(
    reader: &mut OwnedReadHalf,
) -> Result<AgentPacket, AgentPacketReadError> {
    let packet = read_packet(reader).await?;
    match packet.packet {
        Some(packet) => match packet {
            packet::Packet::AgentPacket(agent_packet) => Ok(agent_packet),
            _ => Err(AgentPacketReadError::NoAgentPacket),
        },
        _ => Err(AgentPacketReadError::NoPayload),
    }
}

pub async fn write_server_packet(
    writer: &mut OwnedWriteHalf,
    server_packet: ServerPacket,
) -> Result<(), PacketWriteError> {
    let packet = Packet {
        packet: Some(packet::Packet::ServerPacket(server_packet)),
    };

    write_packet(writer, &packet).await
}

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("Handshake logic is not implemented yet")]
    NotImplemented,

    #[error("IO error during handshake: {0}")]
    IO(#[from] io::Error),

    #[error("Invalid protocol version. Expected {expected}, got {found}")]
    InvalidProtocolVersion { expected: String, found: String },
}

pub struct AgentConnection {
    identifier: String,
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
}

impl AgentConnection {
    pub fn handshake(
        _reader: OwnedReadHalf,
        _writer: OwnedWriteHalf,
    ) -> Result<Self, HandshakeError> {
        Err(HandshakeError::NotImplemented)
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub async fn read_packet(&mut self) -> Result<AgentPacket, AgentPacketReadError> {
        read_agent_packet(&mut self.reader).await
    }

    pub async fn write_packet(
        &mut self,
        server_packet: ServerPacket,
    ) -> Result<(), PacketWriteError> {
        write_server_packet(&mut self.writer, server_packet).await
    }
}

#[derive(Debug, Error)]
pub enum ServerCreateError {
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
}

pub struct ServerState {
    listener: TcpListener,
    connections: Mutex<Vec<AgentConnection>>,
    cancelled: CancellationToken,
    receiver_task: Mutex<Option<JoinHandle<()>>>,
}

pub struct Server {
    state: Arc<ServerState>,
}

impl Server {
    pub async fn bind(
        addr: impl ToSocketAddrs,
        accept_connections: bool,
    ) -> Result<Self, ServerCreateError> {
        let listener = TcpListener::bind(addr).await?;
        info!("Listening on {}", listener.local_addr()?);

        let cancelled = CancellationToken::new();
        let connections = Mutex::new(Vec::new());
        let receiver_task = Mutex::new(None);

        let server = Self {
            state: Arc::new(ServerState {
                cancelled,
                listener,
                connections,
                receiver_task,
            }),
        };

        if accept_connections {
            server.accept_connections().await;
        }

        Ok(server)
    }

    pub async fn accept_connections(&self) {
        let state = self.state();

        let mut lock = state.receiver_task.lock().await;
        if lock.is_some() {
            warn!(
                "accept_connections() was called, but the server is already accepting connections. Ignoring this call."
            );
            return;
        }

        let state = state.clone(); // lock is bound to the original state handle, preventing move
        let receiver_task = tokio::spawn(async move {
            let state = state;
            Self::accept_connections_loop(state).await;
        });

        *lock = Some(receiver_task);
        info!("Server is now accepting incoming connections.");
    }

    async fn accept_connections_loop(state: Arc<ServerState>) {
        loop {
            tokio::select! {
                biased;

                _ = state.cancelled.cancelled() => {
                    debug!("Server is shutting down, cancelling accept_connections_loop.");
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

                    info!("New connection from {}. Performing handshake.", addr);
                    match Self::accept_connection(state.clone(), stream).await {
                        Ok((identifier, connections_count)) => {
                            info!(
                                "Handshake successful for connection from {}. Identified as {}. Total agents: {}",
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
        state: Arc<ServerState>,
        stream: TcpStream,
    ) -> Result<(String, usize), HandshakeError> {
        let (reader, writer) = stream.into_split();
        let connection = AgentConnection::handshake(reader, writer)?;

        let identifier = connection.identifier.clone();
        let connections_count;
        {
            let mut connections = state.connections.lock().await;
            connections.push(connection);
            connections_count = connections.len();
        }

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
        info!("Server has stopped accepting incoming connections.");
    }

    fn state(&self) -> Arc<ServerState> {
        self.state.clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        info!("Server shutting down");
        self.state.cancelled.cancel();
    }
}
