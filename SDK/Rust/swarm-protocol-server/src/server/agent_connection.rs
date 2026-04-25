use std::time::Duration;

use thiserror::Error;

use crate::{
    networking::{
        self, PacketReadError, PacketWriteError, UnpinnableAsyncTlsRead, UnpinnableAsyncTlsWrite,
    },
    protocol::{
        self, Version,
        agent::{
            AgentPacket, Packet, ServerPacket,
            agent_packet::Inner as AgentPacketE,
            handshake::{
                AgentHandshakePacket, ServerHandshakePacket, ServerResponseConnection,
                ServerResponseConnectionError, ServerResponseConnectionSuccess,
                agent_handshake_packet::Inner as AgentHandshakePacketE,
                server_handshake_packet::Inner as ServerHandshakePacketE,
                server_response_connection::Inner as ServerResponseConnectionE,
            },
            packet::Inner as PacketE,
            payload::{AgentPayloadPacket, ServerPayloadPacket},
            server_packet::Inner as ServerPacketE,
        },
    },
};

static RESPONSE_CONNECTION_REJECTED_FAILED_TO_READ: Packet = Packet {
    inner: Some(PacketE::ServerPacket(ServerPacket {
        inner: Some(ServerPacketE::ServerHandshakePacket(
            ServerHandshakePacket {
                inner: Some(ServerHandshakePacketE::ServerResponseConnection(
                    ServerResponseConnection {
                        inner: Some(ServerResponseConnectionE::Error(
                            ServerResponseConnectionError::FailedToRead as i32,
                        )),
                    },
                )),
            },
        )),
    })),
};

static RESPONSE_CONNECTION_REJECTED_FAILED_TO_PARSE: Packet = Packet {
    inner: Some(PacketE::ServerPacket(ServerPacket {
        inner: Some(ServerPacketE::ServerHandshakePacket(
            ServerHandshakePacket {
                inner: Some(ServerHandshakePacketE::ServerResponseConnection(
                    ServerResponseConnection {
                        inner: Some(ServerResponseConnectionE::Error(
                            ServerResponseConnectionError::FailedToParse as i32,
                        )),
                    },
                )),
            },
        )),
    })),
};

static RESPONSE_CONNECTION_REJECTED_UNSUPPORTED_PROTOCOL_VERSION: Packet = Packet {
    inner: Some(PacketE::ServerPacket(ServerPacket {
        inner: Some(ServerPacketE::ServerHandshakePacket(
            ServerHandshakePacket {
                inner: Some(ServerHandshakePacketE::ServerResponseConnection(
                    ServerResponseConnection {
                        inner: Some(ServerResponseConnectionE::Error(
                            ServerResponseConnectionError::UnsupportedProtocolVersion as i32,
                        )),
                    },
                )),
            },
        )),
    })),
};

static RESPONSE_CONNECTION_ACCEPTED: Packet = Packet {
    inner: Some(PacketE::ServerPacket(ServerPacket {
        inner: Some(ServerPacketE::ServerHandshakePacket(
            ServerHandshakePacket {
                inner: Some(ServerHandshakePacketE::ServerResponseConnection(
                    ServerResponseConnection {
                        inner: Some(ServerResponseConnectionE::Success(
                            ServerResponseConnectionSuccess {},
                        )),
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
    max_packet_size: usize,
    reader: UnpinnableAsyncTlsRead,
    writer: UnpinnableAsyncTlsWrite,
}

impl AgentConnection {
    pub async fn handshake(
        max_packet_size: usize,
        mut reader: UnpinnableAsyncTlsRead,
        mut writer: UnpinnableAsyncTlsWrite,
    ) -> Result<Self, HandshakeError> {
        let agent_packet_connection =
            match networking::read_packet(&mut reader, Duration::from_secs(2), max_packet_size)
                .await
            {
                Ok(packet) => packet,
                Err(e) => {
                    let _ = networking::write_packet(
                        &mut writer,
                        &RESPONSE_CONNECTION_REJECTED_FAILED_TO_READ,
                        Duration::from_secs(2),
                        max_packet_size,
                    )
                    .await;
                    return Err(HandshakeError::PacketRead(e));
                }
            };

        let agent_request_connection = match agent_packet_connection.inner {
            Some(PacketE::AgentPacket(AgentPacket {
                inner:
                    Some(AgentPacketE::AgentHandshakePacket(AgentHandshakePacket {
                        inner: Some(AgentHandshakePacketE::AgentRequestConnection(req)),
                    })),
            })) => req,
            _ => {
                let _ = networking::write_packet(
                    &mut writer,
                    &RESPONSE_CONNECTION_REJECTED_FAILED_TO_PARSE,
                    Duration::from_secs(2),
                    max_packet_size,
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
                    &RESPONSE_CONNECTION_REJECTED_UNSUPPORTED_PROTOCOL_VERSION,
                    Duration::from_secs(2),
                    max_packet_size,
                )
                .await;
                return Err(HandshakeError::UnknownProtocolVersion(protocol_version));
            }
        };

        let expected_version = protocol::VERSION;
        if protocol_version != expected_version {
            let _ = networking::write_packet(
                &mut writer,
                &RESPONSE_CONNECTION_REJECTED_UNSUPPORTED_PROTOCOL_VERSION,
                Duration::from_secs(2),
                max_packet_size,
            )
            .await;
            return Err(HandshakeError::UnsupportedProtocolVersion {
                expected: expected_version.as_str_name().to_string(),
                found: protocol_version.as_str_name().to_string(),
            });
        }

        networking::write_packet(
            &mut writer,
            &RESPONSE_CONNECTION_ACCEPTED,
            Duration::from_secs(2),
            max_packet_size,
        )
        .await?;

        let identifier = agent_request_connection.identifier;
        Ok(Self {
            identifier,
            max_packet_size,
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
        let max_packet_size = self.max_packet_size;

        let packet = networking::read_packet(reader, timeout, max_packet_size).await?;
        match packet.inner {
            Some(inner) => match inner {
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
        match agent_packet.inner {
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
            inner: Some(PacketE::ServerPacket(packet)),
        };

        let writer = &mut self.writer;
        let max_packet_size = self.max_packet_size;
        networking::write_packet(writer, &packet, timeout, max_packet_size).await
    }

    pub async fn write_payload(
        &mut self,
        packet: ServerPayloadPacket,
        timeout: Duration,
    ) -> Result<(), PacketWriteError> {
        let packet = ServerPacket {
            inner: Some(ServerPacketE::ServerPayloadPacket(packet)),
        };

        self.write_packet(packet, timeout).await
    }
}
