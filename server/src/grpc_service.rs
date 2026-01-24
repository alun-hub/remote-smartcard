//! gRPC service implementation for Remote Smartcard

use rsc_protocol::{
    ConnectRequest, ConnectResponse,
    ListReadersRequest, ListReadersResponse, Reader,
    UpdateReaderInfoRequest, UpdateReaderInfoResponse,
    TransmitRequest, TransmitResponse,
    CardConnectRequest, CardConnectResponse,
    CardDisconnectRequest, CardDisconnectResponse,
    GetStatusRequest, GetStatusResponse,
    ControlRequest, ControlResponse,
    HeartbeatRequest, HeartbeatResponse,
    StreamRequest, CardEvent,
    CapabilitiesRequest, CapabilitiesResponse,
    CommandRequest, CommandResponse,
    RemoteSmartcard, RemoteSmartcardServer,
    ServerCapabilities,
};
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, debug, warn, error};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;

use crate::session_manager::SessionManager;
use crate::vpcd::VpcdManager;

/// gRPC service implementation
pub struct SmartcardService {
    sessions: Arc<RwLock<SessionManager>>,
    vpcd_manager: Arc<VpcdManager>,
    vpcd_host: String,
    vpcd_port: u16,
    auto_vpcd: bool,
}

impl SmartcardService {
    pub fn new() -> Self {
        let sessions = Arc::new(RwLock::new(SessionManager::new()));
        Self {
            vpcd_manager: Arc::new(VpcdManager::new(sessions.clone())),
            sessions,
            vpcd_host: "127.0.0.1".to_string(),
            vpcd_port: 35963,
            auto_vpcd: false,
        }
    }

    /// Create with shared session manager and vpcd manager
    pub fn with_sessions(
        sessions: Arc<RwLock<SessionManager>>,
        vpcd_manager: Arc<VpcdManager>,
        vpcd_host: String,
        vpcd_port: u16,
        auto_vpcd: bool,
    ) -> Self {
        Self {
            sessions,
            vpcd_manager,
            vpcd_host,
            vpcd_port,
            auto_vpcd,
        }
    }

    /// Create the gRPC server
    pub fn into_server(self) -> RemoteSmartcardServer<Self> {
        RemoteSmartcardServer::new(self)
    }
}

#[tonic::async_trait]
impl RemoteSmartcard for SmartcardService {
    async fn establish_session(
        &self,
        request: Request<ConnectRequest>,
    ) -> Result<Response<ConnectResponse>, Status> {
        let req = request.into_inner();
        info!("New session request from client: {} (v{})", req.client_id, req.client_version);

        let mut sessions = self.sessions.write().await;
        let session = sessions.create_session(&req.client_id);

        let response = ConnectResponse {
            session_id: session.id.clone(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: 1,
            readers: vec![], // Will be populated when client sends ATR
            capabilities: Some(ServerCapabilities {
                max_connections: 100,
                max_apdu_size: 65536,
                features: vec!["apdu".to_string(), "events".to_string()],
            }),
        };

        info!("Session created: {}", session.id);
        Ok(Response::new(response))
    }

    async fn update_reader_info(
        &self,
        request: Request<UpdateReaderInfoRequest>,
    ) -> Result<Response<UpdateReaderInfoResponse>, Status> {
        let req = request.into_inner();
        info!("Reader update for session {}: '{}' (card_present: {})",
              req.session_id, req.reader_name, req.card_present);

        let mut sessions = self.sessions.write().await;
        let session = sessions.get_session_mut(&req.session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;

        // Check if this is a new reader or an update
        let is_new_reader = !session.readers.contains_key(&req.reader_name);

        // Update reader in session
        session.update_reader(&req.reader_name, req.atr.clone(), req.card_present);

        // Create virtual reader name (for now just prefix with "Virtual ")
        let virtual_reader_name = format!("Virtual PCD ({})", req.reader_name);

        info!("Reader '{}' registered as '{}'", req.reader_name, virtual_reader_name);

        // Start vpcd client for new readers if auto_vpcd is enabled
        if is_new_reader && self.auto_vpcd {
            let session_id = req.session_id.clone();
            let reader_name = req.reader_name.clone();
            let vpcd_host = self.vpcd_host.clone();
            let vpcd_port = self.vpcd_port;
            let vpcd_manager = self.vpcd_manager.clone();

            // Drop the lock before spawning
            drop(sessions);

            info!("Starting vpcd client for reader '{}' -> {}:{}", reader_name, vpcd_host, vpcd_port);
            if let Err(e) = vpcd_manager.start_client(
                session_id,
                reader_name,
                vpcd_host,
                Some(vpcd_port),
            ).await {
                error!("Failed to start vpcd client: {}", e);
            }
        }

        let response = UpdateReaderInfoResponse {
            success: true,
            virtual_reader_name,
        };

        Ok(Response::new(response))
    }

    type StreamCardEventsStream = tokio_stream::wrappers::ReceiverStream<Result<CardEvent, Status>>;

    async fn stream_card_events(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::StreamCardEventsStream>, Status> {
        let req = request.into_inner();
        debug!("Card events stream requested for session: {}", req.session_id);

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // For now, just create an empty stream
        // In the future, we'll forward events from the client

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn list_readers(
        &self,
        request: Request<ListReadersRequest>,
    ) -> Result<Response<ListReadersResponse>, Status> {
        let req = request.into_inner();
        debug!("List readers for session: {}", req.session_id);

        let sessions = self.sessions.read().await;
        let session = sessions.get_session(&req.session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;

        let readers = session.readers.iter().map(|(name, reader_info)| {
            Reader {
                name: name.clone(),
                atr: reader_info.atr.clone(),
                card_present: reader_info.card_present,
                state: 0, // TODO: Map to proper state
                vendor: String::new(),
                model: String::new(),
            }
        }).collect();

        Ok(Response::new(ListReadersResponse { readers }))
    }

    async fn card_connect(
        &self,
        request: Request<CardConnectRequest>,
    ) -> Result<Response<CardConnectResponse>, Status> {
        let req = request.into_inner();
        debug!("Card connect for session: {}, reader: {}", req.session_id, req.reader_name);

        // For now, just return a dummy handle
        let response = CardConnectResponse {
            card_handle: 1,
            active_protocol: req.preferred_protocol,
        };

        Ok(Response::new(response))
    }

    async fn card_disconnect(
        &self,
        request: Request<CardDisconnectRequest>,
    ) -> Result<Response<CardDisconnectResponse>, Status> {
        let req = request.into_inner();
        debug!("Card disconnect for session: {}, handle: {}", req.session_id, req.card_handle);

        Ok(Response::new(CardDisconnectResponse { success: true }))
    }

    async fn transmit(
        &self,
        request: Request<TransmitRequest>,
    ) -> Result<Response<TransmitResponse>, Status> {
        let req = request.into_inner();
        debug!("Transmit APDU for session: {}, handle: {}, {} bytes",
               req.session_id, req.card_handle, req.apdu.len());

        let sessions = self.sessions.read().await;
        let _session = sessions.get_session(&req.session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;

        // TODO: Forward APDU to vpcd and get response
        // For now, return a dummy response (SW: 6D00 = Instruction not supported)
        let response = TransmitResponse {
            data: vec![],
            sw1: 0x6D,
            sw2: 0x00,
            response: vec![0x6D, 0x00],
        };

        Ok(Response::new(response))
    }

    async fn get_status(
        &self,
        request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        let req = request.into_inner();
        debug!("Get status for session: {}, handle: {}", req.session_id, req.card_handle);

        // Return dummy status
        let response = GetStatusResponse {
            reader_name: String::new(),
            state: 0,
            protocol: 0,
            atr: vec![],
        };

        Ok(Response::new(response))
    }

    async fn control(
        &self,
        request: Request<ControlRequest>,
    ) -> Result<Response<ControlResponse>, Status> {
        let req = request.into_inner();
        debug!("Control for session: {}, code: {}", req.session_id, req.control_code);

        Ok(Response::new(ControlResponse { data: vec![] }))
    }

    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        debug!("Heartbeat from session: {}", req.session_id);

        let mut sessions = self.sessions.write().await;
        let valid = sessions.touch_session(&req.session_id);

        let response = HeartbeatResponse {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as u64,
            session_valid: valid,
        };

        Ok(Response::new(response))
    }

    async fn get_capabilities(
        &self,
        _request: Request<CapabilitiesRequest>,
    ) -> Result<Response<CapabilitiesResponse>, Status> {
        let response = CapabilitiesResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_versions: vec![1],
            features: vec!["apdu".to_string(), "events".to_string()],
            build_info: None,
        };

        Ok(Response::new(response))
    }

    type CommandChannelStream = tokio_stream::wrappers::ReceiverStream<Result<CommandRequest, Status>>;

    async fn command_channel(
        &self,
        request: Request<Streaming<CommandResponse>>,
    ) -> Result<Response<Self::CommandChannelStream>, Status> {
        let mut stream = request.into_inner();

        // Create channels for bidirectional communication
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(100);

        // We need to get the session ID from the first message
        // Spawn a task to handle incoming responses from the client
        let sessions = self.sessions.clone();
        let cmd_tx_clone = cmd_tx.clone();

        tokio::spawn(async move {
            let mut session_id: Option<String> = None;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(response) => {
                        // First message sets up the session
                        if session_id.is_none() {
                            session_id = Some(response.session_id.clone());

                            // Register command channel with session
                            let mut sessions_guard = sessions.write().await;
                            if let Some(session) = sessions_guard.get_session_mut(&response.session_id) {
                                session.set_command_channel(cmd_tx_clone.clone());
                                info!("Command channel connected for session: {}", response.session_id);
                            } else {
                                warn!("Session not found for command channel: {}", response.session_id);
                                break;
                            }
                        }

                        // Handle the response
                        if response.command_id > 0 {
                            let mut sessions_guard = sessions.write().await;
                            if let Some(session) = sessions_guard.get_session_mut(session_id.as_ref().unwrap()) {
                                if let Err(e) = session.handle_response(response) {
                                    warn!("Failed to handle response: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving command response: {}", e);
                        break;
                    }
                }
            }

            // Clean up when stream ends
            if let Some(sid) = session_id {
                let mut sessions_guard = sessions.write().await;
                if let Some(session) = sessions_guard.get_session_mut(&sid) {
                    session.clear_command_channel();
                }
                info!("Command channel disconnected for session: {}", sid);
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(cmd_rx)))
    }
}
