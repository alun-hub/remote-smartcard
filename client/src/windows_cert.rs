//! Windows Certificate Store integration
//!
//! Provides functions to list and load client certificates from the Windows Certificate Store.
//! Uses schannel crate for certificate store access.
//!
//! Note: Full mTLS support with non-exportable keys requires more complex implementation.

#![cfg(windows)]

use crate::error::{ClientError, Result};
use schannel::cert_store::CertStore;
use tracing::{debug, info};

/// Information about a certificate in the Windows store
#[derive(Debug, Clone)]
pub struct CertInfo {
    /// Certificate thumbprint (SHA1 hash as hex string)
    pub thumbprint: String,
    /// Certificate subject as raw string
    pub subject: String,
    /// Whether the certificate has a private key
    pub has_private_key: bool,
}

/// List all client certificates in the Windows Certificate Store (Current User\My)
pub fn list_certificates() -> Result<Vec<CertInfo>> {
    let store = CertStore::open_current_user("My")
        .map_err(|e| ClientError::Tls(format!("Failed to open certificate store: {}", e)))?;

    let mut certs = Vec::new();

    for cert in store.certs() {
        // Get fingerprint (SHA1 thumbprint)
        let thumbprint = match cert.fingerprint(schannel::cert_context::HashAlgorithm::sha1()) {
            Ok(fp) => hex::encode(fp),
            Err(_) => continue, // Skip certs we can't get fingerprint for
        };

        // Get subject from the certificate friendly name or description
        let subject = cert.friendly_name()
            .or_else(|_| cert.description().map(|d| String::from_utf8_lossy(&d).to_string()))
            .unwrap_or_else(|_| "Unknown".to_string());

        // Check if certificate has a private key by trying to acquire it
        let has_private_key = cert.private_key().acquire().is_ok();

        certs.push(CertInfo {
            thumbprint,
            subject,
            has_private_key,
        });
    }

    Ok(certs)
}

/// Find a certificate in Windows Certificate Store by thumbprint
fn find_certificate_by_thumbprint(thumbprint: &str) -> Result<schannel::cert_context::CertContext> {
    let store = CertStore::open_current_user("My")
        .map_err(|e| ClientError::Tls(format!("Failed to open certificate store: {}", e)))?;

    // Normalize thumbprint (remove spaces, convert to lowercase)
    let thumbprint_normalized = thumbprint
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase();

    debug!("Looking for certificate with thumbprint: {}", thumbprint_normalized);

    for cert in store.certs() {
        let cert_thumbprint = match cert.fingerprint(schannel::cert_context::HashAlgorithm::sha1()) {
            Ok(fp) => hex::encode(fp).to_lowercase(),
            Err(_) => continue,
        };

        if cert_thumbprint == thumbprint_normalized {
            debug!("Found matching certificate");
            return Ok(cert);
        }
    }

    Err(ClientError::Tls(format!(
        "Certificate with thumbprint '{}' not found in Windows store",
        thumbprint
    )))
}

/// Load a client certificate and key from Windows Certificate Store
/// Returns certificate and key as PEM strings for use with tonic
///
/// Note: This only works if the private key is exportable.
pub fn load_certificate_as_pem(thumbprint: &str) -> Result<(String, String)> {
    info!("Loading certificate from Windows store: {}", thumbprint);

    let cert = find_certificate_by_thumbprint(thumbprint)?;

    // Get certificate as DER, then convert to PEM
    let cert_der = cert.to_der();
    let _cert_pem = der_to_pem(&cert_der, "CERTIFICATE");

    let subject = cert.friendly_name()
        .or_else(|_| cert.description().map(|d| String::from_utf8_lossy(&d).to_string()))
        .unwrap_or_else(|_| "Unknown".to_string());

    // Note: schannel doesn't provide a way to export the private key as PEM
    // This would require using Windows CryptoAPI directly (NCryptExportKey)
    // For now, return an error with instructions

    info!("Found certificate: {}", subject);

    Err(ClientError::Tls(format!(
        "Windows Certificate Store mTLS is not yet fully implemented.\n\
         Certificate '{}' was found, but private key export requires additional Windows API integration.\n\n\
         Workaround options:\n\
         1. Export the certificate with private key as .pfx from certmgr.msc, then use:\n\
            openssl pkcs12 -in cert.pfx -out client.crt -clcerts -nokeys\n\
            openssl pkcs12 -in cert.pfx -out client.key -nocerts -nodes\n\
         2. Use file-based certificates directly: --tls-cert client.crt --tls-key client.key",
        subject
    )))
}

/// Convert DER bytes to PEM format
fn der_to_pem(der: &[u8], label: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let b64 = STANDARD.encode(der);

    // Split into 64-character lines
    let lines: Vec<&str> = b64.as_bytes()
        .chunks(64)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect();

    format!(
        "-----BEGIN {}-----\n{}\n-----END {}-----\n",
        label,
        lines.join("\n"),
        label
    )
}

/// Build TLS identity from Windows Certificate Store
/// This is a placeholder - full implementation requires Windows CryptoAPI work
pub fn build_tls_identity(thumbprint: &str) -> Result<tonic::transport::Identity> {
    let (cert_pem, key_pem) = load_certificate_as_pem(thumbprint)?;
    Ok(tonic::transport::Identity::from_pem(cert_pem, key_pem))
}

/// Print certificate list in a human-readable format
pub fn print_certificate_list() -> Result<()> {
    let certs = list_certificates()?;

    if certs.is_empty() {
        println!("No client certificates found in Windows Certificate Store (Current User\\My)");
        println!();
        println!("To import a certificate:");
        println!("  1. Double-click your .pfx/.p12 file");
        println!("  2. Or run: certutil -user -importPFX client.pfx");
        return Ok(());
    }

    println!("Client certificates in Windows Certificate Store (Current User\\My):");
    println!();

    for cert in &certs {
        println!("Thumbprint: {}", cert.thumbprint);
        println!("  Subject:     {}", cert.subject);
        println!(
            "  Private Key: {}",
            if cert.has_private_key { "Yes" } else { "No (cannot use for mTLS)" }
        );
        println!();
    }

    println!("Note: Full Windows Certificate Store mTLS support is not yet implemented.");
    println!("For now, export your certificate as PEM files and use --tls-cert/--tls-key.");
    println!();
    println!("To export from Windows store:");
    println!("  1. Open certmgr.msc");
    println!("  2. Right-click certificate -> All Tasks -> Export");
    println!("  3. Export with private key as .pfx");
    println!("  4. Convert with OpenSSL:");
    println!("     openssl pkcs12 -in cert.pfx -out client.crt -clcerts -nokeys");
    println!("     openssl pkcs12 -in cert.pfx -out client.key -nocerts -nodes");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_certificates() {
        // This test requires Windows and may not find certificates
        let result = list_certificates();
        assert!(result.is_ok());
    }

    #[test]
    fn test_der_to_pem() {
        let der = vec![0x30, 0x82, 0x01, 0x22]; // Minimal DER
        let pem = der_to_pem(&der, "CERTIFICATE");
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(pem.ends_with("-----END CERTIFICATE-----\n"));
    }
}
