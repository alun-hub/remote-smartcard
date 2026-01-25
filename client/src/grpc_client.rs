//! gRPC client for Remote Smartcard

#![allow(dead_code)]

use rsc_protocol::{
    ConnectRequest, ConnectResponse, TransmitRequest, TransmitResponse,
    ListReadersRequest, ListReadersResponse, HeartbeatRequest, HeartbeatResponse,
    UpdateReaderInfoRequest, UpdateReaderInfoResponse,
    CommandRequest, CommandResponse,
    command_request, command_response,
    RemoteSmartcardClient,
};
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tracing::{info, debug, error, warn};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::{ClientError, Result};
use crate::pcsc_reader;
use hex;

/// gRPC client wrapper for Remote Smartcard service
pub struct GrpcClient {
    client: RemoteSmartcardClient<Channel>,
    session_id: Option<String>,
}

impl GrpcClient {
    /// Connect to the remote smartcard server (no TLS)
    pub async fn connect(endpoint: &str) -> Result<Self> {
        Self::connect_with_tls(endpoint, None).await
    }

    /// Connect to the remote smartcard server with optional TLS
    pub async fn connect_with_tls(endpoint: &str, tls_config: Option<ClientTlsConfig>) -> Result<Self> {
        info!("Connecting to server: {}", endpoint);

        let channel = if let Some(tls) = tls_config {
            info!("TLS enabled");
            Endpoint::from_shared(endpoint.to_string())
                .map_err(|e| ClientError::Connection(format!("Invalid endpoint: {}", e)))?
                .tls_config(tls)
                .map_err(|e| ClientError::Tls(format!("TLS config error: {}", e)))?
                .connect()
                .await
                .map_err(|e| ClientError::Connection(format!("Failed to connect: {}", e)))?
        } else {
            Endpoint::from_shared(endpoint.to_string())
                .map_err(|e| ClientError::Connection(format!("Invalid endpoint: {}", e)))?
                .connect()
                .await
                .map_err(|e| ClientError::Connection(format!("Failed to connect: {}", e)))?
        };

        let client = RemoteSmartcardClient::new(channel);

        info!("Connected to server");

        Ok(Self {
            client,
            session_id: None,
        })
    }

    /// Establish a session with the server
    pub async fn establish_session(&mut self, client_id: &str, version: &str) -> Result<ConnectResponse> {
        debug!("Establishing session for client: {}", client_id);

        let request = ConnectRequest {
            client_id: client_id.to_string(),
            client_version: version.to_string(),
            protocol_version: 1,
            capabilities: None,
        };

        let response = self.client
            .establish_session(request)
            .await
            .map_err(|e| ClientError::Connection(format!("Session establishment failed: {}", e)))?
            .into_inner();

        self.session_id = Some(response.session_id.clone());
        info!("Session established: {}", response.session_id);

        Ok(response)
    }

    /// Get the current session ID
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// List readers on the server
    pub async fn list_readers(&mut self) -> Result<ListReadersResponse> {
        let session_id = self.session_id.clone()
            .ok_or_else(|| ClientError::Connection("No session established".to_string()))?;

        let request = ListReadersRequest { session_id };

        let response = self.client
            .list_readers(request)
            .await
            .map_err(|e| ClientError::Connection(format!("List readers failed: {}", e)))?
            .into_inner();

        Ok(response)
    }

    /// Transmit APDU to card
    pub async fn transmit(&mut self, reader_name: &str, card_handle: u64, apdu: &[u8]) -> Result<TransmitResponse> {
        let session_id = self.session_id.clone()
            .ok_or_else(|| ClientError::Connection("No session established".to_string()))?;

        debug!("Transmitting APDU ({} bytes) to reader: {}", apdu.len(), reader_name);

        let request = TransmitRequest {
            session_id,
            card_handle,
            apdu: apdu.to_vec(),
            protocol: 0, // Any protocol
        };

        let response = self.client
            .transmit(request)
            .await
            .map_err(|e| ClientError::Connection(format!("Transmit failed: {}", e)))?
            .into_inner();

        debug!("Got response: SW1={:02X} SW2={:02X}", response.sw1, response.sw2);

        Ok(response)
    }

    /// Send heartbeat
    pub async fn heartbeat(&mut self) -> Result<HeartbeatResponse> {
        let session_id = self.session_id.clone()
            .ok_or_else(|| ClientError::Connection("No session established".to_string()))?;

        let request = HeartbeatRequest {
            session_id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as u64,
        };

        let response = self.client
            .heartbeat(request)
            .await
            .map_err(|e| ClientError::Connection(format!("Heartbeat failed: {}", e)))?
            .into_inner();

        if !response.session_valid {
            error!("Session is no longer valid");
            self.session_id = None;
        }

        Ok(response)
    }

    /// Register a reader with the server
    pub async fn register_reader(&mut self, name: &str, atr: Vec<u8>, card_present: bool) -> Result<UpdateReaderInfoResponse> {
        let session_id = self.session_id.clone()
            .ok_or_else(|| ClientError::Connection("No session established".to_string()))?;

        debug!("Registering reader '{}' (card_present: {})", name, card_present);

        let request = UpdateReaderInfoRequest {
            session_id,
            reader_name: name.to_string(),
            atr,
            card_present,
        };

        let response = self.client
            .update_reader_info(request)
            .await
            .map_err(|e| ClientError::Connection(format!("Register reader failed: {}", e)))?
            .into_inner();

        if response.success {
            info!("Reader '{}' registered as '{}'", name, response.virtual_reader_name);
        } else {
            warn!("Failed to register reader '{}'", name);
        }

        Ok(response)
    }

    /// Start the command channel for receiving commands from server
    /// Returns a receiver for notification when the channel closes
    pub async fn start_command_channel(&mut self) -> Result<mpsc::Receiver<()>> {
        let session_id = self.session_id.clone()
            .ok_or_else(|| ClientError::Connection("No session established".to_string()))?;

        info!("Starting command channel for session: {}", session_id);

        // Create channel for shutdown notification
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        // Clone what we need for the spawned task
        let session_id_clone = session_id.clone();
        let mut client_clone = self.client.clone();

        // Spawn task that owns the entire stream lifecycle
        // This ensures response_tx stays alive as long as the stream needs it
        tokio::spawn(async move {
            use tokio_stream::StreamExt;

            // Create channel for sending responses to the server
            // IMPORTANT: response_tx must stay alive to keep the stream open!
            let (response_tx, response_rx) = mpsc::channel::<CommandResponse>(100);

            // Send initial message to register the session
            let initial_response = CommandResponse {
                command_id: 0,
                session_id: session_id_clone.clone(),
                success: true,
                error: String::new(),
                response: None,
            };
            if let Err(e) = response_tx.send(initial_response).await {
                error!("Failed to send initial response: {}", e);
                let _ = shutdown_tx.send(()).await;
                return;
            }

            // Start the bidirectional stream
            let stream = ReceiverStream::new(response_rx);
            let mut cmd_stream = match client_clone.command_channel(stream).await {
                Ok(response) => response.into_inner(),
                Err(e) => {
                    error!("Failed to start command channel: {}", e);
                    let _ = shutdown_tx.send(()).await;
                    return;
                }
            };

            // Keep response_tx alive - don't drop it!
            // We use a separate variable to make this explicit
            let _keep_stream_open = response_tx;

            info!("Command channel handler started for session: {}", session_id_clone);
            info!("Waiting for commands from server...");

            while let Some(result) = cmd_stream.next().await {
                info!("Command channel received a message from server");
                match result {
                    Ok(request) => {
                        let command_id = request.command_id;
                        info!("Received command {} for reader '{}' (has command: {})",
                              command_id, request.reader_name, request.command.is_some());

                        // Process the command
                        info!("Processing command {}...", command_id);
                        let response = Self::process_command(&session_id_clone, request);
                        info!("Command {} processed, success={}, error={}", command_id, response.success, response.error);

                        // Send response via direct RPC (bypasses streaming issues)
                        info!("Sending response for command {} via direct RPC", command_id);
                        match client_clone.send_command_response(response).await {
                            Ok(ack) => {
                                let ack = ack.into_inner();
                                info!("Response for command {} sent successfully, ack.success={}", command_id, ack.success);
                            }
                            Err(e) => {
                                error!("Failed to send response via RPC: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving command: {}", e);
                        break;
                    }
                }
            }

            info!("Command channel closed for session: {}", session_id_clone);
            let _ = shutdown_tx.send(()).await;
        });

        Ok(shutdown_rx)
    }

    /// Process a command from the server
    fn process_command(session_id: &str, request: CommandRequest) -> CommandResponse {
        let command_id = request.command_id;
        let reader_name = request.reader_name.clone();

        info!("process_command: cmd_id={}, reader='{}', command={:?}",
              command_id, reader_name, request.command.as_ref().map(|c| match c {
                  command_request::Command::Apdu(_) => "Apdu",
                  command_request::Command::Connect(_) => "Connect",
                  command_request::Command::Disconnect(_) => "Disconnect",
                  command_request::Command::GetAtr(_) => "GetAtr",
              }));

        let (success, error, response) = match request.command {
            Some(command_request::Command::Apdu(apdu_cmd)) => {
                info!("process_command: executing APDU command");
                Self::handle_apdu_command(&reader_name, &apdu_cmd.apdu)
            }
            Some(command_request::Command::Connect(connect_cmd)) => {
                info!("process_command: executing Connect command");
                Self::handle_connect_command(&reader_name, connect_cmd)
            }
            Some(command_request::Command::Disconnect(disconnect_cmd)) => {
                info!("process_command: executing Disconnect command");
                Self::handle_disconnect_command(&reader_name, disconnect_cmd)
            }
            Some(command_request::Command::GetAtr(_)) => {
                info!("process_command: executing GetAtr command");
                Self::handle_get_atr_command(&reader_name)
            }
            None => {
                warn!("process_command: received empty command");
                (false, "Empty command".to_string(), None)
            }
        };

        info!("process_command: result success={}, error={}", success, error);

        CommandResponse {
            command_id,
            session_id: session_id.to_string(),
            success,
            error,
            response,
        }
    }

    /// Handle APDU command - transmit to local card
    fn handle_apdu_command(reader_name: &str, apdu: &[u8]) -> (bool, String, Option<command_response::Response>) {
        debug!("Executing APDU ({} bytes) on reader '{}'", apdu.len(), reader_name);

        match pcsc_reader::transmit_apdu(reader_name, apdu) {
            Ok(response_data) => {
                let len = response_data.len();
                if len >= 2 {
                    let sw1 = response_data[len - 2] as u32;
                    let sw2 = response_data[len - 1] as u32;
                    let data = response_data[..len - 2].to_vec();

                    debug!("APDU response: {} bytes, SW={:02X}{:02X}", data.len(), sw1, sw2);

                    (true, String::new(), Some(command_response::Response::Apdu(
                        command_response::ApduResponse {
                            data,
                            sw1,
                            sw2,
                        }
                    )))
                } else {
                    (false, "Invalid response length".to_string(), None)
                }
            }
            Err(e) => {
                error!("APDU transmission failed: {}", e);
                (false, format!("APDU transmission failed: {}", e), None)
            }
        }
    }

    /// Handle card connect command
    fn handle_connect_command(reader_name: &str, _cmd: rsc_protocol::CardConnectCommand) -> (bool, String, Option<command_response::Response>) {
        debug!("Connecting to card on reader '{}'", reader_name);

        // For now, just get the ATR as proof of connection
        match pcsc_reader::get_atr(reader_name) {
            Ok(atr) => {
                (true, String::new(), Some(command_response::Response::Connect(
                    command_response::ConnectResponse {
                        card_handle: 1, // Simple handle for now
                        active_protocol: 0, // T=0
                        atr,
                    }
                )))
            }
            Err(e) => {
                error!("Card connect failed: {}", e);
                (false, format!("Card connect failed: {}", e), None)
            }
        }
    }

    /// Handle card disconnect command
    fn handle_disconnect_command(reader_name: &str, _cmd: rsc_protocol::CardDisconnectCommand) -> (bool, String, Option<command_response::Response>) {
        debug!("Disconnecting from card on reader '{}'", reader_name);

        // PC/SC handles connection state, just acknowledge
        (true, String::new(), Some(command_response::Response::Disconnect(
            command_response::DisconnectResponse {}
        )))
    }

    /// Handle get ATR command
    fn handle_get_atr_command(reader_name: &str) -> (bool, String, Option<command_response::Response>) {
        info!("handle_get_atr_command: Getting ATR from reader '{}'", reader_name);

        match pcsc_reader::get_atr(reader_name) {
            Ok(atr) => {
                info!("handle_get_atr_command: Got ATR ({} bytes): {}", atr.len(), hex::encode(&atr));
                (true, String::new(), Some(command_response::Response::Atr(
                    command_response::AtrResponse {
                        atr,
                        card_present: true,
                    }
                )))
            }
            Err(e) => {
                let err_str = e.to_string();
                info!("handle_get_atr_command: get_atr error: {}", err_str);
                // No card present is not an error
                if err_str.contains("SCARD_E_NO_SMARTCARD") || err_str.contains("No card") || err_str.contains("NoSmartcard") {
                    info!("handle_get_atr_command: No card present");
                    (true, String::new(), Some(command_response::Response::Atr(
                        command_response::AtrResponse {
                            atr: vec![],
                            card_present: false,
                        }
                    )))
                } else {
                    error!("handle_get_atr_command: Get ATR failed: {}", e);
                    (false, format!("Get ATR failed: {}", e), None)
                }
            }
        }
    }
}
