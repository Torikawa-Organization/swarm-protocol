use std::{fs::File, io::BufReader, net::IpAddr, path::PathBuf, sync::Arc};

use clap::Parser;
use lum_log::{info, log::LevelFilter};
use rustls::{RootCertStore, ServerConfig, server::WebPkiClientVerifier};
use rustls_pemfile::{certs, private_key};
use swarm_protocol_server::server::agent_server::{AgentServer, AgentServerConfig};
use tokio::signal;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the CA certificate used to verify client certificates (PEM)
    #[arg(long, default_value = "ca_cert.pem")]
    ca_cert: PathBuf,

    /// Path to the server TLS certificate (PEM)
    #[arg(long, default_value = "cert.pem")]
    cert: PathBuf,

    /// Path to the server TLS private key (PEM)
    #[arg(long, default_value = "key.pem")]
    key: PathBuf,

    /// Bind address for the server
    #[arg(long, default_value = "0.0.0.0")]
    host: IpAddr,

    // Port to listen on
    #[arg(long, default_value_t = 3120)]
    port: u16,

    /// Maximum packet size in MiB
    #[arg(long, default_value_t = 32)]
    max_packet_size: usize,
}

fn load_tls_config(
    ca_cert_path: &PathBuf,
    cert_path: &PathBuf,
    key_path: &PathBuf,
) -> Arc<ServerConfig> {
    let ca_file = File::open(ca_cert_path).expect("Failed to open CA cert file");
    let mut root_store = RootCertStore::empty();
    for cert in certs(&mut BufReader::new(ca_file)) {
        root_store
            .add(cert.expect("Failed to parse CA cert"))
            .expect("Failed to add CA cert");
    }

    let cert_file = File::open(cert_path).expect("Failed to open certificate file");
    let cert_chain = certs(&mut BufReader::new(cert_file))
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to parse certificates");

    let key_file = File::open(key_path).expect("Failed to open private key file");
    let key = private_key(&mut BufReader::new(key_file))
        .expect("Failed to read private key")
        .expect("No private key found in key file");

    let client_verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
        .build()
        .expect("Failed to build client verifier");

    let config = ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(cert_chain, key)
        .expect("TLS configuration should be valid");

    Arc::new(config)
}

#[tokio::main]
pub async fn main() {
    let logger_config = lum_log::ConfigBuilder::default()
        .root_log_level(LevelFilter::Trace)
        .stdout_console_appender()
        .build()
        .expect("Logger configuration should be valid");
    lum_log::setup(logger_config).expect("Logger should be set up correctly");

    let args = Args::parse();
    let host = args.host;
    let port = args.port;
    let bind_addr = format!("{}:{}", host, port);
    let max_packet_size = args.max_packet_size * 1024 * 1024;

    let config = AgentServerConfig::new(max_packet_size);
    let tls_config = load_tls_config(&args.ca_cert, &args.cert, &args.key);
    let _agent_server = AgentServer::bind(bind_addr, config, tls_config, true)
        .await
        .expect("Agent Server should be created successfully");

    signal::ctrl_c()
        .await
        .expect("Listening for Ctrl+C should work");
    info!("Ctrl+C received, exiting");
}
