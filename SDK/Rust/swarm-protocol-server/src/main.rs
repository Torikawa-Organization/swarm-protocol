use std::{fs::File, io::BufReader, path::PathBuf, sync::Arc};

use clap::Parser;
use lum_log::{info, log::LevelFilter};
use rustls::{RootCertStore, ServerConfig, server::WebPkiClientVerifier};
use rustls_pemfile::{certs, private_key};
use swarm_protocol_server::server::agent_server::AgentServer;
use tokio::signal;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the CA certificate used to verify client certificates (PEM)
    ca_cert: PathBuf,

    /// Path to the server TLS certificate (PEM)
    cert: PathBuf,

    /// Path to the server TLS private key (PEM)
    key: PathBuf,
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
    let tls_config = load_tls_config(&args.ca_cert, &args.cert, &args.key);
    let _agent_server = AgentServer::bind("127.0.0.1:3120", "someSecret", tls_config, true)
        .await
        .expect("Agent Server should be created successfully");

    signal::ctrl_c()
        .await
        .expect("Listening for Ctrl+C should work");
    info!("Ctrl+C received, exiting");
}
