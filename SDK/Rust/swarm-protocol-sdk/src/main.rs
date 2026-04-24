use std::{fs::File, io::BufReader, sync::Arc};

use lum_log::{info, log::LevelFilter};
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use swarm_protocol_sdk::server::agent_server::AgentServer;
use tokio::signal;

fn load_tls_config(cert_path: &str, key_path: &str) -> Arc<ServerConfig> {
    let cert_file = File::open(cert_path).expect("Failed to open certificate file");
    let key_file = File::open(key_path).expect("Failed to open private key file");

    let cert_chain = certs(&mut BufReader::new(cert_file))
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to parse certificates");

    let key = private_key(&mut BufReader::new(key_file))
        .expect("Failed to read private key")
        .expect("No private key found in key file");

    //TODO: We need to do better here, but it's okay for concept purposes
    let config = ServerConfig::builder()
        .with_no_client_auth()
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

    let tls_config = load_tls_config("cert.pem", "key.pem");

    let _agent_server = AgentServer::bind("127.0.0.1:3120", "someSecret", tls_config, true)
        .await
        .expect("Agent Server should be created successfully");

    signal::ctrl_c()
        .await
        .expect("Listening for Ctrl+C should work");
    info!("Ctrl+C received, exiting");
}
