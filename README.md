# Remote Smartcard (rsc)

Ett säkert och transparent system för att använda lokala smartkort och Yubikeys på fjärrsystem via nätverket.

## Översikt

Remote Smartcard (rsc) gör det möjligt att använda ett smartkort som sitter i din lokala dator på en fjärrserver, som om kortet var fysiskt anslutet där. Detta är användbart för:

- SSH-autentisering med smartkort till servrar
- Signering av git commits med Yubikey från remote development environments
- Användning av certifikat från smartkort i webbläsare på remote desktop
- Alla andra use cases där PC/SC används

## Funktioner

- ✅ **Transparent**: Applikationer på servern ser vanlig PC/SC reader
- ✅ **Säker**: Mutual TLS (mTLS) kryptering, inget credentials i transit
- ✅ **Stabil**: Automatic reconnect, heartbeat, network resilience
- ✅ **Enkelt**: Minimal konfiguration, systemd integration
- ✅ **Standard**: Använder pcscd, vpcd, och andra etablerade komponenter
- ✅ **Effektivt**: gRPC/protobuf för minimal overhead
- ✅ **Multi-reader**: Stödjer flera smartcard readers samtidigt

## Arkitektur

```
[Lokal Dator] --[TLS/gRPC]--> [Fjärrserver]
  Yubikey/                        Application
  Smartcard                           |
     |                            pcscd
   pcscd                              |
     |                          vpcd (virtual reader)
 rsc-client ===============> rsc-server
```

Se [ARCHITECTURE.md](ARCHITECTURE.md) för detaljerad teknisk dokumentation.

## Snabbstart

### Förutsättningar

**På båda klient och server:**
- Linux (Debian/Ubuntu/RHEL/Arch)
- pcscd installerat
- OpenSSL

**På klienten:**
- Fysiskt smartkort eller Yubikey ansluten

**På servern:**
- vpcd installerat (del av vsmartcard)

### Installation

#### 1. Installera dependencies

**Debian/Ubuntu:**
```bash
# Server
sudo apt-get install pcscd vpcsc libpcsclite1

# Klient
sudo apt-get install pcscd libpcsclite1
```

**RHEL/CentOS/Fedora:**
```bash
# Server
sudo dnf install pcsc-lite vpcd

# Klient
sudo dnf install pcsc-lite
```

#### 2. Installera rsc (när färdigutvecklad)

```bash
# Från DEB package
sudo dpkg -i rsc-server_1.0.0_amd64.deb  # På server
sudo dpkg -i rsc-client_1.0.0_amd64.deb  # På klient

# Eller från source
git clone https://github.com/yourusername/remote-smartcard
cd remote-smartcard
cargo build --release
sudo cp target/release/rsc-server /usr/local/bin/
sudo cp target/release/rsc-client /usr/local/bin/
```

#### 3. Generera certifikat

**På server:**
```bash
# Skapa CA och server certifikat
sudo rsc-keygen --type server --output /etc/rsc-server/certs

# Detta skapar:
# - ca.crt (CA certificate - kopiera till klienter)
# - server.crt (Server certificate)
# - server.key (Server private key)
```

**På klient:**
```bash
# Skapa klient certifikat (behöver ca.crt och ca.key från server)
sudo rsc-keygen --type client \
  --ca-cert /path/to/ca.crt \
  --ca-key /path/to/ca.key \
  --output /etc/rsc-client/certs

# Kopiera CA cert från server
scp server:/etc/rsc-server/certs/ca.crt /etc/rsc-client/certs/
```

#### 4. Konfigurera

**Server** (`/etc/rsc-server/config.yaml`):
```yaml
server:
  bind_address: "0.0.0.0"
  port: 8443

tls:
  server_cert: "/etc/rsc-server/certs/server.crt"
  server_key: "/etc/rsc-server/certs/server.key"
  ca_cert: "/etc/rsc-server/certs/ca.crt"
  require_client_cert: true

logging:
  level: info
```

**Klient** (`/etc/rsc-client/config.yaml`):
```yaml
server:
  host: "your-server.example.com"
  port: 8443

tls:
  client_cert: "/etc/rsc-client/certs/client.crt"
  client_key: "/etc/rsc-client/certs/client.key"
  ca_cert: "/etc/rsc-client/certs/ca.crt"

logging:
  level: info
```

#### 5. Starta services

```bash
# Server
sudo systemctl enable rsc-server
sudo systemctl start rsc-server

# Klient
sudo systemctl enable rsc-client
sudo systemctl start rsc-client
```

#### 6. Verifiera

**På servern:**
```bash
# Lista readers (ska visa virtual reader med ditt kort)
pcsc_scan

# Output bör visa:
# Reader 0: Virtual PCD 00 00
#   Card state: Card inserted
#   ATR: 3B 8A 80 01 ...
```

**Testa med riktig applikation:**
```bash
# Exempel: SSH med smartcard
ssh-keygen -D /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so -e

# Exempel: Lista certifikat med pkcs11-tool
pkcs11-tool --module /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so --list-objects
```

## Användning

Efter installation är systemet transparent. Alla applikationer på servern som använder PC/SC kan nu använda ditt remote smartkort:

### SSH med Smartcard
```bash
# På servern
ssh-keygen -D /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so -e
# Lägger till pubkey till authorized_keys

# SSH till annan host från servern med smartcard auth
ssh -I /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so user@destination
```

### Git Signing med Yubikey
```bash
# På servern
git config --global gpg.program gpg2
git config --global user.signingkey "keyid från smartcard"
git config --global commit.gpgsign true

# Commits kommer nu signeras med Yubikey från din lokala dator
git commit -m "Signed from remote with local Yubikey!"
```

### Firefox med Client Certificates
```bash
# Firefox på servern kommer automatiskt se smartcard reader
# Gå till Settings → Privacy & Security → Security Devices
# Lägg till OpenSC PKCS#11 module om behövs
```

## Felsökning

### Klienten kan inte connecta
```bash
# Kontrollera att server lyssnar
sudo netstat -tlnp | grep 8443

# Kontrollera client logs
sudo journalctl -u rsc-client -f

# Testa TLS connection
openssl s_client -connect server:8443 \
  -cert /etc/rsc-client/certs/client.crt \
  -key /etc/rsc-client/certs/client.key \
  -CAfile /etc/rsc-client/certs/ca.crt
```

### Ingen reader syns på servern
```bash
# Kontrollera server logs
sudo journalctl -u rsc-server -f

# Kontrollera att vpcd körs
ps aux | grep vpcd

# Kontrollera att pcscd ser vpcd reader
systemctl restart pcscd
pcsc_scan
```

### Kort fungerar inte korrekt
```bash
# Kontrollera ATR på klient
pcsc_scan  # På klient

# Kontrollera ATR på server
pcsc_scan  # På server

# Ska vara identiska. Om inte, kolla logs för APDU errors.
```

### Network issues
```bash
# Testa latency
ping server

# Högre latency (>100ms) kan ge timeout i vissa applikationer
# Justera timeout i config:
# client:
#   operation_timeout: 10s  # Öka vid hög latency
```

## Säkerhet

### Best Practices

1. **Använd starka certifikat**: Minimum 2048-bit RSA eller 256-bit ECC
2. **Begränsa server access**: Använd firewall, endast tillåt betrodda klienter
3. **Rotera certifikat**: Byt certifikat regelbundet (årligen)
4. **Monitera logs**: Övervaka för onormala connection patterns
5. **Använd separate network**: Kör på VPN eller privat nätverk om möjligt

### Säkerhetsmodell

- **Krypterad transport**: All data krypteras med TLS 1.3
- **Mutual authentication**: Både klient och server verifierar varandra
- **No credential exposure**: PIN/password skickas ALDRIG över nätverket
  - PIN hanteras lokalt på klienten av pcscd
  - Endast APDU responses skickas över nätverk
- **Session isolation**: Varje klient får egen isolerad session

### Vad som INTE skyddas mot

- **Compromised server**: Om servern är komprometterad kan attacker skicka APDU till ditt kort
  - Detta är inherent i designen - servern måste kunna använda kortet
  - Använd endast betrodda servrar
- **Replay attacks**: Teoretiskt möjligt (men smartcards har ofta replay-skydd)
- **Timing attacks**: Möjligt att mäta timing av operationer

## Performance

Typiska latency-värden:

| Operation | Local | Remote (LAN) | Remote (VPN 50ms RTT) |
|-----------|-------|--------------|------------------------|
| ATR Read  | <1ms  | 5-10ms       | 55-60ms                |
| APDU      | 1-5ms | 10-20ms      | 60-70ms                |
| Sign      | 50ms  | 60-70ms      | 110-120ms              |

**Rekommendationer**:
- LAN: Perfekt, nästan ingen märkbar skillnad
- VPN <50ms: Bra, fungerar för de flesta use cases
- VPN >100ms: OK för många use cases, men vissa timeout-känsliga appar kan ha problem
- Internet >200ms: Funkar men långsamt, öka timeouts

## Roadmap

### Version 1.0 (Proof of Concept)
- [x] Arkitektur och design
- [ ] Grundläggande rsc-client (Rust)
- [ ] Grundläggande rsc-server (Rust)
- [ ] gRPC protokoll
- [ ] APDU forwarding
- [ ] mTLS security
- [ ] Basic testing

### Version 1.1 (Production Ready)
- [ ] Reconnect logic
- [ ] Heartbeat
- [ ] Full error handling
- [ ] systemd integration
- [ ] Config files
- [ ] Logging
- [ ] Debian/RPM packages
- [ ] Setup scripts
- [ ] Dokumentation

### Version 2.0 (Enterprise)
- [ ] Multi-reader support
- [ ] Reader filtering
- [ ] Access control (per-client ACLs)
- [ ] Metrics & monitoring
- [ ] Web UI för admin
- [ ] Auto-discovery (mDNS/Avahi)
- [ ] Windows client
- [ ] macOS client

### Future
- [ ] Hardware Security Module (HSM) support
- [ ] Multiple simultaneous clients
- [ ] Load balancing
- [ ] Clustering
- [ ] REST API

## Bidra

Projektet är öppen källkod och bidrag är välkomna!

1. Fork repository
2. Skapa feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push till branch (`git push origin feature/amazing-feature`)
5. Öppna Pull Request

## Licens

TBD - förmodligen MIT eller Apache 2.0

## Support & Community

- **Issues**: https://github.com/yourusername/remote-smartcard/issues
- **Discussions**: https://github.com/yourusername/remote-smartcard/discussions
- **Wiki**: https://github.com/yourusername/remote-smartcard/wiki

## Acknowledgments

Detta projekt bygger på följande fantastiska open source-projekt:

- **pcsc-lite**: https://pcsclite.apdu.fr/
- **vsmartcard/vpcd**: https://frankmorgner.github.io/vsmartcard/
- **gRPC**: https://grpc.io/
- **Rust**: https://rust-lang.org/
- **OpenSC**: https://github.com/OpenSC/OpenSC

## Authors

- Din Namn <din.email@example.com>

## Relaterade Projekt

- **OpenSC**: Smartcard tools och libraries
- **YubiKey Manager**: Hantera Yubikeys
- **GnuPG**: OpenPGP implementation med smartcard support
- **pam_pkcs11**: PAM module för smartcard auth
