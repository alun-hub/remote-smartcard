//! gRPC client for Remote Smartcard

use rsc_protocol::{
    ConnectRequest, ConnectResponse, TransmitRequest, TransmitResponse,
    ListReadersRequest, ListReadersResponse, HeartbeatRequest, HeartbeatResponse,
    RemoteSmartcardClient,
};
use tonic::transport::Channel;
use tracing::{info, debug, error};

use crate::error::{ClientError, Result};

/// gRPC client wrapper for Remote Smartcard service
pub struct GrpcClient {
    client: RemoteSmartcardClient<Channel>,
    session_id: Option<String>,
}

impl GrpcClient {
    /// Connect to the remote smartcard server
    pub async fn connect(endpoint: &str) -> Result<Self> {
        info!("Connecting to server: {}", endpoint);

        let client = RemoteSmartcardClient::connect(endpoint.to_string())
            .await
            .map_err(|e| ClientError::Connection(format!("Failed to connect: {}", e)))?;

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
}
