use std::{
    io,
    time::{Duration, Instant},
};

use crate::protocol::agent::Packet;
use prost::Message;
use thiserror::Error;
use tokio::{
    io::{
        AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter, ReadHalf,
        WriteHalf,
    },
    net::TcpStream,
};
use tokio_rustls::server::TlsStream;

const MAX_PACKET_SIZE: usize = 32 * 1024 * 1024; // 32 MB //TODO: Make configurable

type UnpinnableAsyncRead = dyn AsyncRead + Unpin + Send;
type UnpinnableAsyncWrite = dyn AsyncWrite + Unpin + Send;
pub type UnpinnableAsyncTlsRead = BufReader<ReadHalf<TlsStream<TcpStream>>>;
pub type UnpinnableAsyncTlsWrite = BufWriter<WriteHalf<TlsStream<TcpStream>>>;

#[derive(Debug, Error)]
pub enum PacketReadError {
    #[error("Timeout reached while waiting for bytes: {0}")]
    Timeout(String),

    #[error("IO error while reading packet: {0}")]
    IO(#[from] io::Error),

    #[error("Packet size {0} exceeds maximum allowed size of {MAX_PACKET_SIZE} bytes")]
    PacketTooLarge(usize),

    #[error("Error while decoding packet: {0}")]
    Decode(#[from] prost::DecodeError),
}

impl PacketReadError {
    pub fn timeout_from_duration(duration: Duration) -> Self {
        let duration_string = humantime::format_duration(duration).to_string();
        PacketReadError::Timeout(duration_string)
    }
}

pub async fn read_packet(
    reader: &mut UnpinnableAsyncRead,
    timeout: Duration,
) -> Result<Packet, PacketReadError> {
    let now = Instant::now();

    let mut buffer = [0u8; 4]; // u32 is 4 bytes
    match tokio::time::timeout(timeout, reader.read_exact(&mut buffer)).await {
        Ok(Ok(_)) => (),
        Ok(Err(e)) => return Err(PacketReadError::IO(e)),
        Err(_) => {
            return Err(PacketReadError::timeout_from_duration(timeout));
        }
    };

    let packet_size = u32::from_be_bytes(buffer) as usize;
    if packet_size > MAX_PACKET_SIZE {
        return Err(PacketReadError::PacketTooLarge(packet_size));
    }

    let remaining_timeout = timeout
        .checked_sub(now.elapsed())
        .unwrap_or_else(|| Duration::from_secs(0));

    if remaining_timeout.is_zero() {
        return Err(PacketReadError::timeout_from_duration(timeout));
    }

    let mut buffer = vec![0u8; packet_size];

    match tokio::time::timeout(remaining_timeout, reader.read_exact(&mut buffer)).await {
        Ok(Ok(_)) => (),
        Ok(Err(e)) => return Err(PacketReadError::IO(e)),
        Err(_) => {
            return Err(PacketReadError::timeout_from_duration(timeout));
        }
    };

    let agent_packet = Packet::decode(&buffer[..])?;
    Ok(agent_packet)
}

#[derive(Debug, Error)]
pub enum PacketWriteError {
    #[error("Timeout reached while writing bytes: {0}")]
    Timeout(String),

    #[error("IO error while writing packet: {0}")]
    IO(#[from] io::Error),

    #[error("Error while encoding packet: {0}")]
    Encode(#[from] prost::EncodeError),

    #[error("Packet size {0} exceeds maximum allowed size of {MAX_PACKET_SIZE} bytes")]
    PacketTooLarge(usize),
}

impl PacketWriteError {
    pub fn timeout_from_duration(duration: Duration) -> Self {
        let duration_string = humantime::format_duration(duration).to_string();
        PacketWriteError::Timeout(duration_string)
    }
}

pub async fn write_packet(
    writer: &mut UnpinnableAsyncWrite,
    packet: &Packet,
    timeout: Duration,
) -> Result<(), PacketWriteError> {
    let now = Instant::now();

    let mut buffer = Vec::new();
    packet.encode(&mut buffer)?;

    let packet_size = buffer.len() as u32;
    if packet_size as usize > MAX_PACKET_SIZE {
        return Err(PacketWriteError::PacketTooLarge(packet_size as usize));
    }

    let packet_size_bytes = packet_size.to_be_bytes();
    match tokio::time::timeout(timeout, writer.write_all(&packet_size_bytes)).await {
        Ok(Ok(_)) => (),
        Ok(Err(e)) => return Err(PacketWriteError::IO(e)),
        Err(_) => {
            return Err(PacketWriteError::timeout_from_duration(timeout));
        }
    };

    let remaining_timeout = timeout
        .checked_sub(now.elapsed())
        .unwrap_or_else(|| Duration::from_secs(0));

    if remaining_timeout.is_zero() {
        return Err(PacketWriteError::timeout_from_duration(timeout));
    }

    match tokio::time::timeout(remaining_timeout, writer.write_all(&buffer)).await {
        Ok(Ok(_)) => (),
        Ok(Err(e)) => return Err(PacketWriteError::IO(e)),
        Err(_) => {
            return Err(PacketWriteError::timeout_from_duration(timeout));
        }
    };

    Ok(())
}
