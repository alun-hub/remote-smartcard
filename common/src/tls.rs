//! TLS utilities for certificate loading and configuration

use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

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
