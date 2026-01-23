# Development Plan - Remote Smartcard

## Sammanfattning

Detta dokument beskriver utvecklingsplanen för Remote Smartcard-projektet.

## Tech Stack

### Programmering
- **Språk**: Rust
- **gRPC Framework**: tonic
- **Async Runtime**: tokio
- **TLS**: rustls / tokio-rustls
- **PC/SC Bindings**: pcsc-rs
- **Config**: serde + serde_yaml
- **Logging**: tracing

### Motivering för Rust
1. **Memory safety** - Kritiskt för network-facing security software
2. **Excellent error handling** - Result<T,E> och ? operator
3. **Great async support** - tokio är väldigt mature
4. **Strong ecosystem** - tonic (gRPC), rustls (TLS), pcsc-rs (PC/SC)
5. **Performance** - Zero-cost abstractions
6. **Safety** - Compile-time guarantees mot många buggar

## Project Structure

```
remote-smartcard/
├── Cargo.toml                 # Workspace root
├── README.md
├── ARCHITECTURE.md
├── SETUP.md
├── COMPONENTS.md
├── LICENSE
│
├── protocol/
│   ├── smartcard.proto        # Protocol definition
│   ├── build.rs               # Protobuf codegen
│   └── Cargo.toml
│
├── common/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── config.rs          # Shared config types
│   │   ├── error.rs           # Shared error types
│   │   ├── tls.rs             # TLS utilities
│   │   └── logging.rs         # Logging setup
│   └── Cargo.toml
│
├── client/
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs          # Client config
│   │   ├── pcsc_reader.rs     # PC/SC interface
│   │   ├── grpc_client.rs     # gRPC client
│   │   ├── session.rs         # Session management
│   │   └── error.rs           # Client errors
│   ├── Cargo.toml
│   └── tests/
│       └── integration_test.rs
│
├── server/
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs          # Server config
│   │   ├── grpc_server.rs     # gRPC service impl
│   │   ├── session_manager.rs # Session tracking
│   │   ├── vpcd_manager.rs    # vpcd process management
│   │   ├── vpcd_protocol.rs   # vpcd socket protocol
│   │   ├── auth.rs            # Client authentication
│   │   └── error.rs           # Server errors
│   ├── Cargo.toml
│   └── tests/
│       └── integration_test.rs
│
├── tools/
│   ├── keygen/
│   │   ├── src/
│   │   │   └── main.rs        # Certificate generation tool
│   │   └── Cargo.toml
│   └── test-client/           # Manual testing tool
│       ├── src/
│       │   └── main.rs
│       └── Cargo.toml
│
├── systemd/
│   ├── rsc-client.service
│   └── rsc-server.service
│
├── config/
│   ├── client.example.yaml
│   └── server.example.yaml
│
├── scripts/
│   ├── setup-ca.sh            # CA creation script
│   ├── setup-dev-env.sh       # Development environment setup
│   └── build-packages.sh      # Build DEB/RPM packages
│
├── packaging/
│   ├── debian/
│   │   ├── control
│   │   ├── rules
│   │   ├── postinst
│   │   └── prerm
│   └── rpm/
│       └── remote-smartcard.spec
│
├── docs/
│   ├── protocol.md            # Protocol documentation
│   ├── security.md            # Security considerations
│   └── troubleshooting.md
│
└── tests/
    ├── integration/           # End-to-end tests
    ├── mock-pcsc/             # Mock PC/SC for testing
    └── test-data/             # Test certificates, etc.
```

## Utvecklingsfaser

### Fas 1: Foundation & Proof of Concept (3-4 veckor)

#### Vecka 1: Project Setup & Protocol
**Mål**: Få grundstruktur på plats och protobuf kompilerande

**Tasks**:
- [x] Skapa project structure
- [ ] Setup Cargo workspace
- [ ] Implementera protocol/build.rs för protobuf codegen
- [ ] Skapa common crate med shared types
- [ ] Setup CI/CD (GitHub Actions)
  - Cargo build
  - Cargo test
  - Cargo clippy
  - Cargo fmt check

**Deliverable**: `cargo build` fungerar, protobuf genererar Rust kod

#### Vecka 2-3: Minimal Client & Server
**Mål**: Basic APDU forwarding utan TLS

**Client tasks**:
- [ ] Implementera PC/SC reader enumeration
- [ ] Läs ATR från lokalt kort
- [ ] Basic gRPC client (Connect RPC)
- [ ] Skicka ATR till server
- [ ] Basic logging

**Server tasks**:
- [ ] Implementera basic gRPC server
- [ ] Starta vpcd subprocess
- [ ] Kommunicera med vpcd via socket
- [ ] Forwarda ATR från klient till vpcd
- [ ] Basic logging

**Test**:
```bash
# Terminal 1: Starta server
cargo run --bin rsc-server

# Terminal 2: Starta client
cargo run --bin rsc-client

# Terminal 3: På server, verifiera reader
pcsc_scan
# Ska visa Virtual PCD med ditt korts ATR
```

**Deliverable**: Demo som visar ATR från remote card

#### Vecka 3-4: APDU Transmission
**Mål**: Full APDU forwarding

**Tasks**:
- [ ] Implementera Transmit RPC
- [ ] Client: läs APDU från local card
- [ ] Server: forwarda APDU till vpcd
- [ ] vpcd: skicka APDU till virtuell card
- [ ] Response path: vpcd → server → client → local pcscd
- [ ] Error handling för APDU errors

**Test**:
```bash
# På server
pkcs11-tool --module /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so --list-objects
# Ska fungera!
```

**Deliverable**: Fungerande APDU transmission, kan använda remote smartcard

---

### Fas 2: Security & Reliability (2-3 veckor)

#### Vecka 5: TLS Implementation
**Mål**: Säker krypterad transport

**Tasks**:
- [ ] Implementera TLS config loading (common crate)
- [ ] Client: TLS client med client certificate
- [ ] Server: TLS server med client cert verification
- [ ] Certificate validation
- [ ] Error handling för TLS failures
- [ ] Update config files med TLS settings

**Test**:
```bash
# Generera test certificates
./scripts/setup-ca.sh

# Testa med TLS enabled
cargo run --bin rsc-server -- --config config/server-tls.yaml
cargo run --bin rsc-client -- --config config/client-tls.yaml
```

**Deliverable**: mTLS fungerar, ingen plaintext över nätverket

#### Vecka 6: Reconnection & Heartbeat
**Mål**: Robust mot network failures

**Tasks**:
- [ ] Client: reconnect logic med exponential backoff
- [ ] Client: detect connection loss
- [ ] Server: detect dead clients
- [ ] Implement Heartbeat RPC
- [ ] Client: periodic heartbeat
- [ ] Server: timeout inactive sessions
- [ ] State recovery efter reconnect

**Test**:
```bash
# Starta client och server
# Under operation: stäng av network interface
sudo ifconfig eth0 down
# Vänta 10 sekunder
sudo ifconfig eth0 up
# Client ska reconnecta automatiskt
```

**Deliverable**: System återhämtar sig från network issues

#### Vecka 7: Error Handling & Logging
**Mål**: Production-grade error handling

**Tasks**:
- [ ] Comprehensive error types
- [ ] Error conversion från alla dependencies
- [ ] Structured logging (tracing)
- [ ] Log levels (ERROR, WARN, INFO, DEBUG, TRACE)
- [ ] Context i error messages
- [ ] Metrics (optional: prometheus integration)

**Deliverable**: Tydliga felmeddelanden, bra debuggability

---

### Fas 3: Production Ready (2-3 veckor)

#### Vecka 8: Configuration & CLI
**Mål**: User-friendly configuration

**Tasks**:
- [ ] Config file validation
- [ ] CLI arguments (clap)
- [ ] Environment variable overrides
- [ ] Default values som gör sense
- [ ] Config exempel-filer
- [ ] --help documentation

**Deliverable**: Lätt att konfigurera och använda

#### Vecka 9: systemd Integration
**Mål**: Körs som system service

**Tasks**:
- [ ] systemd service files
- [ ] systemd notification (sd_notify)
- [ ] Proper signal handling (SIGTERM)
- [ ] Graceful shutdown
- [ ] Log till journald
- [ ] Service dependencies (pcscd)

**Test**:
```bash
sudo systemctl start rsc-server
sudo systemctl status rsc-server
sudo journalctl -u rsc-server -f
```

**Deliverable**: Körs stabilt som systemd service

#### Vecka 10: Packaging & Documentation
**Mål**: Installerbart system

**Tasks**:
- [ ] Debian package
- [ ] RPM package
- [ ] Installation scripts
- [ ] Man pages
- [ ] Update README.md
- [ ] TROUBLESHOOTING.md
- [ ] SECURITY.md

**Deliverable**: Installerbara packages

---

### Fas 4: Testing & Release (1-2 veckor)

#### Vecka 11-12: Testing
**Mål**: Vältestad release

**Tasks**:
- [ ] Unit tests för alla moduler
- [ ] Integration tests
- [ ] End-to-end tests
- [ ] Test med olika smartcards (Yubikey, JavaCard, etc.)
- [ ] Test med olika applikationer (SSH, GPG, Firefox)
- [ ] Performance testing
- [ ] Load testing (multipla klienter)
- [ ] Security audit
- [ ] Penetration testing

**Deliverable**: Version 1.0 release!

---

## Development Environment Setup

### Prerequisites
```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Protobuf compiler
sudo apt-get install protobuf-compiler

# PC/SC development
sudo apt-get install libpcsclite-dev

# vpcd
# (bygg från source, se SETUP.md)

# Development tools
cargo install cargo-watch    # Auto-rebuild on change
cargo install cargo-expand   # Expand macros
cargo install cargo-audit    # Security audit
```

### Development Workflow

#### Running in development
```bash
# Terminal 1: Run server with auto-reload
cd server
cargo watch -x 'run -- --config ../config/server.dev.yaml'

# Terminal 2: Run client
cd client
cargo watch -x 'run -- --config ../config/client.dev.yaml'

# Terminal 3: Run tests
cargo watch -x test
```

#### Code quality
```bash
# Format code
cargo fmt

# Lint
cargo clippy -- -D warnings

# Security audit
cargo audit

# Check for outdated dependencies
cargo outdated
```

#### Testing
```bash
# All tests
cargo test

# Specific test
cargo test test_pcsc_reader

# With logging
RUST_LOG=debug cargo test

# Integration tests only
cargo test --test integration_test
```

#### Debugging
```bash
# With GDB
rust-gdb target/debug/rsc-server

# With LLDB
rust-lldb target/debug/rsc-server

# Print all logs
RUST_LOG=trace cargo run --bin rsc-client
```

## Git Workflow

### Branches
- `main` - Stable releases
- `develop` - Development branch
- `feature/*` - Feature branches
- `bugfix/*` - Bug fixes
- `release/*` - Release preparation

### Commit Messages
```
type(scope): Subject

Body

Fixes #123
```

**Types**: feat, fix, docs, style, refactor, test, chore

**Examples**:
```
feat(client): Add automatic reconnection logic

Implements exponential backoff for reconnections.
Handles network failures gracefully.

Fixes #42
```

```
fix(server): Fix vpcd socket race condition

vpcd socket was sometimes not ready when we tried to connect.
Added retry logic with timeout.

Fixes #58
```

## Code Style

### Rust Conventions
- Follow Rust API guidelines
- Use `rustfmt` for formatting
- Use `clippy` for linting
- Prefer `?` over `unwrap()`
- Document public APIs with `///`
- Use `anyhow::Result` for applications
- Use `thiserror` for library errors

### Error Handling Pattern
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("PC/SC error: {0}")]
    Pcsc(#[from] pcsc::Error),

    #[error("Connection error: {0}")]
    Connection(String),
}

pub type Result<T> = std::result::Result<T, ClientError>;
```

### Logging Pattern
```rust
use tracing::{info, warn, error, debug, trace};

#[instrument]
async fn connect_to_server(config: &Config) -> Result<Session> {
    info!("Connecting to server: {}", config.server.host);

    match establish_connection(config).await {
        Ok(conn) => {
            info!("Connected successfully");
            Ok(conn)
        }
        Err(e) => {
            error!("Connection failed: {}", e);
            Err(e)
        }
    }
}
```

## Performance Targets

### Latency
- ATR read: <50ms overhead vs local
- APDU transmission: <30ms overhead vs local
- Reconnect time: <2 seconds

### Throughput
- Support 100+ APDU/second per connection
- Support 100+ concurrent clients

### Resource Usage
- Client: <50MB RAM
- Server: <100MB RAM + 20MB per client
- CPU: <5% idle, <50% under load

## Security Considerations

### Threat Model
- **In scope**: Network attacks, unauthorized access
- **Out of scope**: Physical attacks on smartcard, compromised endpoints

### Security Requirements
- TLS 1.3 minimum
- Client certificate authentication
- No credentials in transit (PIN stays on client)
- Audit logging of all connections
- Rate limiting

### Security Testing
- [ ] Run cargo-audit regularly
- [ ] Static analysis (cargo-clippy)
- [ ] Fuzzing (cargo-fuzz) for protocol parsing
- [ ] Penetration testing before 1.0

## Release Process

### Version Numbering
Semantic Versioning: MAJOR.MINOR.PATCH

- MAJOR: Breaking changes
- MINOR: New features (backwards compatible)
- PATCH: Bug fixes

### Release Checklist
- [ ] All tests pass
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
- [ ] Version bumped in Cargo.toml
- [ ] Git tag created
- [ ] Packages built (DEB, RPM)
- [ ] GitHub release created
- [ ] Announcements posted

## Support & Community

### Communication Channels
- **GitHub Issues**: Bug reports, feature requests
- **GitHub Discussions**: Questions, ideas
- **Wiki**: Community documentation

### Contributing
See CONTRIBUTING.md for guidelines.

## Future Roadmap (Post-1.0)

### Version 1.1
- Windows client support
- macOS client support
- Multiple smartcard readers support
- Reader filtering improvements

### Version 2.0
- Web UI for monitoring
- REST API
- Auto-discovery (mDNS/Avahi)
- Load balancing
- Clustering

### Version 3.0
- HSM support
- Hardware token support (U2F/FIDO2)
- Mobile client (Android/iOS)

## Resources

### Learning Resources
- Rust Book: https://doc.rust-lang.org/book/
- Tokio Tutorial: https://tokio.rs/tokio/tutorial
- gRPC/Tonic: https://github.com/hyperium/tonic
- PC/SC API: https://pcsclite.apdu.fr/api/

### Related Projects
- vsmartcard: https://github.com/frankmorgner/vsmartcard
- OpenSC: https://github.com/OpenSC/OpenSC
- pcsc-lite: https://salsa.debian.org/rousseau/PCSC

### Standards
- ISO 7816 (Smartcard standard)
- PC/SC Workgroup specifications
- CCID specification

## Contact

Project Lead: [Your Name] <your.email@example.com>

## License

TBD - Proposed: MIT or Apache 2.0
