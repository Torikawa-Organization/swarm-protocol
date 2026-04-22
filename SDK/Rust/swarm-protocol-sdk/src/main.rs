use lum_log::{info, log::LevelFilter};
use swarm_protocol_sdk::server::agent_server::AgentServer;
use tokio::signal;

#[tokio::main]
pub async fn main() {
    let logger_config = lum_log::ConfigBuilder::default()
        .root_log_level(LevelFilter::Trace)
        .stdout_console_appender()
        .build()
        .expect("Logger configuration should be valid");
    lum_log::setup(logger_config).expect("Logger should be set up correctly");

    let _agent_server = AgentServer::bind("127.0.0.1:3120", "someSecret", true)
        .await
        .expect("Agent Server should be created successfully");

    signal::ctrl_c()
        .await
        .expect("Listening for Ctrl+C should work");
    info!("Ctrl+C received, exiting");
}
