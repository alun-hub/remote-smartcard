# Komponenter att utveckla

## Översikt

Detta dokument listar alla komponenter som behöver utvecklas för Remote Smartcard-systemet.

## 1. rsc-client (Remote Smartcard Client)

### Språk & Dependencies
- **Språk**: Rust
- **Crates**:
  ```toml
  [dependencies]
  tonic = "0.11"           # gRPC framework
  prost = "0.12"           # Protocol Buffers
  tokio = "1"              # Async runtime
  tokio-rustls = "0.25"    # TLS support
  pcsc = "2.8"             # PC/SC bindings
  serde = "1.0"            # Serialization
  serde_yaml = "0.9"       # Config files
  tracing = "0.1"          # Logging
  tracing-subscriber = "0.3"
  anyhow = "1.0"           # Error handling
  thiserror = "1.0"        # Custom errors
  clap = "4.0"             # CLI parsing
  tokio-stream = "0.1"     # Stream utilities
  ```

### Moduler att implementera

#### 1.1 `main.rs`
- CLI argument parsing
- Config loading
- Service initialization
- Signal handling (SIGTERM, SIGINT)
- systemd notification

#### 1.2 `config.rs`
- Config struct definitions
- YAML parsing
- Config validation
- Default values
- Environment variable overrides

```rust
pub struct ClientConfig {
    pub server: ServerConfig,
    pub tls: TlsConfig,
    pub client: ClientSettings,
    pub logging: LoggingConfig,
}

pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

pub struct TlsConfig {
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
    pub ca_cert: PathBuf,
    pub verify_server: bool,
}

pub struct ClientSettings {
    pub client_id: String,
    pub reader_filter: Option<Vec<String>>,
    pub reconnect_interval: Duration,
    pub heartbeat_interval: Duration,
    pub operation_timeout: Duration,
}
```

#### 1.3 `pcsc_reader.rs`
- PC/SC context management
- Reader enumeration
- Card detection
- ATR reading
- APDU transmission
- Event monitoring

```rust
pub struct PcscReader {
    context: pcsc::Context,
}

impl PcscReader {
    pub fn new() -> Result<Self>;
    pub fn list_readers(&self) -> Result<Vec<String>>;
    pub fn get_atr(&self, reader: &str) -> Result<Vec<u8>>;
    pub fn transmit(&self, reader: &str, apdu: &[u8]) -> Result<Vec<u8>>;
    pub fn monitor_events(&self) -> Result<EventStream>;
}
```

#### 1.4 `grpc_client.rs`
- gRPC client implementation
- Connection management
- Request/response handling
- Stream management
- Error mapping

```rust
pub struct GrpcClient {
    client: RemoteSmartcardClient<Channel>,
    session_id: Option<String>,
}

impl GrpcClient {
    pub async fn connect(config: &ClientConfig) -> Result<Self>;
    pub async fn list_readers(&mut self) -> Result<Vec<Reader>>;
    pub async fn transmit(&mut self, req: TransmitRequest) -> Result<TransmitResponse>;
    pub async fn stream_events(&mut self) -> Result<EventStream>;
}
```

#### 1.5 `session.rs`
- Session lifecycle management
- Automatic reconnection
- State synchronization
- Heartbeat handling

```rust
pub struct Session {
    pcsc: PcscReader,
    grpc: GrpcClient,
    state: SessionState,
}

impl Session {
    pub async fn run(&mut self) -> Result<()>;
    async fn handle_reconnect(&mut self) -> Result<()>;
    async fn send_heartbeat(&mut self) -> Result<()>;
}
```

#### 1.6 `tls.rs`
- TLS configuration
- Certificate loading
- mTLS setup

```rust
pub fn load_tls_config(config: &TlsConfig) -> Result<ClientTlsConfig>;
```

#### 1.7 `error.rs`
- Custom error types
- Error conversion from pcsc, tonic, etc.

```rust
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("PC/SC error: {0}")]
    Pcsc(#[from] pcsc::Error),

    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Configuration error: {0}")]
    Config(String),
}
```

### Estimat
- **Kod**: ~2000-3000 rader Rust
- **Tid**: 2-3 veckor för första version
- **Komplexitet**: Medium-hög (async, PC/SC, gRPC)

---

## 2. rsc-server (Remote Smartcard Server)

### Språk & Dependencies
- **Språk**: Rust
- **Crates**: (samma som client + process management)
  ```toml
  [dependencies]
  # ... samma som client ...
  tokio-process = "0.2"    # vpcd process management
  dashmap = "5.5"          # Concurrent hashmap för sessions
  tower = "0.4"            # gRPC middleware
  ```

### Moduler att implementera

#### 2.1 `main.rs`
- CLI argument parsing
- Server initialization
- Graceful shutdown
- systemd integration

#### 2.2 `config.rs`
- Server config structures
- Similar to client but with server-specific settings

```rust
pub struct ServerConfig {
    pub server: BindConfig,
    pub tls: TlsConfig,
    pub vpcd: VpcdConfig,
    pub logging: LoggingConfig,
}

pub struct VpcdConfig {
    pub binary: PathBuf,
    pub socket_dir: PathBuf,
    pub timeout: Duration,
}
```

#### 2.3 `grpc_server.rs`
- gRPC service implementation
- Request handlers
- Session validation
- Error handling

```rust
#[derive(Debug)]
pub struct RemoteSmartcardService {
    sessions: Arc<SessionManager>,
    vpcd_manager: Arc<VpcdManager>,
}

#[tonic::async_trait]
impl rsc::remote_smartcard_server::RemoteSmartcard for RemoteSmartcardService {
    async fn connect(&self, req: Request<ConnectRequest>) -> Result<Response<ConnectResponse>, Status>;
    async fn transmit(&self, req: Request<TransmitRequest>) -> Result<Response<TransmitResponse>, Status>;
    // ... implement all RPC methods
}
```

#### 2.4 `session_manager.rs`
- Track active client sessions
- Session creation/destruction
- Session validation
- Concurrent access handling

```rust
pub struct SessionManager {
    sessions: DashMap<String, ClientSession>,
}

pub struct ClientSession {
    session_id: String,
    client_id: String,
    vpcd: VpcdInstance,
    created_at: Instant,
    last_heartbeat: Instant,
}
```

#### 2.5 `vpcd_manager.rs`
- Spawn vpcd processes
- Manage vpcd lifecycle
- Socket communication with vpcd
- Process cleanup

```rust
pub struct VpcdManager {
    config: VpcdConfig,
    instances: DashMap<String, VpcdInstance>,
}

pub struct VpcdInstance {
    process: Child,
    socket_path: PathBuf,
    socket: UnixStream,
}

impl VpcdManager {
    pub async fn spawn_vpcd(&self, session_id: &str) -> Result<VpcdInstance>;
    pub async fn send_apdu(&self, session_id: &str, apdu: &[u8]) -> Result<Vec<u8>>;
    pub async fn shutdown_vpcd(&self, session_id: &str) -> Result<()>;
}
```

#### 2.6 `vpcd_protocol.rs`
- vpcd socket protocol implementation
- Message encoding/decoding
- ATR exchange
- APDU exchange

```rust
pub struct VpcdProtocol {
    socket: UnixStream,
}

impl VpcdProtocol {
    pub async fn send_atr(&mut self, atr: &[u8]) -> Result<()>;
    pub async fn receive_apdu(&mut self) -> Result<Vec<u8>>;
    pub async fn send_response(&mut self, response: &[u8]) -> Result<()>;
}
```

#### 2.7 `tls.rs`
- Server TLS configuration
- Certificate loading
- Client certificate validation

#### 2.8 `auth.rs`
- Client authentication logic
- Certificate-based auth
- Optional: ACL checking

```rust
pub fn validate_client_cert(cert: &Certificate) -> Result<ClientIdentity>;
```

### Estimat
- **Kod**: ~3000-4000 rader Rust
- **Tid**: 3-4 veckor för första version
- **Komplexitet**: Hög (gRPC server, process management, concurrent sessions)

---

## 3. rsc-keygen (Certificate Generation Tool)

### Språk & Dependencies
- **Språk**: Rust eller Bash
- **Crates** (om Rust):
  ```toml
  [dependencies]
  clap = "4.0"
  rcgen = "0.11"      # Certificate generation
  ```

### Funktionalitet
- Generera CA
- Generera server certificates
- Generera client certificates
- Interactive prompts
- Batch mode för automation

### Script alternativ (enklare)
```bash
#!/bin/bash
# rsc-keygen - Certificate generation script

case "$1" in
  --type)
    case "$2" in
      ca)
        generate_ca
        ;;
      server)
        generate_server_cert
        ;;
      client)
        generate_client_cert
        ;;
    esac
    ;;
esac
```

### Estimat
- **Kod**: 500-800 rader (Rust) eller 200-300 rader (Bash)
- **Tid**: 3-5 dagar
- **Komplexitet**: Låg-medium

---

## 4. systemd Service Files

### 4.1 `rsc-client.service`
```ini
[Unit]
Description=Remote Smartcard Client
After=network-online.target pcscd.service
Wants=network-online.target
Requires=pcscd.service

[Service]
Type=notify
ExecStart=/usr/local/bin/rsc-client --config /etc/rsc-client/config.yaml
Restart=always
RestartSec=5
User=root
Group=root

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/log/rsc-client

[Install]
WantedBy=multi-user.target
```

### 4.2 `rsc-server.service`
```ini
[Unit]
Description=Remote Smartcard Server
After=network.target pcscd.service
Requires=pcscd.service

[Service]
Type=notify
ExecStart=/usr/local/bin/rsc-server --config /etc/rsc-server/config.yaml
Restart=always
RestartSec=5
User=root
Group=root

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/log/rsc-server /var/run/rsc-server

[Install]
WantedBy=multi-user.target
```

### Estimat
- **Tid**: 1 dag
- **Komplexitet**: Låg

---

## 5. Packaging

### 5.1 Debian Package
- `debian/control`
- `debian/rules`
- `debian/postinst` - create directories, enable services
- `debian/prerm` - stop services
- `debian/postrm` - cleanup

### 5.2 RPM Package
- `.spec` file
- Similar to Debian

### Scripts att inkludera
- `/usr/share/rsc/examples/config.yaml`
- `/usr/share/doc/rsc/README.md`
- `/usr/share/doc/rsc/ARCHITECTURE.md`

### Estimat
- **Tid**: 1 vecka för båda package-typer
- **Komplexitet**: Medium

---

## 6. Testing

### 6.1 Unit Tests
- Test per modul
- Mock PC/SC
- Mock gRPC
- Test error handling

### 6.2 Integration Tests
- End-to-end tests
- Test med riktigt smartcard (i CI svårt, lokalt)
- Test reconnect scenarios
- Test concurrent clients

### 6.3 Test Tools
```rust
// tests/common/mod.rs
pub fn setup_mock_pcsc() -> MockPcsc;
pub fn setup_test_server() -> TestServer;
```

### Estimat
- **Tid**: Löpande under utveckling + 1 vecka dedicated testing
- **Komplexitet**: Medium-hög

---

## 7. Documentation

### 7.1 User Documentation
- ✅ README.md (klar)
- ✅ ARCHITECTURE.md (klar)
- [ ] SETUP.md - Detaljerad setup guide
- [ ] TROUBLESHOOTING.md - Vanliga problem
- [ ] SECURITY.md - Security best practices
- [ ] FAQ.md

### 7.2 Developer Documentation
- [ ] CONTRIBUTING.md
- [ ] DEVELOPMENT.md - Setup dev environment
- [ ] API.md - gRPC API dokumentation
- [ ] Rustdoc comments i kod

### 7.3 Man Pages
- [ ] rsc-client(1)
- [ ] rsc-server(1)
- [ ] rsc-keygen(1)
- [ ] rsc-client.yaml(5)
- [ ] rsc-server.yaml(5)

### Estimat
- **Tid**: 1 vecka
- **Komplexitet**: Låg

---

## Sammanfattning

### Total Effort Estimation

| Komponent | LOC (Lines of Code) | Tid | Komplexitet |
|-----------|---------------------|-----|-------------|
| rsc-client | 2000-3000 | 2-3v | Medium-Hög |
| rsc-server | 3000-4000 | 3-4v | Hög |
| rsc-keygen | 500-800 | 3-5d | Låg-Medium |
| systemd services | 100 | 1d | Låg |
| Packaging | 500 | 1v | Medium |
| Testing | 1000-2000 | 2v | Medium-Hög |
| Documentation | - | 1v | Låg |
| **TOTAL** | **~7000-10000** | **10-14 veckor** | - |

### Development Phases

#### Phase 1: Proof of Concept (3-4 veckor)
- Minimal rsc-client (bara läs ATR, skicka en APDU)
- Minimal rsc-server (ta emot APDU, forwarda till vpcd)
- Basic gRPC utan TLS
- Manual testing

**Deliverable**: Demo som visar att det fungerar i princip

#### Phase 2: Security & Reliability (2-3 veckor)
- Implementera mTLS
- Reconnect logic
- Heartbeat
- Error handling
- Logging

**Deliverable**: Säkert, stabilt system

#### Phase 3: Production Polish (2-3 veckor)
- systemd integration
- Config files
- Certificate generation
- Packaging
- Documentation

**Deliverable**: Installbart, användbart system

#### Phase 4: Testing & Hardening (1-2 veckor)
- Unit tests
- Integration tests
- Real-world testing
- Bug fixes

**Deliverable**: Release-kvalitet

### Risk Factors

1. **vpcd integration**: Kan vara tricky, protocol kanske inte dokumenterat perfekt
   - Mitigation: Studera vsmartcard source code tidigt

2. **PC/SC på olika plattformar**: PC/SC API kan variera Linux/Windows/macOS
   - Mitigation: Fokusera på Linux först

3. **Network edge cases**: Många corner cases med reconnect, timeout, etc
   - Mitigation: Extensive testing och robusta state machines

4. **Performance**: Latency kan vara problem för vissa use cases
   - Mitigation: Optimera protokoll, använd binary (protobuf), mät tidigt

### Next Steps

1. ✅ Arkitektur och design - KLART
2. ⏭️ Setup development environment
3. ⏭️ Implementera minimal POC
4. ⏭️ Test med riktigt smartcard
5. ⏭️ Iterera vidare enligt fasplan
