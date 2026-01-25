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
use hex;

use crate::session_manager::SessionManager;
use rsc_protocol::command_request;

/// vpcd control command types (single byte commands)
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum VpcdCtrlCommand {
    PowerOff = 0x00,  // VPCD_CTRL_OFF
    PowerOn = 0x01,   // VPCD_CTRL_ON
    Reset = 0x02,     // VPCD_CTRL_RESET
    GetAtr = 0x04,    // VPCD_CTRL_ATR (note: 4, not 3!)
}

impl TryFrom<u8> for VpcdCtrlCommand {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(VpcdCtrlCommand::PowerOff),
            0x01 => Ok(VpcdCtrlCommand::PowerOn),
            0x02 => Ok(VpcdCtrlCommand::Reset),
            0x04 => Ok(VpcdCtrlCommand::GetAtr),
            _ => Err(format!("Unknown vpcd control command: 0x{:02X}", value)),
        }
    }
}

/// vpcd message types
/// Protocol distinguishes by length: len==1 is control command, len>1 is APDU
#[derive(Debug, Clone)]
pub enum VpcdMessage {
    Control(VpcdCtrlCommand),
    Apdu(Vec<u8>),
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
            powered: true, // Card is "powered" when we connect
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
        info!("Connecting to vpcd at {} for session {:?} reader {:?}", addr, self.session_id, self.reader_name);

        let mut stream = TcpStream::connect(&addr).await
            .map_err(|e| format!("Failed to connect to vpcd: {}", e))?;

        // Reset power state on new connection (card is "inserted")
        self.powered = true;

        info!("Connected to vpcd, starting to process commands immediately");
        // NOTE: We don't wait for command channel here anymore!
        // GetATR uses cached ATR, so we can respond immediately.
        // APDUs will wait for command channel when needed.

        // Main loop - receive messages from vpcd
        info!("Entering vpcd message loop");
        loop {
            info!("Waiting for next vpcd message...");
            match self.receive_message(&mut stream).await {
                Ok(Some(msg)) => {
                    info!("Received vpcd message: {:?}", msg);
                    // handle_message returns Some(response) for commands that need a response,
                    // None for commands that don't (PowerOn/PowerOff/Reset)
                    if let Some(response) = self.handle_message(msg).await {
                        info!("Sending response: {} bytes", response.len());
                        if let Err(e) = self.send_response(&mut stream, &response).await {
                            error!("Failed to send response to vpcd: {}", e);
                            break;
                        }
                        info!("Response sent successfully");
                    } else {
                        info!("No response needed for this command");
                    }
                }
                Ok(None) => {
                    info!("vpcd connection closed gracefully (EOF)");
                    break;
                }
                Err(e) => {
                    error!("Error receiving from vpcd: {}", e);
                    break;
                }
            }
        }
        info!("Exited vpcd message loop");

        Ok(())
    }

    /// Receive a message from vpcd
    /// Protocol: [2 bytes length BE] [data]
    /// - If length == 1: data is a control command byte (PowerOff=0, PowerOn=1, Reset=2, GetATR=4)
    /// - If length > 1: data is a raw APDU (no command prefix)
    async fn receive_message(&self, stream: &mut TcpStream) -> Result<Option<VpcdMessage>, String> {
        // Read 2-byte length (big-endian)
        let mut len_buf = [0u8; 2];
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(format!("Failed to read length: {}", e)),
        }

        let len = u16::from_be_bytes(len_buf) as usize;
        if len == 0 {
            return Err("Invalid zero-length message".to_string());
        }

        // Read the data
        let mut data = vec![0u8; len];
        stream.read_exact(&mut data).await
            .map_err(|e| format!("Failed to read message data: {}", e))?;

        // Distinguish between control commands (len==1) and APDUs (len>1)
        if len == 1 {
            // Control command - single byte
            let cmd = VpcdCtrlCommand::try_from(data[0])?;
            debug!("Received vpcd control command: {:?}", cmd);
            Ok(Some(VpcdMessage::Control(cmd)))
        } else {
            // APDU - raw data, no command prefix
            debug!("Received vpcd APDU: {} bytes: {}", len, hex::encode(&data));
            Ok(Some(VpcdMessage::Apdu(data)))
        }
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

        debug!("Sent response to vpcd: {} bytes", data.len());

        Ok(())
    }

    /// Handle a vpcd message (control command or APDU)
    /// Returns Some(response) for commands that need a response (GetATR, APDU)
    /// Returns None for commands that don't need a response (PowerOn, PowerOff, Reset)
    async fn handle_message(&mut self, msg: VpcdMessage) -> Option<Vec<u8>> {
        debug!("vpcd: Handling message {:?}", msg);
        match msg {
            VpcdMessage::Control(cmd) => match cmd {
                VpcdCtrlCommand::PowerOff => {
                    debug!("vpcd: Power Off (no response expected)");
                    self.powered = false;
                    None // No response for PowerOff
                }
                VpcdCtrlCommand::PowerOn => {
                    debug!("vpcd: Power On (no response expected)");
                    self.powered = true;
                    None // No response for PowerOn
                }
                VpcdCtrlCommand::Reset => {
                    debug!("vpcd: Reset (no response expected)");
                    None // No response for Reset
                }
                VpcdCtrlCommand::GetAtr => {
                    debug!("vpcd: Get ATR");
                    let atr = self.get_atr_from_client().await;
                    debug!("vpcd: ATR response {} bytes", atr.len());
                    Some(atr)
                }
            }
            VpcdMessage::Apdu(apdu) => {
                info!("vpcd: APDU ({} bytes) - forwarding to client", apdu.len());
                let response = self.forward_apdu(&apdu).await;
                debug!("vpcd: APDU response {} bytes", response.len());
                Some(response)
            }
        }
    }

    /// Wait for command channel to be available
    async fn wait_for_command_channel(&self) -> bool {
        let (session_id, reader_name) = match (&self.session_id, &self.reader_name) {
            (Some(s), Some(r)) => (s.clone(), r.clone()),
            _ => return false,
        };

        info!("Waiting for command channel for session {} reader {}", session_id, reader_name);

        // Wait up to 30 seconds for command channel
        for i in 0..300 {
            {
                let sessions = self.sessions.read().await;
                if let Some(session) = sessions.get_session(&session_id) {
                    if session.command_channel.is_some() {
                        info!("Command channel ready after {}ms for session {}", i * 100, session_id);
                        return true;
                    }
                } else {
                    warn!("Session {} not found while waiting for command channel", session_id);
                    return false;
                }
            }
            if i % 10 == 0 && i > 0 {
                debug!("Still waiting for command channel... {}ms elapsed", i * 100);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        warn!("Timeout (30s) waiting for command channel for session {}", session_id);
        false
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

        debug!("get_atr_from_client: session={}, reader={}", session_id, reader_name);

        // First, check if we have a cached ATR from the session
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get_session(&session_id) {
                if let Some(reader_info) = session.readers.get(&reader_name) {
                    if !reader_info.atr.is_empty() {
                        debug!("Using cached ATR: {} bytes", reader_info.atr.len());
                        self.atr = reader_info.atr.clone();
                        return self.atr.clone();
                    }
                }
            }
        }

        info!("No cached ATR, fetching from client...");

        // Wait for command channel to be ready
        if !self.wait_for_command_channel().await {
            warn!("Command channel not available for GetATR");
            return vec![];
        }

        info!("get_atr_from_client: command channel ready, sending GetAtr command");

        // Send command and get response receiver - release lock before waiting!
        let response_rx = {
            let mut sessions = self.sessions.write().await;
            let session = match sessions.get_session_mut(&session_id) {
                Some(s) => s,
                None => {
                    warn!("Session {} not found", session_id);
                    return vec![];
                }
            };

            // Send GetAtr command to client - returns receiver for response
            match session.send_command_async(command_request::Command::GetAtr(rsc_protocol::GetAtrCommand {}), &reader_name).await {
                Ok(rx) => {
                    info!("get_atr_from_client: GetAtr command sent, waiting for response");
                    rx
                }
                Err(e) => {
                    error!("Error sending GetAtr command: {}", e);
                    return vec![];
                }
            }
        }; // Lock released here!

        // Now wait for response without holding the lock
        info!("get_atr_from_client: waiting for response (timeout 30s)");
        match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
            Ok(Ok(response)) => {
                info!("get_atr_from_client: got response, success={}, error={}", response.success, response.error);
                if response.success {
                    if let Some(rsc_protocol::command_response::Response::Atr(atr_resp)) = response.response {
                        self.atr = atr_resp.atr.clone();
                        info!("Got ATR from client: {} bytes: {}", self.atr.len(), hex::encode(&self.atr));
                        return self.atr.clone();
                    } else {
                        warn!("get_atr_from_client: response has no ATR data");
                    }
                }
                warn!("Failed to get ATR: {}", response.error);
                vec![]
            }
            Ok(Err(_)) => {
                error!("Response channel closed while waiting for ATR");
                vec![]
            }
            Err(_) => {
                error!("GetAtr command timed out after 30s");
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

        info!("forward_apdu: APDU={} for session={}, reader={}", hex::encode(apdu), session_id, reader_name);

        // Wait for command channel to be ready
        if !self.wait_for_command_channel().await {
            warn!("Command channel not available for APDU");
            return vec![0x6F, 0x00];
        }

        // Send command and get response receiver - release lock before waiting!
        let response_rx = {
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

            match session.send_command_async(command, &reader_name).await {
                Ok(rx) => {
                    info!("forward_apdu: APDU command sent, waiting for response");
                    rx
                }
                Err(e) => {
                    error!("Error sending APDU command: {}", e);
                    return vec![0x6F, 0x00];
                }
            }
        }; // Lock released here!

        // Now wait for response without holding the lock
        match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
            Ok(Ok(response)) => {
                info!("forward_apdu: got response, success={}, error={}", response.success, response.error);
                if response.success {
                    if let Some(rsc_protocol::command_response::Response::Apdu(apdu_resp)) = response.response {
                        // Build full response: data + SW1 + SW2
                        let mut result = apdu_resp.data;
                        result.push(apdu_resp.sw1 as u8);
                        result.push(apdu_resp.sw2 as u8);
                        info!("APDU response: {} bytes, SW={:02X}{:02X}, data={}",
                               result.len() - 2, apdu_resp.sw1, apdu_resp.sw2, hex::encode(&result));
                        return result;
                    }
                }
                error!("APDU failed: {}", response.error);
                vec![0x6F, 0x00]
            }
            Ok(Err(_)) => {
                error!("Response channel closed while waiting for APDU");
                vec![0x6F, 0x00]
            }
            Err(_) => {
                error!("APDU command timed out after 30s");
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
            let mut consecutive_errors = 0u32;

            loop {
                match client.connect(&vpcd_host, port).await {
                    Ok(_) => {
                        // Normal disconnect (pcscd calls vicc_eject periodically)
                        // Small delay to let socket fully close before reconnecting
                        info!("vpcd client for {} disconnected normally, reconnecting in 100ms", reader_name);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        consecutive_errors = 0;
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors <= 3 {
                            warn!("vpcd client error for {}: {} (attempt {})", reader_name, e, consecutive_errors);
                        } else {
                            error!("vpcd client error for {}: {} (attempt {})", reader_name, e, consecutive_errors);
                        }
                        // Only wait on errors, not normal disconnects
                        let delay = std::cmp::min(consecutive_errors as u64, 5);
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    }
                }
            }
        });

        Ok(())
    }
}
