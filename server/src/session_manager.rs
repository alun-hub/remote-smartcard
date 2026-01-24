//! Session management for Remote Smartcard Server

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use tonic::Status;
use tracing::{info, debug, warn};
use uuid::Uuid;

use rsc_protocol::{CommandRequest, CommandResponse};

/// Global command ID counter
static COMMAND_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a unique command ID
fn next_command_id() -> u64 {
    COMMAND_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Information about a reader within a session
#[derive(Debug, Clone)]
pub struct ReaderInfo {
    pub name: String,
    pub atr: Vec<u8>,
    pub card_present: bool,
}

/// Pending command waiting for response
pub struct PendingCommand {
    pub command_id: u64,
    pub response_tx: oneshot::Sender<CommandResponse>,
}

/// Command channel for a session
pub struct CommandChannel {
    /// Sender for commands to the client
    pub command_tx: mpsc::Sender<Result<CommandRequest, Status>>,
    /// Pending commands waiting for responses
    pub pending: HashMap<u64, oneshot::Sender<CommandResponse>>,
}

/// A client session
pub struct Session {
    pub id: String,
    pub client_id: String,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub readers: HashMap<String, ReaderInfo>,
    /// Command channel for sending commands to client (None if client not connected)
    pub command_channel: Option<CommandChannel>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("client_id", &self.client_id)
            .field("created_at", &self.created_at)
            .field("last_activity", &self.last_activity)
            .field("readers", &self.readers)
            .field("command_channel", &self.command_channel.is_some())
            .finish()
    }
}

impl Session {
    fn new(client_id: &str) -> Self {
        let id = Uuid::new_v4().to_string();
        let now = Instant::now();

        Self {
            id,
            client_id: client_id.to_string(),
            created_at: now,
            last_activity: now,
            readers: HashMap::new(),
            command_channel: None,
        }
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Check if session has timed out
    pub fn is_expired(&self, timeout_secs: u64) -> bool {
        self.last_activity.elapsed().as_secs() > timeout_secs
    }

    /// Add or update a reader
    pub fn update_reader(&mut self, name: &str, atr: Vec<u8>, card_present: bool) {
        self.readers.insert(name.to_string(), ReaderInfo {
            name: name.to_string(),
            atr,
            card_present,
        });
    }

    /// Remove a reader
    pub fn remove_reader(&mut self, name: &str) {
        self.readers.remove(name);
    }

    /// Set the command channel for this session
    pub fn set_command_channel(&mut self, tx: mpsc::Sender<Result<CommandRequest, Status>>) {
        self.command_channel = Some(CommandChannel {
            command_tx: tx,
            pending: HashMap::new(),
        });
        info!("Command channel established for session {}", self.id);
    }

    /// Clear the command channel
    pub fn clear_command_channel(&mut self) {
        if self.command_channel.is_some() {
            info!("Command channel closed for session {}", self.id);
            self.command_channel = None;
        }
    }

    /// Send a command to the client and wait for response
    pub async fn send_command(&mut self, command: rsc_protocol::command_request::Command, reader_name: &str) -> Result<CommandResponse, String> {
        let channel = self.command_channel.as_mut()
            .ok_or_else(|| "Client command channel not connected".to_string())?;

        let command_id = next_command_id();

        // Create oneshot channel for the response
        let (response_tx, response_rx) = oneshot::channel();

        // Store pending command
        channel.pending.insert(command_id, response_tx);

        // Build the request
        let request = CommandRequest {
            command_id,
            command: Some(command),
            reader_name: reader_name.to_string(),
        };

        // Send the command
        channel.command_tx.send(Ok(request)).await
            .map_err(|e| format!("Failed to send command: {}", e))?;

        debug!("Sent command {} to client", command_id);

        // Wait for response (with timeout)
        match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err("Response channel closed".to_string()),
            Err(_) => {
                // Remove pending command on timeout
                channel.pending.remove(&command_id);
                Err("Command timed out".to_string())
            }
        }
    }

    /// Handle a response from the client
    pub fn handle_response(&mut self, response: CommandResponse) -> Result<(), String> {
        let channel = self.command_channel.as_mut()
            .ok_or_else(|| "No command channel".to_string())?;

        let command_id = response.command_id;
        if let Some(tx) = channel.pending.remove(&command_id) {
            if tx.send(response).is_err() {
                warn!("Response receiver dropped for command {}", command_id);
            }
            Ok(())
        } else {
            warn!("No pending command for response ID {}", command_id);
            Err(format!("Unknown command ID: {}", command_id))
        }
    }
}

/// Manages all active sessions
pub struct SessionManager {
    sessions: HashMap<String, Session>,
    session_timeout_secs: u64,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            session_timeout_secs: 300, // 5 minutes default timeout
        }
    }

    /// Create a new session
    pub fn create_session(&mut self, client_id: &str) -> &Session {
        let session = Session::new(client_id);
        let session_id = session.id.clone();
        info!("Creating session {} for client {}", session_id, client_id);

        self.sessions.insert(session_id.clone(), session);
        self.sessions.get(&session_id).unwrap()
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &str) -> Option<&Session> {
        self.sessions.get(session_id)
    }

    /// Get a mutable session by ID
    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut Session> {
        self.sessions.get_mut(session_id)
    }

    /// Update session activity timestamp
    pub fn touch_session(&mut self, session_id: &str) -> bool {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.touch();
            true
        } else {
            false
        }
    }

    /// Remove a session
    pub fn remove_session(&mut self, session_id: &str) -> Option<Session> {
        info!("Removing session {}", session_id);
        self.sessions.remove(session_id)
    }

    /// Clean up expired sessions
    pub fn cleanup_expired(&mut self) -> Vec<String> {
        let timeout = self.session_timeout_secs;
        let expired: Vec<String> = self.sessions
            .iter()
            .filter(|(_, s)| s.is_expired(timeout))
            .map(|(id, _)| id.clone())
            .collect();

        for id in &expired {
            info!("Session {} expired, removing", id);
            self.sessions.remove(id);
        }

        expired
    }

    /// Get number of active sessions
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Set session timeout in seconds
    pub fn set_timeout(&mut self, timeout_secs: u64) {
        self.session_timeout_secs = timeout_secs;
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let mut manager = SessionManager::new();
        let session = manager.create_session("test-client");

        assert!(!session.id.is_empty());
        assert_eq!(session.client_id, "test-client");
        assert_eq!(manager.session_count(), 1);
    }

    #[test]
    fn test_touch_session() {
        let mut manager = SessionManager::new();
        let session = manager.create_session("test-client");
        let session_id = session.id.clone();

        assert!(manager.touch_session(&session_id));
        assert!(!manager.touch_session("nonexistent"));
    }
}
