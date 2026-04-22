use std::time::Duration;

use thiserror::Error;
use tokio::{
    io::{BufReader, BufWriter},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::{
    networking::{self, PacketReadError, PacketWriteError},
    protocol::{
        self, Version,
        agent::{
            AgentPacket, Packet, ServerPacket,
            agent_packet::Packet as AgentPacketE,
            handshake::{
                AgentHandshakePacket, ServerHandshakePacket, ServerResponseConnection,
                ServerResponseConnectionError, ServerResponseConnectionStatus,
                agent_handshake_packet::Packet as AgentHandshakePacketE,
                server_handshake_packet::Packet as ServerHandshakePacketE,
            },
            packet::Packet as PacketE,
            payload::{AgentPayloadPacket, ServerPayloadPacket},
            server_packet::Packet as ServerPacketE,
        },
    },
};

static RESPONSE_REJECTED_FAILED_TO_PARSE: Packet = Packet {
    packet: Some(PacketE::ServerPacket(ServerPacket {
        packet: Some(ServerPacketE::ServerHandshakePacket(
            ServerHandshakePacket {
                packet: Some(ServerHandshakePacketE::ServerResponseConnection(
                    ServerResponseConnection {
                        status: ServerResponseConnectionStatus::Rejected as i32,
                        error: ServerResponseConnectionError::FailedToParse as i32,
                    },
                )),
            },
        )),
    })),
};

static RESPONSE_REJECTED_FAILED_TO_READ: Packet = Packet {
    packet: Some(PacketE::ServerPacket(ServerPacket {
        packet: Some(ServerPacketE::ServerHandshakePacket(
            ServerHandshakePacket {
                packet: Some(ServerHandshakePacketE::ServerResponseConnection(
                    ServerResponseConnection {
                        status: ServerResponseConnectionStatus::Rejected as i32,
                        error: ServerResponseConnectionError::FailedToRead as i32,
                    },
                )),
            },
        )),
    })),
};

static RESPONSE_REJECTED_UNSUPPORTED_PROTOCOL_VERSION: Packet = Packet {
    packet: Some(PacketE::ServerPacket(ServerPacket {
        packet: Some(ServerPacketE::ServerHandshakePacket(
            ServerHandshakePacket {
                packet: Some(ServerHandshakePacketE::ServerResponseConnection(
                    ServerResponseConnection {
                        status: ServerResponseConnectionStatus::Rejected as i32,
                        error: ServerResponseConnectionError::UnsupportedProtocolVersion as i32,
                    },
                )),
            },
        )),
    })),
};

static RESPONSE_ACCEPTED: Packet = Packet {
    packet: Some(PacketE::ServerPacket(ServerPacket {
        packet: Some(ServerPacketE::ServerHandshakePacket(
            ServerHandshakePacket {
                packet: Some(ServerHandshakePacketE::ServerResponseConnection(
                    ServerResponseConnection {
                        status: ServerResponseConnectionStatus::Accepted as i32,
                        error: ServerResponseConnectionError::Unspecified as i32,
                    },
                )),
            },
        )),
    })),
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

#[derive(Debug, Error)]
pub enum AgentPayloadReadError {
    #[error("Error reading packet: {0}")]
    AgentPacketRead(#[from] AgentPacketReadError),

    #[error("Packet is not an AgentPayloadPacket")]
    NoAgentPayloadPacket,
}

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("Error while reading packet from Agent: {0}")]
    PacketRead(#[from] PacketReadError),

    #[error("Received a packet of unexpected type from Agent during handshake")]
    UnexpectedPacketType,

    #[error("Agent specified an unknown protocol version during handshake: {0}")]
    UnknownProtocolVersion(i32),

    #[error("Unsupported protocol version. Expected {expected}, got {found}")]
    UnsupportedProtocolVersion { expected: String, found: String },

    #[error("Error while writing packet to Agent: {0}")]
    PacketWrite(#[from] PacketWriteError),
}

pub struct AgentConnection {
    identifier: String,
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
}

impl AgentConnection {
    pub async fn handshake(
        mut reader: BufReader<OwnedReadHalf>,
        mut writer: BufWriter<OwnedWriteHalf>,
    ) -> Result<Self, HandshakeError> {
        let connection_request =
            match networking::read_packet(&mut reader, Duration::from_secs(2)).await {
                Ok(packet) => packet,
                Err(e) => {
                    let _ = networking::write_packet(
                        &mut writer,
                        &RESPONSE_REJECTED_FAILED_TO_READ,
                        Duration::from_secs(2),
                    )
                    .await;
                    return Err(HandshakeError::PacketRead(e));
                }
            };

        let agent_request_connection = match connection_request.packet {
            Some(PacketE::AgentPacket(AgentPacket {
                packet:
                    Some(AgentPacketE::AgentHandshakePacket(AgentHandshakePacket {
                        packet: Some(AgentHandshakePacketE::AgentRequestConnection(req)),
                    })),
            })) => req,
            _ => {
                let _ = networking::write_packet(
                    &mut writer,
                    &RESPONSE_REJECTED_FAILED_TO_PARSE,
                    Duration::from_secs(2),
                )
                .await;
                return Err(HandshakeError::UnexpectedPacketType);
            }
        };

        let protocol_version = agent_request_connection.version;
        let protocol_version = match Version::try_from(protocol_version) {
            Ok(version) => version,
            Err(_) => {
                let _ = networking::write_packet(
                    &mut writer,
                    &RESPONSE_REJECTED_UNSUPPORTED_PROTOCOL_VERSION,
                    Duration::from_secs(2),
                )
                .await;
                return Err(HandshakeError::UnknownProtocolVersion(protocol_version));
            }
        };

        let expected_version = protocol::VERSION;
        if protocol_version != expected_version {
            let _ = networking::write_packet(
                &mut writer,
                &RESPONSE_REJECTED_UNSUPPORTED_PROTOCOL_VERSION,
                Duration::from_secs(2),
            )
            .await;
            return Err(HandshakeError::UnsupportedProtocolVersion {
                expected: expected_version.as_str_name().to_string(),
                found: protocol_version.as_str_name().to_string(),
            });
        }

        networking::write_packet(&mut writer, &RESPONSE_ACCEPTED, Duration::from_secs(2)).await?;

        let identifier = agent_request_connection.identifier;
        Ok(Self {
            identifier,
            reader,
            writer,
        })
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub async fn read_packet(
        &mut self,
        timeout: Duration,
    ) -> Result<AgentPacket, AgentPacketReadError> {
        let reader = &mut self.reader;

        let packet = networking::read_packet(reader, timeout).await?;
        match packet.packet {
            Some(packet) => match packet {
                PacketE::AgentPacket(agent_packet) => Ok(agent_packet),
                _ => Err(AgentPacketReadError::NoAgentPacket),
            },
            _ => Err(AgentPacketReadError::NoPayload),
        }
    }

    pub async fn read_payload(
        &mut self,
        timeout: Duration,
    ) -> Result<AgentPayloadPacket, AgentPayloadReadError> {
        let agent_packet = self.read_packet(timeout).await?;
        match agent_packet.packet {
            Some(AgentPacketE::AgentPayloadPacket(payload)) => Ok(payload),
            _ => Err(AgentPayloadReadError::NoAgentPayloadPacket),
        }
    }

    pub async fn write_packet(
        &mut self,
        packet: ServerPacket,
        timeout: Duration,
    ) -> Result<(), PacketWriteError> {
        let packet = Packet {
            packet: Some(PacketE::ServerPacket(packet)),
        };

        let writer = &mut self.writer;
        networking::write_packet(writer, &packet, timeout).await
    }

    pub async fn write_payload(
        &mut self,
        packet: ServerPayloadPacket,
        timeout: Duration,
    ) -> Result<(), PacketWriteError> {
        let packet = ServerPacket {
            packet: Some(ServerPacketE::ServerPayloadPacket(packet)),
        };

        self.write_packet(packet, timeout).await
    }
}
