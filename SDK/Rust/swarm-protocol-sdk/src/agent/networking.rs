use std::io;

use prost::Message;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::protocol::agent::Packet;

const MAX_PACKET_SIZE: usize = 32 * 1024 * 1024; // 32 MB //TODO: Make configurable

#[derive(Debug, Error)]
pub enum PacketReadError {
    #[error("IO error while reading packet: {0}")]
    IO(#[from] io::Error),

    #[error("Packet size {0} exceeds maximum allowed size of {MAX_PACKET_SIZE} bytes")]
    PacketTooLarge(usize),

    #[error("Error while decoding packet: {0}")]
    Decode(#[from] prost::DecodeError),
}

pub async fn read_packet(reader: &mut OwnedReadHalf) -> Result<Packet, PacketReadError> {
    let mut buffer = [0u8; 4]; // u32 is 4 bytes
    reader.read_exact(&mut buffer).await?;

    let packet_size = u32::from_be_bytes(buffer) as usize;
    if packet_size > MAX_PACKET_SIZE {
        return Err(PacketReadError::PacketTooLarge(packet_size));
    }

    let mut buffer = vec![0u8; packet_size];
    reader.read_exact(&mut buffer).await?;

    let agent_packet = Packet::decode(&buffer[..])?;
    Ok(agent_packet)
}

#[derive(Debug, Error)]
pub enum PacketWriteError {
    #[error("IO error while writing packet: {0}")]
    IO(#[from] io::Error),

    #[error("Error while encoding packet: {0}")]
    Encode(#[from] prost::EncodeError),

    #[error("Packet size {0} exceeds maximum allowed size of {MAX_PACKET_SIZE} bytes")]
    PacketTooLarge(usize),
}

pub async fn write_packet(
    writer: &mut OwnedWriteHalf,
    packet: &Packet,
) -> Result<(), PacketWriteError> {
    let mut buffer = Vec::new();
    packet.encode(&mut buffer)?;

    let packet_size = buffer.len() as u32;
    if packet_size as usize > MAX_PACKET_SIZE {
        return Err(PacketWriteError::PacketTooLarge(packet_size as usize));
    }

    let packet_size_bytes = packet_size.to_be_bytes();
    writer.write_all(&packet_size_bytes).await?;
    writer.write_all(&buffer).await?;

    Ok(())
}
