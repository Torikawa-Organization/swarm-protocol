use thiserror::Error;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::{
    agent::networking::{PacketReadError, PacketWriteError, read_packet, write_packet},
    protocol::agent::{AgentPacket, Packet, ServerPacket, packet},
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
) -> Result<ServerPacket, ServerPacketReadError> {
    let packet = read_packet(reader).await?;
    match packet.packet {
        Some(packet) => match packet {
            packet::Packet::ServerPacket(server_packet) => Ok(server_packet),
            _ => Err(ServerPacketReadError::NoServerPacket),
        },
        _ => Err(ServerPacketReadError::NoPayload),
    }
}

pub async fn write_agent_packet(
    writer: &mut OwnedWriteHalf,
    agent_packet: AgentPacket,
) -> Result<(), PacketWriteError> {
    let packet = Packet {
        packet: Some(packet::Packet::AgentPacket(agent_packet)),
    };

    write_packet(writer, &packet).await
}
