use dashmap::DashMap;
use thiserror::Error;

use crate::server::agent_connection::AgentConnection;

#[derive(Debug, Error)]
pub enum AddAgentConnectionError {
    #[error("Agent with identifier {0} already exists")]
    AgentAlreadyExists(String),
}

#[derive(Debug, Error)]
pub enum RemoveAgentConnectionError {
    #[error("Agent with identifier {0} does not exist")]
    AgentNotFound(String),
}

pub struct AgentConnectionManager {
    connections: DashMap<String, AgentConnection>,
}

impl AgentConnectionManager {
    pub fn new() -> Self {
        let connections = DashMap::new();

        Self { connections }
    }

    pub fn add(&self, connection: AgentConnection) -> Result<(), AddAgentConnectionError> {
        let identifier = connection.identifier().to_string();
        if self.connections.contains_key(&identifier) {
            return Err(AddAgentConnectionError::AgentAlreadyExists(identifier));
        }

        //TODO: Start per-agent packet receive task and re-publish packets

        self.connections.insert(identifier, connection);
        Ok(())
    }

    pub fn remove(&self, identifier: impl AsRef<str>) -> Result<(), RemoveAgentConnectionError> {
        let identifier = identifier.as_ref();

        if !self.connections.contains_key(identifier) {
            return Err(RemoveAgentConnectionError::AgentNotFound(
                identifier.to_string(),
            ));
        }

        //TODO: Send disconnect packet

        self.connections.remove(identifier);
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for AgentConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

//TODO: Implement Drop and send disconnect packets to all agents. Maybe needs work in AgentConnection.
