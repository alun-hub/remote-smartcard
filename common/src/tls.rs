//! TLS utilities for certificate loading and configuration
//!
//! Provides utilities for setting up TLS and mTLS (mutual TLS) for
//! secure communication between rsc-client and rsc-server.

// Note: Using full paths (rustls::pki_types::*) in function signatures for clarity
use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use crate::{Error, Result};

/// Load certificates from a PEM file
pub fn load_certs(path: &Path) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let certs: Vec<_> = certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Tls(format!("Failed to load certificates: {}", e)))?;

    if certs.is_empty() {
        return Err(Error::Tls(format!("No certificates found in {:?}", path)));
    }

    Ok(certs)
}

/// Load private key from a PEM file
pub fn load_private_key(path: &Path) -> Result<rustls::pki_types::PrivateKeyDer<'static>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let key = private_key(&mut reader)
        .map_err(|e| Error::Tls(format!("Failed to load private key: {}", e)))?
        .ok_or_else(|| Error::Tls(format!("No private key found in {:?}", path)))?;

    Ok(key)
}

/// Load root CA certificates for verification
pub fn load_root_certs(path: &Path) -> Result<rustls::RootCertStore> {
    let certs = load_certs(path)?;
    let mut root_store = rustls::RootCertStore::empty();

    for cert in certs {
        root_store.add(cert)
            .map_err(|e| Error::Tls(format!("Failed to add root certificate: {}", e)))?;
    }

    Ok(root_store)
}

/// TLS configuration for the server
#[derive(Clone)]
pub struct ServerTlsConfig {
    /// Server certificate chain
    pub cert_path: std::path::PathBuf,
    /// Server private key
    pub key_path: std::path::PathBuf,
    /// CA certificate for client verification (enables mTLS if set)
    pub ca_path: Option<std::path::PathBuf>,
    /// Require client certificate (mTLS)
    pub require_client_cert: bool,
}

impl ServerTlsConfig {
    /// Create a new server TLS config
    pub fn new(cert_path: impl Into<std::path::PathBuf>, key_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
            ca_path: None,
            require_client_cert: false,
        }
    }

    /// Enable mTLS with the given CA certificate
    pub fn with_client_auth(mut self, ca_path: impl Into<std::path::PathBuf>, required: bool) -> Self {
        self.ca_path = Some(ca_path.into());
        self.require_client_cert = required;
        self
    }

    /// Build the rustls ServerConfig
    pub fn build(&self) -> Result<rustls::ServerConfig> {
        let certs = load_certs(&self.cert_path)?;
        let key = load_private_key(&self.key_path)?;

        let builder = rustls::ServerConfig::builder();

        let config = if let Some(ca_path) = &self.ca_path {
            // mTLS: verify client certificates
            let root_store = load_root_certs(ca_path)?;

            let client_cert_verifier = if self.require_client_cert {
                rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
                    .build()
                    .map_err(|e| Error::Tls(format!("Failed to build client verifier: {}", e)))?
            } else {
                rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
                    .allow_unauthenticated()
                    .build()
                    .map_err(|e| Error::Tls(format!("Failed to build client verifier: {}", e)))?
            };

            builder
                .with_client_cert_verifier(client_cert_verifier)
                .with_single_cert(certs, key)
                .map_err(|e| Error::Tls(format!("Failed to build server config: {}", e)))?
        } else {
            // No client authentication
            builder
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .map_err(|e| Error::Tls(format!("Failed to build server config: {}", e)))?
        };

        Ok(config)
    }
}

/// TLS configuration for the client
#[derive(Clone)]
pub struct ClientTlsConfig {
    /// CA certificate for server verification
    pub ca_path: std::path::PathBuf,
    /// Client certificate (for mTLS)
    pub cert_path: Option<std::path::PathBuf>,
    /// Client private key (for mTLS)
    pub key_path: Option<std::path::PathBuf>,
    /// Server name for verification (SNI)
    pub server_name: Option<String>,
    /// Skip server certificate verification (INSECURE - for testing only)
    pub danger_skip_verify: bool,
}

impl ClientTlsConfig {
    /// Create a new client TLS config with CA verification
    pub fn new(ca_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            ca_path: ca_path.into(),
            cert_path: None,
            key_path: None,
            server_name: None,
            danger_skip_verify: false,
        }
    }

    /// Add client certificate for mTLS
    pub fn with_client_cert(
        mut self,
        cert_path: impl Into<std::path::PathBuf>,
        key_path: impl Into<std::path::PathBuf>,
    ) -> Self {
        self.cert_path = Some(cert_path.into());
        self.key_path = Some(key_path.into());
        self
    }

    /// Set the server name for SNI
    pub fn with_server_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = Some(name.into());
        self
    }

    /// Skip server certificate verification (INSECURE)
    pub fn danger_skip_server_verification(mut self) -> Self {
        self.danger_skip_verify = true;
        self
    }

    /// Build the rustls ClientConfig
    pub fn build(&self) -> Result<rustls::ClientConfig> {
        let root_store = load_root_certs(&self.ca_path)?;

        let builder = rustls::ClientConfig::builder()
            .with_root_certificates(root_store);

        let config = if let (Some(cert_path), Some(key_path)) = (&self.cert_path, &self.key_path) {
            // mTLS: provide client certificate
            let certs = load_certs(cert_path)?;
            let key = load_private_key(key_path)?;

            builder
                .with_client_auth_cert(certs, key)
                .map_err(|e| Error::Tls(format!("Failed to build client config: {}", e)))?
        } else {
            // No client certificate
            builder.with_no_client_auth()
        };

        Ok(config)
    }
}

/// Create a tonic TLS config for the server
#[cfg(feature = "tonic")]
pub fn create_server_tls_config(config: &ServerTlsConfig) -> Result<tonic::transport::ServerTlsConfig> {
    use std::fs;

    let cert = fs::read_to_string(&config.cert_path)
        .map_err(|e| Error::Tls(format!("Failed to read certificate: {}", e)))?;
    let key = fs::read_to_string(&config.key_path)
        .map_err(|e| Error::Tls(format!("Failed to read private key: {}", e)))?;

    let identity = tonic::transport::Identity::from_pem(cert, key);

    let mut tls_config = tonic::transport::ServerTlsConfig::new()
        .identity(identity);

    if let Some(ca_path) = &config.ca_path {
        let ca_cert = fs::read_to_string(ca_path)
            .map_err(|e| Error::Tls(format!("Failed to read CA certificate: {}", e)))?;
        let ca = tonic::transport::Certificate::from_pem(ca_cert);
        tls_config = tls_config.client_ca_root(ca);
    }

    Ok(tls_config)
}

/// Create a tonic TLS config for the client
#[cfg(feature = "tonic")]
pub fn create_client_tls_config(config: &ClientTlsConfig) -> Result<tonic::transport::ClientTlsConfig> {
    use std::fs;

    let ca_cert = fs::read_to_string(&config.ca_path)
        .map_err(|e| Error::Tls(format!("Failed to read CA certificate: {}", e)))?;
    let ca = tonic::transport::Certificate::from_pem(ca_cert);

    let mut tls_config = tonic::transport::ClientTlsConfig::new()
        .ca_certificate(ca);

    if let Some(server_name) = &config.server_name {
        tls_config = tls_config.domain_name(server_name);
    }

    if let (Some(cert_path), Some(key_path)) = (&config.cert_path, &config.key_path) {
        let cert = fs::read_to_string(cert_path)
            .map_err(|e| Error::Tls(format!("Failed to read client certificate: {}", e)))?;
        let key = fs::read_to_string(key_path)
            .map_err(|e| Error::Tls(format!("Failed to read client key: {}", e)))?;
        let identity = tonic::transport::Identity::from_pem(cert, key);
        tls_config = tls_config.identity(identity);
    }

    Ok(tls_config)
}
