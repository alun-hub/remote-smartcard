# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-01-24

### Added

- **Core functionality**
  - gRPC-based protocol for smartcard APDU forwarding
  - Bidirectional command channel for server-initiated requests
  - Session management with unique session IDs
  - Reader registration and status tracking

- **Security**
  - TLS support with server certificates
  - Mutual TLS (mTLS) with client certificate authentication
  - Certificate generation tool (rsc-keygen)

- **Reliability**
  - Automatic reconnection with exponential backoff
  - Configurable initial and maximum reconnection delays
  - Heartbeat mechanism for connection health monitoring
  - Graceful shutdown handling

- **vpcd Integration**
  - Virtual smartcard reader support via vpcd
  - Automatic vpcd client management
  - Full vicc protocol implementation

- **Packaging & Deployment**
  - Systemd service files for server and client
  - Installation scripts for Debian/Ubuntu and RHEL/Fedora
  - Debian packaging files
  - RPM spec file
  - Certificate generation scripts

- **Documentation**
  - Comprehensive user guide with examples
  - Developer documentation
  - Architecture documentation
  - Setup guide with step-by-step instructions

### Technical Details

- Built with Rust for memory safety and performance
- Uses tonic for gRPC implementation
- Uses tokio for async runtime
- Uses rustls for TLS implementation
- Uses pcsc-rs for PC/SC bindings

## [0.0.1] - 2026-01-23

### Added

- Initial project structure
- Cargo workspace with client, server, common, protocol, and tools
- Basic protocol definition (smartcard.proto)
- Project documentation (README, ARCHITECTURE, SETUP, DEVELOPMENT)

---

## Version History

| Version | Date | Description |
|---------|------|-------------|
| 0.1.0 | 2026-01-24 | First functional release with TLS and reconnection |
| 0.0.1 | 2026-01-23 | Initial project structure |

## Upgrade Notes

### From 0.0.1 to 0.1.0

This is the first functional release. If you were testing the 0.0.1 structure:

1. Rebuild all binaries: `cargo build --release`
2. Update configuration to include TLS settings
3. Generate certificates using the new keygen tool or scripts
4. Update systemd services if using them
