# Setup Guide - Remote Smartcard

Detaljerad guide för att sätta upp Remote Smartcard från scratch.

## Förutsättningar

### Hårdvara
- **Klient**: Dator med USB-port för smartcard reader/Yubikey
- **Server**: Linux server (fysisk eller VM) som klienten kan nå över nätverket
- **Smartcard**: Yubikey, smartcard med PIV/OpenPGP, eller annat CCID-kompatibelt kort
- **Nätverk**: Klient och server måste kunna kommunicera (LAN, VPN, eller internet med portar öppna)

### Software (före installation)

#### På båda klient OCH server:
- Linux OS (testat på Debian 12, Ubuntu 22.04, RHEL 9)
- Root/sudo access
- Internet access för paketinstallation

#### På klienten:
- Fungerande smartcard reader med kort isatt

#### På servern:
- Port 8443 öppen (eller annan port du väljer)

---

## Del 1: Preparation

### 1.1 Verifiera system

**På klient:**
```bash
# Kontrollera OS version
lsb_release -a
uname -a

# Kontrollera att smartcard reader fungerar
lsusb | grep -i yubico  # eller grep -i smartcard

# Kontrollera att pcscd är installerat
systemctl status pcscd
```

**På server:**
```bash
# Kontrollera OS version
lsb_release -a

# Kontrollera att port är öppen (om firewall finns)
sudo ss -tlnp | grep 8443
```

---

## Del 2: Installera Dependencies

### 2.1 På Debian/Ubuntu

**Server:**
```bash
# Uppdatera paketlistor
sudo apt-get update

# Installera PC/SC stack
sudo apt-get install -y \
    pcscd \
    libpcsclite1 \
    libccid \
    opensc

# Installera vpcd (virtual reader)
# Option A: Från package (om tillgängligt)
sudo apt-get install -y vpcsc

# Option B: Bygg från source (om package saknas)
sudo apt-get install -y \
    git \
    build-essential \
    pkg-config \
    help2man \
    gengetopt \
    libnfc-dev

git clone https://github.com/frankmorgner/vsmartcard.git
cd vsmartcard/virtualsmartcard
autoreconf --install
./configure
make
sudo make install

cd ../vpcd
autoreconf --install
./configure
make
sudo make install
```

**Klient:**
```bash
sudo apt-get update

sudo apt-get install -y \
    pcscd \
    libpcsclite1 \
    libccid \
    opensc
```

### 2.2 På RHEL/CentOS/Fedora

**Server:**
```bash
sudo dnf install -y \
    pcsc-lite \
    pcsc-lite-ccid \
    opensc

# vpcd (kan behöva EPEL eller bygga från source)
sudo dnf install -y epel-release
sudo dnf install -y vsmartcard-vpcd

# Eller bygg från source (samma som Debian ovan)
```

**Klient:**
```bash
sudo dnf install -y \
    pcsc-lite \
    pcsc-lite-ccid \
    opensc
```

### 2.3 Verifiera installation

**På båda:**
```bash
# Start pcscd
sudo systemctl enable pcscd
sudo systemctl start pcscd
sudo systemctl status pcscd

# Ska visa "active (running)"
```

**På klient (med kort isatt):**
```bash
# Lista readers
pcsc_scan

# Output bör visa din reader och kort-info:
# Reader 0: Yubico YubiKey OTP+FIDO+CCID 00 00
#   Card state: Card inserted,
#   ATR: 3B 8D 80 01 ...
```

**På server:**
```bash
# Testa att vpcd kan startas
vpcd --help

# Ska visa help text utan error
```

---

## Del 3: Bygg och Installera rsc

### 3.1 Installera Rust (om bygger från source)

**På båda (klient och server):**
```bash
# Installera rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Följ instruktioner, välj default installation

# Ladda environment
source $HOME/.cargo/env

# Verifiera
rustc --version
cargo --version
```

### 3.2 Bygg rsc från source

**På build-maskin (kan vara samma som klient/server):**
```bash
# Klona repository
git clone https://github.com/yourusername/remote-smartcard.git
cd remote-smartcard

# Bygg protokoll (generera Rust kod från .proto)
# Kräver protobuf compiler
sudo apt-get install -y protobuf-compiler  # Debian/Ubuntu
# eller
sudo dnf install -y protobuf-compiler      # RHEL/Fedora

# Bygg client
cd client
cargo build --release

# Binary finns nu i: target/release/rsc-client

# Bygg server
cd ../server
cargo build --release

# Binary finns nu i: target/release/rsc-server

# Bygg keygen tool
cd ../tools/keygen
cargo build --release

# Binary: target/release/rsc-keygen
```

### 3.3 Installera binaries

**På server:**
```bash
# Kopiera binary
sudo cp server/target/release/rsc-server /usr/local/bin/
sudo chmod +x /usr/local/bin/rsc-server

# Kopiera keygen
sudo cp tools/keygen/target/release/rsc-keygen /usr/local/bin/
sudo chmod +x /usr/local/bin/rsc-keygen

# Skapa config directory
sudo mkdir -p /etc/rsc-server/certs
sudo mkdir -p /var/log/rsc-server
sudo mkdir -p /var/run/rsc-server/vpcd

# Kopiera systemd service
sudo cp systemd/rsc-server.service /etc/systemd/system/
sudo systemctl daemon-reload
```

**På klient:**
```bash
# Kopiera binary
sudo cp client/target/release/rsc-client /usr/local/bin/
sudo chmod +x /usr/local/bin/rsc-client

# Skapa config directory
sudo mkdir -p /etc/rsc-client/certs
sudo mkdir -p /var/log/rsc-client

# Kopiera systemd service
sudo cp systemd/rsc-client.service /etc/systemd/system/
sudo systemctl daemon-reload
```

---

## Del 4: Generera Certifikat

### 4.1 Skapa Certificate Authority (CA)

**På server (eller dedikerad CA-maskin):**
```bash
# Generera CA private key (4096-bit RSA)
openssl genrsa -out /tmp/ca.key 4096

# Skapa CA certificate (10 års giltighet)
openssl req -x509 -new -nodes \
    -key /tmp/ca.key \
    -sha256 \
    -days 3650 \
    -out /tmp/ca.crt \
    -subj "/C=SE/ST=Stockholm/L=Stockholm/O=RemoteSmartcard/OU=CA/CN=RSC Root CA"

# Flytta till säker plats
sudo mv /tmp/ca.key /etc/rsc-server/certs/ca.key
sudo mv /tmp/ca.crt /etc/rsc-server/certs/ca.crt

# Sätt permissions (CA key är mycket känslig!)
sudo chmod 600 /etc/rsc-server/certs/ca.key
sudo chmod 644 /etc/rsc-server/certs/ca.crt
```

### 4.2 Generera Server Certificate

**På server:**
```bash
# Generera server private key
openssl genrsa -out /tmp/server.key 4096

# Skapa Certificate Signing Request
# VIKTIGT: CN måste matcha server hostname/IP
openssl req -new \
    -key /tmp/server.key \
    -out /tmp/server.csr \
    -subj "/C=SE/ST=Stockholm/L=Stockholm/O=RemoteSmartcard/OU=Server/CN=your-server.example.com"

# Signera med CA
openssl x509 -req \
    -in /tmp/server.csr \
    -CA /etc/rsc-server/certs/ca.crt \
    -CAkey /etc/rsc-server/certs/ca.key \
    -CAcreateserial \
    -out /tmp/server.crt \
    -days 3650 \
    -sha256

# Flytta certifikat
sudo mv /tmp/server.key /etc/rsc-server/certs/server.key
sudo mv /tmp/server.crt /etc/rsc-server/certs/server.crt

# Cleanup
rm /tmp/server.csr

# Permissions
sudo chmod 600 /etc/rsc-server/certs/server.key
sudo chmod 644 /etc/rsc-server/certs/server.crt
```

### 4.3 Generera Client Certificate

**På server (eller CA-maskin), sedan kopiera till klient:**
```bash
# Generera client private key
openssl genrsa -out /tmp/client.key 4096

# Skapa CSR
# CN kan vara hostname eller annat unikt ID
openssl req -new \
    -key /tmp/client.key \
    -out /tmp/client.csr \
    -subj "/C=SE/ST=Stockholm/L=Stockholm/O=RemoteSmartcard/OU=Client/CN=client-laptop"

# Signera med CA
openssl x509 -req \
    -in /tmp/client.csr \
    -CA /etc/rsc-server/certs/ca.crt \
    -CAkey /etc/rsc-server/certs/ca.key \
    -CAcreateserial \
    -out /tmp/client.crt \
    -days 3650 \
    -sha256

# Cleanup
rm /tmp/client.csr

# Certifikaten är klara i /tmp/, redo att kopieras till klient
```

### 4.4 Kopiera Certifikat till Klient

**På klient:**
```bash
# Kopiera från server (säkert över SSH)
scp server:/tmp/client.key /tmp/
scp server:/tmp/client.crt /tmp/
scp server:/etc/rsc-server/certs/ca.crt /tmp/

# Flytta till config directory
sudo mv /tmp/client.key /etc/rsc-client/certs/
sudo mv /tmp/client.crt /etc/rsc-client/certs/
sudo mv /tmp/ca.crt /etc/rsc-client/certs/

# Permissions
sudo chmod 600 /etc/rsc-client/certs/client.key
sudo chmod 644 /etc/rsc-client/certs/client.crt
sudo chmod 644 /etc/rsc-client/certs/ca.crt
```

**På server (cleanup):**
```bash
# Ta bort temporära certifikat
rm /tmp/client.key /tmp/client.crt
```

### 4.5 Verifiera Certifikat

```bash
# Verifiera CA cert
openssl x509 -in /etc/rsc-server/certs/ca.crt -text -noout

# Verifiera server cert
openssl x509 -in /etc/rsc-server/certs/server.crt -text -noout

# Verifiera att server cert är signerat av CA
openssl verify -CAfile /etc/rsc-server/certs/ca.crt \
    /etc/rsc-server/certs/server.crt

# Ska visa: /etc/rsc-server/certs/server.crt: OK
```

---

## Del 5: Konfigurera rsc

### 5.1 Server Configuration

**Skapa `/etc/rsc-server/config.yaml`:**
```yaml
server:
  # Lyssna på alla interfaces
  bind_address: "0.0.0.0"
  # Port för gRPC server
  port: 8443
  # Max antal samtidiga klienter
  max_clients: 100

tls:
  # Server certifikat och nyckel
  server_cert: "/etc/rsc-server/certs/server.crt"
  server_key: "/etc/rsc-server/certs/server.key"
  # CA för att verifiera klient-certifikat
  ca_cert: "/etc/rsc-server/certs/ca.crt"
  # Kräv att klienter autentiserar med certifikat
  require_client_cert: true
  # TLS version (minimum)
  min_tls_version: "1.3"

vpcd:
  # Path till vpcd binary
  binary: "/usr/local/bin/vpcd"
  # Directory för vpcd sockets
  socket_dir: "/var/run/rsc-server/vpcd"
  # Timeout för vpcd operations
  timeout: 30s
  # vpcd port range (en port per klient)
  port_range_start: 35963
  port_range_end: 36963

logging:
  # Log level: error, warn, info, debug, trace
  level: "info"
  # Log till fil
  file: "/var/log/rsc-server/rsc-server.log"
  # Även logga till stdout (för systemd journal)
  stdout: true
  # Strukturerad logging (JSON format)
  json: false

# Optional: metrics och monitoring
metrics:
  enabled: false
  prometheus_port: 9090
```

### 5.2 Client Configuration

**Skapa `/etc/rsc-client/config.yaml`:**
```yaml
server:
  # Server hostname eller IP
  # MÅSTE matcha CN i server-certifikatet!
  host: "your-server.example.com"
  # Port
  port: 8443

tls:
  # Klient certifikat och nyckel
  client_cert: "/etc/rsc-client/certs/client.crt"
  client_key: "/etc/rsc-client/certs/client.key"
  # CA för att verifiera server-certifikat
  ca_cert: "/etc/rsc-client/certs/ca.crt"
  # Verifiera server certificate
  verify_server: true

client:
  # Unikt ID för denna klient (default: hostname)
  client_id: "laptop-home"

  # Optional: Filtrera vilka readers som ska forwardas
  # Om tom: alla readers forwardas
  reader_filter:
    - "Yubico"
    # - "Gemalto"

  # Reconnect settings
  reconnect_interval: 5s
  reconnect_max_attempts: 0  # 0 = oändligt
  reconnect_backoff_max: 60s  # Max backoff time

  # Heartbeat för att hålla connection vid liv
  heartbeat_interval: 30s

  # Timeout för operationer
  operation_timeout: 10s

logging:
  level: "info"
  file: "/var/log/rsc-client/rsc-client.log"
  stdout: true
  json: false
```

---

## Del 6: Starta Services

### 6.1 Konfigurera Firewall (om behövs)

**På server:**
```bash
# UFW (Debian/Ubuntu)
sudo ufw allow 8443/tcp
sudo ufw reload

# firewalld (RHEL/CentOS)
sudo firewall-cmd --permanent --add-port=8443/tcp
sudo firewall-cmd --reload

# iptables (manuellt)
sudo iptables -A INPUT -p tcp --dport 8443 -j ACCEPT
sudo iptables-save > /etc/iptables/rules.v4
```

### 6.2 Starta Server

```bash
# Enable service (startar vid boot)
sudo systemctl enable rsc-server

# Starta nu
sudo systemctl start rsc-server

# Kontrollera status
sudo systemctl status rsc-server

# Bör visa:
# ● rsc-server.service - Remote Smartcard Server
#    Loaded: loaded (/etc/systemd/system/rsc-server.service; enabled)
#    Active: active (running) since ...

# Kolla logs
sudo journalctl -u rsc-server -f
```

### 6.3 Starta Client

```bash
# Enable service
sudo systemctl enable rsc-client

# Starta nu
sudo systemctl start rsc-client

# Kontrollera status
sudo systemctl status rsc-client

# Kolla logs
sudo journalctl -u rsc-client -f

# Du bör se något som:
# INFO Connected to server your-server.example.com:8443
# INFO Session established: session_id=abc123
# INFO Reader found: Yubico YubiKey OTP+FIDO+CCID 00 00
# INFO Card present, ATR: 3B8D8001...
```

---

## Del 7: Verifiering

### 7.1 Verifiera att Virtual Reader Syns på Server

**På server:**
```bash
# Lista PC/SC readers
pcsc_scan

# Output bör nu visa:
# Reader 0: Virtual PCD 00 00
#   Card state: Card inserted
#   ATR: 3B 8D 80 01 ...  (samma ATR som ditt lokala kort!)

# Om det fungerar är grundsystemet klart! 🎉
```

### 7.2 Testa med OpenSC Tools

**På server:**
```bash
# Lista smartcard
pkcs11-tool --module /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so --list-slots

# Ska visa ditt kort

# Lista certifikat/objekt på kortet
pkcs11-tool --module /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so --list-objects

# Test random number generation (använder kort)
pkcs11-tool --module /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so --test
```

### 7.3 Testa SSH med Smartcard

**Setup SSH på server:**
```bash
# Extrahera pubkey från smartcard
ssh-keygen -D /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so -e > ~/.ssh/smartcard.pub

# Lägg till i authorized_keys på target host
cat ~/.ssh/smartcard.pub | ssh target-host 'cat >> ~/.ssh/authorized_keys'
```

**Testa SSH:**
```bash
# SSH med smartcard authentication
ssh -I /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so user@target-host

# Om det fungerar kommer du bli frågad om PIN på din LOKALA dator
# (klienten), och sedan loggas in på target-host!
```

### 7.4 Testa Git Signing

**Setup på server:**
```bash
# Om du använder GPG på smartcard
gpg --card-status

# Konfigurera git
git config --global user.signingkey "KEYID från card"
git config --global commit.gpgsign true

# Skapa test commit
cd /tmp
git init test-repo
cd test-repo
echo "test" > file.txt
git add file.txt
git commit -m "Test signed commit"

# Om det fungerar är commit signad med ditt smartcard!
git log --show-signature
```

---

## Del 8: Troubleshooting

### Problem: Klient kan inte connecta till server

**Symptom:**
```
ERROR Failed to connect: Connection refused
```

**Debug:**
```bash
# På server: kontrollera att service lyssnar
sudo netstat -tlnp | grep 8443

# Ska visa rsc-server

# Testa TLS connection manuellt
openssl s_client -connect your-server.example.com:8443 \
  -cert /etc/rsc-client/certs/client.crt \
  -key /etc/rsc-client/certs/client.key \
  -CAfile /etc/rsc-client/certs/ca.crt

# Ska connecta och visa certifikat-info
```

**Lösningar:**
- Kontrollera firewall (ufw, firewalld, iptables)
- Kontrollera att server faktiskt lyssnar på rätt port
- Kontrollera att hostname i config matchar server cert CN
- Kontrollera network connectivity: `ping server`

### Problem: TLS handshake failure

**Symptom:**
```
ERROR TLS handshake failed: certificate verify failed
```

**Debug:**
```bash
# Verifiera certifikat-chain
openssl verify -CAfile /etc/rsc-client/certs/ca.crt \
  /etc/rsc-server/certs/server.crt

# Kontrollera server cert CN
openssl x509 -in /etc/rsc-server/certs/server.crt -noout -subject

# CN måste matcha server hostname i client config!
```

**Lösningar:**
- Regenerera certifikat med rätt CN
- Eller: använd IP istället för hostname (och sätt CN till IP)
- Kontrollera att ca.crt är samma på klient och server

### Problem: Ingen reader syns på server

**Symptom:**
```
pcsc_scan
# Shows no readers, eller "Waiting for reader..."
```

**Debug:**
```bash
# På server: kolla rsc-server logs
sudo journalctl -u rsc-server -n 50

# Kolla att vpcd startade
ps aux | grep vpcd

# Kolla vpcd sockets
ls -la /var/run/rsc-server/vpcd/
```

**Lösningar:**
- Restart rsc-server: `sudo systemctl restart rsc-server`
- Restart pcscd: `sudo systemctl restart pcscd`
- Kontrollera att vpcd binary finns: `which vpcd`
- Kontrollera permissions på socket_dir

### Problem: APDU errors

**Symptom:**
```
ERROR APDU transmission failed: Smartcard error
```

**Debug:**
```bash
# På klient: testa lokalt
pcsc_scan

# Ska fungera lokalt

# Kontrollera logs på båda sidor
sudo journalctl -u rsc-client -f   # På klient
sudo journalctl -u rsc-server -f   # På server
```

**Lösningar:**
- Kontrollera network latency: `ping -c 10 server`
- Öka operation_timeout i client config
- Kontrollera att kort inte är i användning av annat program lokalt

### Problem: Reconnect loop

**Symptom:**
```
INFO Connection lost, reconnecting...
INFO Connected
INFO Connection lost, reconnecting...
```

**Lösningar:**
- Kontrollera network stabilitet
- Öka heartbeat_interval
- Kontrollera server logs för errors
- Kontrollera server resource usage (CPU, memory)

---

## Del 9: Production Hardening

### 9.1 Security Checklist

- [ ] Certifikat har starka keys (4096-bit RSA eller 256-bit ECC)
- [ ] CA private key är säkert lagrad (encrypted filesystem, HSM, eller offline)
- [ ] Server endast nåbar från trusted networks (VPN, firewall)
- [ ] Client certifikat har unique CN per klient
- [ ] Logs övervakas för onormala patterns
- [ ] Certifikat har rimlig expiration och rotation-plan
- [ ] Server körs som non-root user (TODO: implementera i systemd service)

### 9.2 Backup Checklist

- [ ] Backup av CA key och cert
- [ ] Backup av server/client keys och certs
- [ ] Backup av config-filer
- [ ] Dokumentera cert generation process

### 9.3 Monitoring

**Metrics att övervaka:**
- Connection count
- APDU transmission rate och latency
- Error rate
- Reconnect frequency

**Setup med Prometheus (optional):**
```yaml
# I server config.yaml
metrics:
  enabled: true
  prometheus_port: 9090
```

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'rsc-server'
    static_configs:
      - targets: ['server:9090']
```

---

## Del 10: Nästa Steg

Efter grundinstallation:

1. **Testa med verkliga applikationer**
   - SSH
   - Git signing
   - Web client certificates
   - Email signing

2. **Performance tuning**
   - Mät latency
   - Justera timeouts
   - Optimera network settings

3. **Dokumentera din specifika setup**
   - Vilka smartcards används
   - Vilka applikationer
   - Eventuella quirks eller workarounds

4. **Setup monitoring**
   - Logrotation
   - Metrics collection
   - Alerting

5. **Planera certifikat-rotation**
   - Sätt påminnelser för cert expiration
   - Testa cert rotation-processen

---

## Support

Om problem uppstår:
- Kolla logs: `journalctl -u rsc-{client,server}`
- Kolla TROUBLESHOOTING.md (TODO)
- Öppna issue på GitHub

## Gratulationer!

Om du kommit hit har du nu ett fungerande Remote Smartcard-system! 🎉

Ditt lokala smartkort kan nu användas transparent från fjärrservern, säkert och effektivt.
