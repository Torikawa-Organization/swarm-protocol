use std::time::Duration;

use thiserror::Error;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::{
    networking::{PacketReadError, PacketWriteError, read_packet, write_packet},
    protocol::agent::{AgentPacket, Packet, ServerPacket, packet::Packet as PacketE},
};

#[derive(Debug, Error)]
pub enum ServerPacketReadError {
    #[error("Error reading packet: {0}")]
    PacketRead(#[from] PacketReadError),

    #[error("Received packet has no payload")]
    NoPayload,

    #[error("Received packet is not a ServerPacket")]
    NoServerPacket,
}

pub async fn read_server_packet(
    reader: &mut OwnedReadHalf,
    timeout: Duration,
) -> Result<ServerPacket, ServerPacketReadError> {
    let packet = read_packet(reader, timeout).await?;
    match packet.packet {
        Some(packet) => match packet {
            PacketE::ServerPacket(server_packet) => Ok(server_packet),
            _ => Err(ServerPacketReadError::NoServerPacket),
        },
        _ => Err(ServerPacketReadError::NoPayload),
    }
}

pub async fn write_agent_packet(
    writer: &mut OwnedWriteHalf,
    agent_packet: AgentPacket,
    timeout: Duration,
) -> Result<(), PacketWriteError> {
    let packet = Packet {
        packet: Some(PacketE::AgentPacket(agent_packet)),
    };

    write_packet(writer, &packet, timeout).await
}
