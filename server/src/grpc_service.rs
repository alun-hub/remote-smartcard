//! gRPC service implementation for Remote Smartcard

use rsc_protocol::{
    ConnectRequest, ConnectResponse, ConnectResponse as SessionResponse,
    ListReadersRequest, ListReadersResponse, Reader,
    TransmitRequest, TransmitResponse,
    CardConnectRequest, CardConnectResponse,
    CardDisconnectRequest, CardDisconnectResponse,
    GetStatusRequest, GetStatusResponse,
    ControlRequest, ControlResponse,
    HeartbeatRequest, HeartbeatResponse,
    StreamRequest, CardEvent,
    CapabilitiesRequest, CapabilitiesResponse,
    RemoteSmartcard, RemoteSmartcardServer,
    ServerCapabilities,
};
use tonic::{Request, Response, Status};
use tracing::{info, debug, warn};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

use crate::session_manager::{SessionManager, Session};

/// gRPC service implementation
pub struct SmartcardService {
    sessions: Arc<RwLock<SessionManager>>,
}

impl SmartcardService {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(SessionManager::new())),
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
}
