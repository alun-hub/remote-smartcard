//! vpcd (Virtual PC/SC Device) integration
//!
//! Implements the vicc (Virtual ICC) protocol to communicate with vpcd.
//! vpcd creates a virtual smartcard reader visible to pcscd, and we
//! forward the APDU commands to the remote client.
//!
//! Protocol format:
//! - 2 bytes: length (big-endian)
//! - 1 byte: command type
//! - N bytes: payload (for APDU commands)
//!
//! Command types:
//! - 0x00: Power Off
//! - 0x01: Power On
//! - 0x02: Reset
//! - 0x03: Get ATR
//! - 0x04: APDU

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tracing::{info, debug, warn, error};

use crate::session_manager::SessionManager;
use rsc_protocol::command_request;

/// vpcd command types
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum VpcdCommand {
    PowerOff = 0x00,
    PowerOn = 0x01,
    Reset = 0x02,
    GetAtr = 0x03,
    Apdu = 0x04,
}

impl TryFrom<u8> for VpcdCommand {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(VpcdCommand::PowerOff),
            0x01 => Ok(VpcdCommand::PowerOn),
            0x02 => Ok(VpcdCommand::Reset),
            0x03 => Ok(VpcdCommand::GetAtr),
            0x04 => Ok(VpcdCommand::Apdu),
            _ => Err(format!("Unknown vpcd command: 0x{:02X}", value)),
        }
    }
}

/// vpcd client state
pub struct VpcdClient {
    /// Session manager for accessing client sessions
    sessions: Arc<RwLock<SessionManager>>,
    /// Session ID to use for forwarding commands
    session_id: Option<String>,
    /// Reader name on the client side
    reader_name: Option<String>,
    /// Current ATR (cached)
    atr: Vec<u8>,
    /// Card powered on
    powered: bool,
}

impl VpcdClient {
    pub fn new(sessions: Arc<RwLock<SessionManager>>) -> Self {
        Self {
            sessions,
            session_id: None,
            reader_name: None,
            atr: Vec::new(),
            powered: false,
        }
    }

    /// Set the session and reader to use
    pub fn set_target(&mut self, session_id: String, reader_name: String) {
        self.session_id = Some(session_id);
        self.reader_name = Some(reader_name);
    }

    /// Connect to vpcd and handle commands
    pub async fn connect(&mut self, host: &str, port: u16) -> Result<(), String> {
        let addr = format!("{}:{}", host, port);
        info!("Connecting to vpcd at {}", addr);

        let mut stream = TcpStream::connect(&addr).await
            .map_err(|e| format!("Failed to connect to vpcd: {}", e))?;

        info!("Connected to vpcd");

        // Main loop - receive commands from vpcd
        loop {
            match self.receive_command(&mut stream).await {
                Ok(Some((cmd, data))) => {
                    let response = self.handle_command(cmd, &data).await;
                    if let Err(e) = self.send_response(&mut stream, &response).await {
                        error!("Failed to send response to vpcd: {}", e);
                        break;
                    }
                }
                Ok(None) => {
                    info!("vpcd connection closed");
                    break;
                }
                Err(e) => {
                    error!("Error receiving from vpcd: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Receive a command from vpcd
    async fn receive_command(&self, stream: &mut TcpStream) -> Result<Option<(VpcdCommand, Vec<u8>)>, String> {
        // Read 2-byte length
        let mut len_buf = [0u8; 2];
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(format!("Failed to read length: {}", e)),
        }

        let len = u16::from_be_bytes(len_buf) as usize;
        if len == 0 {
            return Err("Invalid zero-length command".to_string());
        }

        // Read command and payload
        let mut data = vec![0u8; len];
        stream.read_exact(&mut data).await
            .map_err(|e| format!("Failed to read command data: {}", e))?;

        let cmd = VpcdCommand::try_from(data[0])?;
        let payload = data[1..].to_vec();

        debug!("Received vpcd command: {:?}, payload: {} bytes", cmd, payload.len());

        Ok(Some((cmd, payload)))
    }

    /// Send a response to vpcd
    async fn send_response(&self, stream: &mut TcpStream, data: &[u8]) -> Result<(), String> {
        let len = data.len() as u16;
        let len_bytes = len.to_be_bytes();

        stream.write_all(&len_bytes).await
            .map_err(|e| format!("Failed to write length: {}", e))?;
        stream.write_all(data).await
            .map_err(|e| format!("Failed to write data: {}", e))?;
        stream.flush().await
            .map_err(|e| format!("Failed to flush: {}", e))?;

        debug!("Sent response: {} bytes", data.len());

        Ok(())
    }

    /// Handle a vpcd command
    async fn handle_command(&mut self, cmd: VpcdCommand, data: &[u8]) -> Vec<u8> {
        match cmd {
            VpcdCommand::PowerOff => {
                info!("vpcd: Power Off");
                self.powered = false;
                vec![] // Empty response = success
            }
            VpcdCommand::PowerOn => {
                info!("vpcd: Power On");
                self.powered = true;
                // Return ATR on power on
                self.get_atr_from_client().await
            }
            VpcdCommand::Reset => {
                info!("vpcd: Reset");
                // Return ATR on reset
                self.get_atr_from_client().await
            }
            VpcdCommand::GetAtr => {
                debug!("vpcd: Get ATR");
                self.get_atr_from_client().await
            }
            VpcdCommand::Apdu => {
                debug!("vpcd: APDU ({} bytes)", data.len());
                self.forward_apdu(data).await
            }
        }
    }

    /// Get ATR from the remote client
    async fn get_atr_from_client(&mut self) -> Vec<u8> {
        let (session_id, reader_name) = match (&self.session_id, &self.reader_name) {
            (Some(s), Some(r)) => (s.clone(), r.clone()),
            _ => {
                warn!("No session/reader configured for vpcd");
                return vec![];
            }
        };

        let mut sessions = self.sessions.write().await;
        let session = match sessions.get_session_mut(&session_id) {
            Some(s) => s,
            None => {
                warn!("Session {} not found", session_id);
                return vec![];
            }
        };

        // Send GetAtr command to client
        let command = command_request::Command::GetAtr(rsc_protocol::GetAtrCommand {});

        match session.send_command(command, &reader_name).await {
            Ok(response) => {
                if response.success {
                    if let Some(rsc_protocol::command_response::Response::Atr(atr_resp)) = response.response {
                        self.atr = atr_resp.atr.clone();
                        info!("Got ATR from client: {} bytes", self.atr.len());
                        return self.atr.clone();
                    }
                }
                warn!("Failed to get ATR: {}", response.error);
                vec![]
            }
            Err(e) => {
                error!("Error getting ATR: {}", e);
                vec![]
            }
        }
    }

    /// Forward APDU to the remote client
    async fn forward_apdu(&self, apdu: &[u8]) -> Vec<u8> {
        let (session_id, reader_name) = match (&self.session_id, &self.reader_name) {
            (Some(s), Some(r)) => (s.clone(), r.clone()),
            _ => {
                warn!("No session/reader configured for vpcd");
                // Return error status: 6F00 = No precise diagnosis
                return vec![0x6F, 0x00];
            }
        };

        let mut sessions = self.sessions.write().await;
        let session = match sessions.get_session_mut(&session_id) {
            Some(s) => s,
            None => {
                warn!("Session {} not found", session_id);
                return vec![0x6F, 0x00];
            }
        };

        // Send APDU command to client
        let command = command_request::Command::Apdu(rsc_protocol::ApduCommand {
            apdu: apdu.to_vec(),
        });

        match session.send_command(command, &reader_name).await {
            Ok(response) => {
                if response.success {
                    if let Some(rsc_protocol::command_response::Response::Apdu(apdu_resp)) = response.response {
                        // Build full response: data + SW1 + SW2
                        let mut result = apdu_resp.data;
                        result.push(apdu_resp.sw1 as u8);
                        result.push(apdu_resp.sw2 as u8);
                        debug!("APDU response: {} bytes, SW={:02X}{:02X}",
                               result.len() - 2, apdu_resp.sw1, apdu_resp.sw2);
                        return result;
                    }
                }
                error!("APDU failed: {}", response.error);
                vec![0x6F, 0x00]
            }
            Err(e) => {
                error!("Error forwarding APDU: {}", e);
                vec![0x6F, 0x00]
            }
        }
    }
}

/// vpcd connection manager
/// Manages connections to vpcd for each remote reader
pub struct VpcdManager {
    sessions: Arc<RwLock<SessionManager>>,
    /// Default vpcd port
    default_port: u16,
}

impl VpcdManager {
    pub fn new(sessions: Arc<RwLock<SessionManager>>) -> Self {
        Self {
            sessions,
            default_port: 35963, // Default vpcd port
        }
    }

    /// Start a vpcd client for a specific session/reader
    pub async fn start_client(
        &self,
        session_id: String,
        reader_name: String,
        vpcd_host: String,
        vpcd_port: Option<u16>,
    ) -> Result<(), String> {
        let port = vpcd_port.unwrap_or(self.default_port);
        let sessions = self.sessions.clone();

        tokio::spawn(async move {
            let mut client = VpcdClient::new(sessions);
            client.set_target(session_id.clone(), reader_name.clone());

            loop {
                match client.connect(&vpcd_host, port).await {
                    Ok(_) => {
                        info!("vpcd client for {} disconnected", reader_name);
                    }
                    Err(e) => {
                        error!("vpcd client error for {}: {}", reader_name, e);
                    }
                }

                // Wait before reconnecting
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                info!("Reconnecting to vpcd for {}...", reader_name);
            }
        });

        Ok(())
    }
}
