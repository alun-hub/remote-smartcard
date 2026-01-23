//! Session management for Remote Smartcard Server

use std::collections::HashMap;
use std::time::Instant;
use tracing::{info, debug};
use uuid::Uuid;

/// Information about a reader within a session
#[derive(Debug, Clone)]
pub struct ReaderInfo {
    pub name: String,
    pub atr: Vec<u8>,
    pub card_present: bool,
}

/// A client session
#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub client_id: String,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub readers: HashMap<String, ReaderInfo>,
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
