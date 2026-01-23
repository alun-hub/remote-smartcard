//! gRPC protocol definitions for Remote Smartcard
//!
//! This crate contains the generated gRPC client and server code
//! from the smartcard.proto definition.

// Include the generated protobuf code
tonic::include_proto!("rsc");

// Re-export commonly used types for convenience
pub use remote_smartcard_client::RemoteSmartcardClient;
pub use remote_smartcard_server::{RemoteSmartcard, RemoteSmartcardServer};
