# Remote Smartcard - Arkitektur

## Översikt

Ett system för att använda lokala smartkort/Yubikeys på fjärrsystem via nätverket, med full transparens för applikationer som om kortet satt fysiskt anslutet.

## Design-principer

1. **Återanvänd standard OS-komponenter** - Minimal custom kod
2. **Säkerhet först** - Krypterad transport, ingen plaintext credentials
3. **Transparent** - Fjärrapplikationer ser vanlig PC/SC reader
4. **Stabilt** - Hantera network drops, reconnects, timeout
5. **Enkelt** - Minimal konfiguration, fungerar out-of-the-box

## Arkitektur-översikt

```
┌─────────────────────────────────┐         ┌─────────────────────────────────┐
│      LOKAL DATOR (Klient)       │         │    FJÄRR SERVER (Server)        │
├─────────────────────────────────┤         ├─────────────────────────────────┤
│                                 │         │                                 │
│  ┌───────────────────────┐     │         │     ┌──────────────────┐       │
│  │   Fysisk Yubikey/     │     │         │     │   Application    │       │
│  │   Smartcard Reader    │     │         │     │  (Firefox, SSH,  │       │
│  └──────────┬────────────┘     │         │     │   gpg, osv...)   │       │
│             │                   │         │     └────────┬─────────┘       │
│             │ USB               │         │              │ PC/SC API       │
│             ▼                   │         │              ▼                 │
│  ┌─────────────────────────┐   │         │   ┌─────────────────────────┐  │
│  │   pcscd (system)        │   │         │   │   pcscd (system)        │  │
│  │   + CCID driver         │   │         │   │                         │  │
│  └──────────┬──────────────┘   │         │   └──────────┬──────────────┘  │
│             │ libpcsclite       │         │              │                 │
│             ▼                   │         │              │                 │
│  ┌─────────────────────────┐   │         │   ┌──────────▼──────────────┐  │
│  │  rsc-client             │   │  Secure │   │  vpcd (Virtual Reader)  │  │
│  │  (vårt program)         │◄──┼─────────┼──►│  + vreader driver       │  │
│  │  - Läser via PC/SC      │   │  TLS    │   └──────────┬──────────────┘  │
│  │  - Skickar APDU via net │   │  mTLS   │              │ Socket          │
│  └─────────────────────────┘   │  gRPC   │              ▼                 │
│                                 │         │   ┌─────────────────────────┐  │
│                                 │         │   │  rsc-server             │  │
│                                 │         │   │  (vårt program)         │  │
│                                 │         │   │  - Tar emot APDU        │  │
│                                 │         │   │  - Pratar med vpcd      │  │
│                                 │         │   └─────────────────────────┘  │
└─────────────────────────────────┘         └─────────────────────────────────┘
```

## OS-komponenter som återanvänds

### 1. pcscd (PC/SC Daemon)
- **Vad**: Standard PC/SC Smart Card Daemon i Linux
- **Paket**: `pcscd` / `pcsc-lite`
- **Roll**:
  - Klient: Kommunicerar med fysiskt smartkort
  - Server: Tillhandahåller PC/SC API till applikationer
- **Status**: ✅ Finns i alla distributioner, väl testad
- **Dokumentation**: https://pcsclite.apdu.fr/

### 2. libpcsclite
- **Vad**: Client-bibliotek för PC/SC
- **Paket**: `libpcsclite-dev`
- **Roll**: API för att prata med pcscd
- **Status**: ✅ Standard library, används av vår klient-daemon
- **API**: `SCardConnect()`, `SCardTransmit()`, etc.

### 3. vpcd (Virtual PC/SC Device)
- **Vad**: Virtual smartcard reader som kan fjärrstyras via socket
- **Del av**: vsmartcard project (https://frankmorgner.github.io/vsmartcard/)
- **Paket**: `vpcsc` eller bygg från source
- **Roll**: Emulerar en CCID reader på servern som vårt program kan mata med data
- **Status**: ✅ Open source, väl testat, perfekt för vårt use case
- **Protokoll**: Simpelt socket-baserat protokoll

### 4. CCID Driver
- **Vad**: Standard driver för USB CCID smartcard readers
- **Paket**: `libccid`
- **Roll**: Driver för fysiska readers (klient-sidan)
- **Status**: ✅ Stödjer de flesta Yubikeys och smartcards

### 5. systemd
- **Vad**: Service manager
- **Roll**: Hantera rsc-client och rsc-server som systemd services
- **Status**: ✅ Standard i moderna Linux
- **Features**: Auto-restart, logging, dependency management

### 6. OpenSSL / GnuTLS
- **Vad**: TLS implementation
- **Roll**: Krypterad transport mellan klient och server
- **Status**: ✅ Standard, vältestad
- **Alt**: Använd gRPC som har inbyggd TLS

## Komponenter som måste utvecklas

### 1. rsc-client (Remote Smartcard Client)
**Språk**: C eller Rust (förslag: Rust för säkerhet och stabilitet)

**Ansvar**:
- Upptäck smartcard readers via pcscd
- Läs smartcard information (ATR, readers)
- Lyssna på card insert/remove events
- Proxya PC/SC kommandon till servern
- Hantera reconnects och network failures
- Auto-discovery av servers (mDNS/Avahi optional)

**Dependencies**:
- libpcsclite
- OpenSSL eller gRPC
- systemd integration

**Configuration** (`/etc/rsc-client/config.yaml`):
```yaml
server:
  host: "remote.example.com"
  port: 8443

tls:
  client_cert: "/etc/rsc-client/client.crt"
  client_key: "/etc/rsc-client/client.key"
  ca_cert: "/etc/rsc-client/ca.crt"
  verify_server: true

client:
  reader_filter: "Yubico"  # Endast forwarda vissa readers (optional)
  reconnect_interval: 5s
  heartbeat_interval: 30s

logging:
  level: info
  file: "/var/log/rsc-client.log"
```

### 2. rsc-server (Remote Smartcard Server)
**Språk**: C eller Rust

**Ansvar**:
- Lyssna på TLS port för klient-connections
- Authentisera klienter (mTLS)
- För varje klient: starta vpcd instance
- Proxya APDU mellan klient och vpcd
- Hantera multipla samtidiga klienter
- Session management

**Dependencies**:
- vpcd (spawnas som subprocess)
- OpenSSL eller gRPC
- systemd integration

**Configuration** (`/etc/rsc-server/config.yaml`):
```yaml
server:
  bind_address: "0.0.0.0"
  port: 8443
  max_clients: 100

tls:
  server_cert: "/etc/rsc-server/server.crt"
  server_key: "/etc/rsc-server/server.key"
  ca_cert: "/etc/rsc-server/ca.crt"
  require_client_cert: true

vpcd:
  binary: "/usr/local/bin/vpcd"
  socket_dir: "/var/run/rsc-server/vpcd"

logging:
  level: info
  file: "/var/log/rsc-server.log"
```

### 3. Protokoll (gRPC eller Custom)

#### Alternativ A: gRPC (Rekommenderat)
**Fördelar**:
- Inbyggd TLS/mTLS
- Effektiv binär serialisering (protobuf)
- Streaming för events
- Code generation för flera språk
- Väl dokumenterat

**Protocol Buffer Definition** (`protocol/smartcard.proto`):
```protobuf
syntax = "proto3";

package rsc;

service RemoteSmartcard {
  // Establish session and list readers
  rpc Connect(ConnectRequest) returns (ConnectResponse);

  // Stream of card events (insert/remove)
  rpc StreamCardEvents(Empty) returns (stream CardEvent);

  // Transmit APDU to card
  rpc Transmit(TransmitRequest) returns (TransmitResponse);

  // Heartbeat
  rpc Heartbeat(Empty) returns (Empty);
}

message ConnectRequest {
  string client_id = 1;
  string client_version = 2;
}

message ConnectResponse {
  string session_id = 1;
  repeated Reader readers = 2;
}

message Reader {
  string name = 1;
  string atr = 2;  // Answer To Reset (hex encoded)
  bool card_present = 3;
}

message CardEvent {
  enum EventType {
    CARD_INSERTED = 0;
    CARD_REMOVED = 1;
    READER_ADDED = 2;
    READER_REMOVED = 3;
  }

  EventType type = 1;
  string reader_name = 2;
  string atr = 3;
}

message TransmitRequest {
  string reader_name = 1;
  bytes apdu = 2;
}

message TransmitResponse {
  bytes response = 1;
  uint32 sw1 = 2;  // Status word 1
  uint32 sw2 = 3;  // Status word 2
}

message Empty {}
```

#### Alternativ B: Custom Protokoll över TLS
- Mer lightweight
- Mer kontroll
- Mer jobb att implementera
- Mindre robust

**Rekommendation**: Använd gRPC - det ger oss mycket gratis.

## Säkerhet

### Transport Security
1. **TLS 1.3** minimum
2. **Mutual TLS (mTLS)** - både klient och server autentiserar med certifikat
3. **Cipher suites**: Endast moderna säkra ciphers
4. **Certificate pinning**: Optional för extra säkerhet

### Certificate Management
**Option 1: Self-signed CA (Rekommenderat för privat bruk)**
```bash
# Skapa egen CA
openssl genrsa -out ca.key 4096
openssl req -x509 -new -nodes -key ca.key -sha256 -days 3650 -out ca.crt

# Server cert
openssl genrsa -out server.key 4096
openssl req -new -key server.key -out server.csr
openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -out server.crt -days 3650 -sha256

# Client cert
openssl genrsa -out client.key 4096
openssl req -new -key client.key -out client.csr
openssl x509 -req -in client.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -out client.crt -days 3650 -sha256
```

**Option 2: Let's Encrypt** (För publika servers)
**Option 3: Företags-PKI** (Om sådan finns)

### Access Control
- Endast tillåtna klient-certifikat kan connecta
- Per-klient ACL möjligt i framtiden
- Rate limiting för att förhindra DoS

## Felhantering & Stabilitet

### Network Resilience
1. **Automatic reconnect** - Klient försöker återansluta vid disconnect
2. **Exponential backoff** - 1s, 2s, 4s, 8s, max 60s
3. **Heartbeat** - Keepalive every 30s för att upptäcka dead connections
4. **Timeout handling** - Rimliga timeouts för alla operationer
5. **Buffering** - Minimal buffring, fail fast vid network issues

### State Management
- Servern håller session state per klient
- Vid reconnect: försök återskapa session
- Applikationer måste hantera card removed → inserted events

### Logging & Monitoring
- Strukturerad logging (JSON)
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE
- Metrics: connections, APDU count, errors, latency
- Health check endpoint

## Deployment & Setup

### Installation (Debian/Ubuntu)

**Server**:
```bash
# 1. Installera dependencies
apt-get install pcscd vpcsc libpcsclite1 openssl

# 2. Installera rsc-server
dpkg -i rsc-server_1.0.0_amd64.deb

# 3. Generera certifikat (eller kopiera befintliga)
/usr/local/bin/rsc-keygen --type server \
  --output /etc/rsc-server/certs

# 4. Konfigurera
vim /etc/rsc-server/config.yaml

# 5. Starta service
systemctl enable rsc-server
systemctl start rsc-server
```

**Klient**:
```bash
# 1. Installera dependencies
apt-get install pcscd libpcsclite1 openssl

# 2. Installera rsc-client
dpkg -i rsc-client_1.0.0_amd64.deb

# 3. Kopiera klient-certifikat
cp client.crt client.key ca.crt /etc/rsc-client/certs/

# 4. Konfigurera server address
vim /etc/rsc-client/config.yaml

# 5. Starta service
systemctl enable rsc-client
systemctl start rsc-client
```

### Testing
```bash
# På servern - verifiera att virtual reader syns
pcsc_scan

# Ska visa "Virtual PCD" reader med card present när klient är connectad

# Testa med verklig app
ssh-keygen -D /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so -e
```

## Utvecklingsplan

### Fas 1: Proof of Concept (2-3 veckor)
- [ ] Sätt upp dev environment
- [ ] Grundläggande rsc-client (läs local card ATR)
- [ ] Grundläggande rsc-server (starta vpcd)
- [ ] gRPC protokoll implementation
- [ ] Simpel APDU forward (ingen TLS än)
- [ ] Test: läs remote card ATR via pcsc_scan

### Fas 2: Säkerhet & Stabilitet (2 veckor)
- [ ] mTLS implementation
- [ ] Certificate generation scripts
- [ ] Reconnect logic
- [ ] Error handling
- [ ] Heartbeat
- [ ] Logging
- [ ] Test: simulera network failures

### Fas 3: Production Ready (2 veckor)
- [ ] systemd integration
- [ ] Konfigurationsfiler
- [ ] Debian/RPM packages
- [ ] Dokumentation
- [ ] Setup scripts
- [ ] Test: real-world användning (SSH, Firefox, etc.)

### Fas 4: Extra Features (optional)
- [ ] Multi-reader support
- [ ] Auto-discovery (mDNS)
- [ ] Web UI för monitoring
- [ ] Metrics & monitoring
- [ ] macOS/Windows support

## Alternativa Implementationer

### Rust (Rekommenderat)
**Fördelar**:
- Memory safety
- Excellent error handling
- Great async/await for network
- tonic = excellent gRPC library
- cargo = easy dependency management

**Crates**:
```toml
[dependencies]
tonic = "0.11"
prost = "0.12"
tokio = { version = "1", features = ["full"] }
tokio-rustls = "0.25"
pcsc = "2.8"  # PC/SC bindings
serde = { version = "1.0", features = ["derive"] }
serde_yaml = "0.9"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1.0"
thiserror = "1.0"
```

### C
**Fördelar**:
- Minimal dependencies
- Direkt libpcsclite integration
- Liten binary size

**Nackdelar**:
- Mer kod för error handling
- Svårare att skriva säkert
- Mindre ecosystem för gRPC

### Go
**Fördelar**:
- Excellent gRPC support
- Good standard library
- Easy deployment (static binary)

**Nackdelar**:
- PC/SC bindings mindre mature
- Större binary size

**Rekommendation**: Rust - bäst balans mellan säkerhet, performance, och developer experience.

## References

- PC/SC Workgroup: https://www.pcscworkgroup.com/
- pcscd: https://pcsclite.apdu.fr/
- vsmartcard (vpcd): https://frankmorgner.github.io/vsmartcard/
- gRPC: https://grpc.io/
- ISO 7816 (Smartcard standard): https://en.wikipedia.org/wiki/ISO/IEC_7816
- CCID specification: https://www.usb.org/sites/default/files/DWG_Smart-Card_CCID_Rev110.pdf

## Licens

TBD - föreslår MIT eller Apache 2.0 för maximal kompatibilitet.
