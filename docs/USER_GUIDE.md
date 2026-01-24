# Remote Smartcard User Guide

This comprehensive guide explains how to install, configure, and use Remote Smartcard to access your local smartcard or Yubikey from a remote server.

## Table of Contents

1. [Introduction](#introduction)
2. [Quick Start](#quick-start)
3. [Installation](#installation)
4. [Configuration](#configuration)
5. [Usage Examples](#usage-examples)
6. [Troubleshooting](#troubleshooting)
7. [Security Best Practices](#security-best-practices)
8. [FAQ](#faq)

**Platform-specific guides:**
- [Fedora / Rocky Linux / RHEL Setup](FEDORA_ROCKY_SETUP.md)

---

## Introduction

### What is Remote Smartcard?

Remote Smartcard (rsc) lets you use a smartcard or Yubikey connected to your local computer on a remote server, as if it were physically connected there. This enables:

- **SSH authentication** with your Yubikey on remote servers
- **Git commit signing** from remote development environments
- **Certificate-based authentication** in browsers on remote desktops
- Any application using PC/SC (Personal Computer/Smart Card) interface

### How it Works

```
┌─────────────────┐                      ┌─────────────────┐
│  Local Machine  │                      │  Remote Server  │
│                 │                      │                 │
│  ┌───────────┐  │    TLS/gRPC over     │  ┌───────────┐  │
│  │ Yubikey/  │  │      Network         │  │Application│  │
│  │ Smartcard │  │  ◄──────────────►    │  │(SSH, Git) │  │
│  └─────┬─────┘  │                      │  └─────┬─────┘  │
│        │        │                      │        │        │
│  ┌─────▼─────┐  │                      │  ┌─────▼─────┐  │
│  │   pcscd   │  │                      │  │   pcscd   │  │
│  └─────┬─────┘  │                      │  └─────┬─────┘  │
│        │        │                      │        │        │
│  ┌─────▼─────┐  │                      │  ┌─────▼─────┐  │
│  │rsc-client │◄─┼──────────────────────┼─►│rsc-server │  │
│  └───────────┘  │                      │  └─────┬─────┘  │
│                 │                      │        │        │
│                 │                      │  ┌─────▼─────┐  │
│                 │                      │  │   vpcd    │  │
│                 │                      │  │ (virtual  │  │
│                 │                      │  │  reader)  │  │
│                 │                      │  └───────────┘  │
└─────────────────┘                      └─────────────────┘
```

---

## Quick Start

### Prerequisites

- **Local machine**: Linux/macOS/Windows with smartcard reader and card inserted
- **Remote server**: Linux with root access
- **Network**: Both machines can communicate (port 8443)

### 5-Minute Setup

**1. On the server (where you want to use the smartcard):**

```bash
# Download and install
curl -LO https://github.com/alun-hub/remote-smartcard/releases/latest/download/rsc-server-linux-amd64
sudo mv rsc-server-linux-amd64 /usr/local/bin/rsc-server
sudo chmod +x /usr/local/bin/rsc-server

# Install vpcd (virtual smartcard reader)
sudo apt-get install vsmartcard-vpcd  # Debian/Ubuntu
# or: sudo dnf install vsmartcard-vpcd  # Fedora/RHEL

# Generate certificates
mkdir -p ~/rsc-certs && cd ~/rsc-certs
openssl req -x509 -newkey rsa:4096 -keyout server.key -out server.crt -days 365 -nodes \
    -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

# Start server
rsc-server --port 8443 --tls-cert server.crt --tls-key server.key
```

**2. On the client (where your smartcard is connected):**

```bash
# Download and install
curl -LO https://github.com/alun-hub/remote-smartcard/releases/latest/download/rsc-client-linux-amd64
sudo mv rsc-client-linux-amd64 /usr/local/bin/rsc-client
sudo chmod +x /usr/local/bin/rsc-client

# Copy server certificate (for verification)
scp server:~/rsc-certs/server.crt ~/

# Connect to server (use server's IP)
rsc-client --server https://SERVER_IP:8443 --tls-ca ~/server.crt
```

**3. Verify on the server:**

```bash
# List readers - should show your remote smartcard!
pcsc_scan

# Output:
# Reader 0: Virtual PCD (Yubico YubiKey)
#   Card state: Card inserted
#   ATR: 3B 8D 80 01 ...
```

---

## Installation

### Option 1: From Binary Release

Download pre-built binaries from the [releases page](https://github.com/alun-hub/remote-smartcard/releases).

**Server:**
```bash
# Download
curl -LO https://github.com/alun-hub/remote-smartcard/releases/latest/download/rsc-server-linux-amd64.tar.gz
tar xzf rsc-server-linux-amd64.tar.gz

# Install
sudo mv rsc-server /usr/local/bin/
sudo mv rsc-keygen /usr/local/bin/

# Install systemd service
sudo mv rsc-server.service /etc/systemd/system/
sudo systemctl daemon-reload
```

**Client:**
```bash
curl -LO https://github.com/alun-hub/remote-smartcard/releases/latest/download/rsc-client-linux-amd64.tar.gz
tar xzf rsc-client-linux-amd64.tar.gz
sudo mv rsc-client /usr/local/bin/
sudo mv rsc-client.service /etc/systemd/system/
sudo systemctl daemon-reload
```

### Option 2: From Package (Debian/Ubuntu)

```bash
# Server
sudo dpkg -i rsc-server_0.1.0_amd64.deb

# Client
sudo dpkg -i rsc-client_0.1.0_amd64.deb
```

### Option 3: From Package (RHEL/Fedora)

```bash
# Server
sudo rpm -i rsc-server-0.1.0-1.x86_64.rpm

# Client
sudo rpm -i rsc-client-0.1.0-1.x86_64.rpm
```

### Option 4: Build from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install dependencies
sudo apt-get install protobuf-compiler libpcsclite-dev  # Debian/Ubuntu
# or: sudo dnf install protobuf-compiler pcsc-lite-devel  # Fedora/RHEL

# Clone and build
git clone https://github.com/alun-hub/remote-smartcard.git
cd remote-smartcard
cargo build --release

# Binaries are in target/release/
ls target/release/rsc-*
```

### Install Dependencies

**On the server:**
```bash
# Debian/Ubuntu
sudo apt-get install pcscd libpcsclite1 vsmartcard-vpcd opensc

# Fedora/RHEL
sudo dnf install pcsc-lite vsmartcard-vpcd opensc

# Start pcscd
sudo systemctl enable pcscd
sudo systemctl start pcscd
```

**On the client:**
```bash
# Debian/Ubuntu
sudo apt-get install pcscd libpcsclite1 opensc

# Fedora/RHEL
sudo dnf install pcsc-lite opensc

# Start pcscd
sudo systemctl enable pcscd
sudo systemctl start pcscd

# Verify your smartcard is detected
pcsc_scan
```

---

## Configuration

### Certificate Setup

Remote Smartcard uses TLS with mutual authentication (mTLS) for security.

#### Generate Certificates with rsc-keygen

**On the server (create CA and server cert):**
```bash
# Create certificate directory
sudo mkdir -p /etc/rsc-server/certs
cd /etc/rsc-server/certs

# Generate CA and server certificate
sudo rsc-keygen \
    --ca-name "My RSC CA" \
    --server-name $(hostname -f) \
    --server-ip $(hostname -I | awk '{print $1}') \
    --output .

# Files created:
# - ca.crt (CA certificate - share with clients)
# - ca.key (CA private key - keep secure!)
# - server.crt (server certificate)
# - server.key (server private key)
```

**Generate client certificate:**
```bash
# On the server, generate a client cert
sudo rsc-keygen \
    --client-name "my-laptop" \
    --ca-cert /etc/rsc-server/certs/ca.crt \
    --ca-key /etc/rsc-server/certs/ca.key \
    --output /tmp

# Copy to client
scp /tmp/my-laptop.crt /tmp/my-laptop.key client:~/.rsc/
scp /etc/rsc-server/certs/ca.crt client:~/.rsc/
```

#### Generate Certificates with OpenSSL

**Create CA:**
```bash
# Generate CA key and certificate
openssl genrsa -out ca.key 4096
openssl req -x509 -new -nodes -key ca.key -sha256 -days 3650 \
    -out ca.crt -subj "/CN=RSC CA"
```

**Create server certificate:**
```bash
# Generate server key
openssl genrsa -out server.key 4096

# Create CSR
openssl req -new -key server.key -out server.csr \
    -subj "/CN=server.example.com"

# Create extension file for SANs
cat > server.ext << EOF
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyEncipherment
subjectAltName = @alt_names

[alt_names]
DNS.1 = server.example.com
DNS.2 = localhost
IP.1 = 192.168.1.100
IP.2 = 127.0.0.1
EOF

# Sign with CA
openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out server.crt -days 3650 -sha256 \
    -extfile server.ext
```

**Create client certificate:**
```bash
openssl genrsa -out client.key 4096
openssl req -new -key client.key -out client.csr \
    -subj "/CN=my-laptop"
openssl x509 -req -in client.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out client.crt -days 3650 -sha256
```

### Server Configuration

**Command-line options:**
```bash
rsc-server --help

Options:
  -p, --port <PORT>           Port to listen on [default: 8443]
  -b, --bind <ADDRESS>        Address to bind to [default: 0.0.0.0]
      --tls-cert <FILE>       Server certificate (PEM)
      --tls-key <FILE>        Server private key (PEM)
      --tls-ca <FILE>         CA certificate for client verification (PEM)
  -l, --log-level <LEVEL>     Log level [default: info]
      --vpcd-host <HOST>      vpcd host [default: 127.0.0.1]
      --vpcd-port <PORT>      vpcd port [default: 35963]
```

**Example - start server with mTLS:**
```bash
rsc-server \
    --port 8443 \
    --bind 0.0.0.0 \
    --tls-cert /etc/rsc-server/certs/server.crt \
    --tls-key /etc/rsc-server/certs/server.key \
    --tls-ca /etc/rsc-server/certs/ca.crt \
    --log-level info
```

**Systemd service configuration:**

Edit `/etc/systemd/system/rsc-server.service`:
```ini
[Service]
ExecStart=/usr/local/bin/rsc-server \
    --port 8443 \
    --bind 0.0.0.0 \
    --tls-cert /etc/rsc-server/certs/server.crt \
    --tls-key /etc/rsc-server/certs/server.key \
    --tls-ca /etc/rsc-server/certs/ca.crt
```

Then:
```bash
sudo systemctl daemon-reload
sudo systemctl enable rsc-server
sudo systemctl start rsc-server
```

### Client Configuration

**Command-line options:**
```bash
rsc-client --help

Options:
  -s, --server <URL>          Server URL [default: http://127.0.0.1:8443]
      --client-id <ID>        Client identifier [default: hostname]
      --tls-ca <FILE>         CA certificate for server verification
      --tls-cert <FILE>       Client certificate for mTLS
      --tls-key <FILE>        Client private key for mTLS
      --tls-server-name <N>   Server name for TLS verification (SNI)
      --reconnect-delay <S>   Initial reconnect delay [default: 1]
      --reconnect-max-delay <S>  Max reconnect delay [default: 60]
      --no-reconnect          Disable automatic reconnection
  -l, --log-level <LEVEL>     Log level [default: info]
```

**Example - connect to server with mTLS:**
```bash
rsc-client \
    --server https://server.example.com:8443 \
    --tls-ca ~/.rsc/ca.crt \
    --tls-cert ~/.rsc/client.crt \
    --tls-key ~/.rsc/client.key \
    --client-id "my-laptop"
```

**Systemd service configuration:**

Edit `/etc/systemd/system/rsc-client.service`:
```ini
[Service]
ExecStart=/usr/local/bin/rsc-client \
    --server https://server.example.com:8443 \
    --tls-ca /etc/rsc-client/certs/ca.crt \
    --tls-cert /etc/rsc-client/certs/client.crt \
    --tls-key /etc/rsc-client/certs/client.key
```

---

## Usage Examples

### Example 1: SSH Authentication with Yubikey

**Scenario:** You have a Yubikey on your laptop and want to SSH from a remote server to another machine using the Yubikey.

**Setup:**

1. Start rsc-server on the remote server
2. Start rsc-client on your laptop with Yubikey
3. On the remote server:

```bash
# Verify the Yubikey is visible
pcsc_scan
# Should show: Virtual PCD (Yubico YubiKey)

# Extract SSH public key from Yubikey
ssh-keygen -D /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so -e

# Add this public key to target server's authorized_keys

# SSH using the Yubikey
ssh -I /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so user@target-server
```

**Make it permanent in ~/.ssh/config:**
```
Host target-server
    HostName target.example.com
    User myuser
    PKCS11Provider /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so
```

Now you can simply run:
```bash
ssh target-server
# PIN prompt appears on your LOCAL laptop!
```

### Example 2: Git Commit Signing

**Scenario:** Sign git commits with your Yubikey's GPG key from a remote development server.

**Setup:**

1. Configure GPG to use the smartcard:
```bash
# On the remote server
gpg --card-status

# Should show your Yubikey's GPG key info
# If it shows "No card" - check that rsc-client is connected

# Get the key ID
gpg --card-status | grep "Signature key"
# Output: Signature key ....: ABCD 1234 5678 90EF ...
```

2. Configure git:
```bash
git config --global user.signingkey ABCD123456789EF
git config --global commit.gpgsign true
git config --global gpg.program gpg2
```

3. Create a signed commit:
```bash
cd your-repo
echo "test" > test.txt
git add test.txt
git commit -m "Signed commit from remote server"

# PIN prompt appears on your LOCAL laptop!
# Commit is signed with your Yubikey
```

4. Verify the signature:
```bash
git log --show-signature -1
# Shows: Good signature from "Your Name <your@email.com>"
```

### Example 3: Browser Client Certificates

**Scenario:** Use client certificates stored on your Yubikey in Firefox on a remote desktop.

**Setup:**

1. Start rsc-server and rsc-client
2. Configure Firefox on the remote server:

```
Firefox → Settings → Privacy & Security → Security Devices

Click "Load"
Module Name: OpenSC PKCS#11
Module filename: /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so

Click OK
```

3. Browse to a site requiring client certificate
4. Firefox will prompt to select certificate from your Yubikey
5. PIN prompt appears on your LOCAL laptop

### Example 4: Code Signing

**Scenario:** Sign code or packages using a certificate on your smartcard.

```bash
# Sign a file using OpenSSL with PKCS#11 engine
openssl dgst -engine pkcs11 -keyform engine \
    -sign "pkcs11:object=Private%20Key" \
    -out signature.bin \
    file-to-sign.txt

# The PIN prompt will appear on your local machine
```

### Example 5: VPN Authentication

**Scenario:** Connect to a VPN that requires smartcard authentication.

```bash
# OpenVPN with PKCS#11
sudo openvpn --config vpn.conf \
    --pkcs11-providers /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so \
    --pkcs11-id 'YOUR_CERT_ID'
```

---

## Troubleshooting

### Common Issues

#### "No readers found" on server

**Symptoms:**
```bash
pcsc_scan
# Waiting for reader...
```

**Solutions:**

1. Check that rsc-server is running:
```bash
systemctl status rsc-server
# or
ps aux | grep rsc-server
```

2. Check that rsc-client is connected:
```bash
journalctl -u rsc-server -f
# Look for: "Session established" message
```

3. Restart pcscd on the server:
```bash
sudo systemctl restart pcscd
```

4. Check vpcd is working:
```bash
# vpcd should be started by rsc-server
# Check logs for vpcd-related errors
journalctl -u rsc-server | grep -i vpcd
```

#### Client can't connect to server

**Symptoms:**
```
ERROR Connection failed: Connection refused
```

**Solutions:**

1. Verify server is listening:
```bash
# On server
ss -tlnp | grep 8443
```

2. Check firewall:
```bash
# UFW
sudo ufw allow 8443/tcp

# firewalld
sudo firewall-cmd --add-port=8443/tcp --permanent
sudo firewall-cmd --reload
```

3. Test connectivity:
```bash
# From client
nc -zv server.example.com 8443
```

#### TLS handshake failed

**Symptoms:**
```
ERROR TLS error: certificate verify failed
```

**Solutions:**

1. Verify CA certificate is correct:
```bash
openssl verify -CAfile ca.crt server.crt
# Should say: server.crt: OK
```

2. Check server certificate CN/SAN matches hostname:
```bash
openssl x509 -in server.crt -text -noout | grep -A1 "Subject Alternative Name"
```

3. Use --tls-server-name if hostname doesn't match:
```bash
rsc-client --server https://192.168.1.100:8443 \
    --tls-server-name server.example.com \
    --tls-ca ca.crt
```

#### Smartcard operations are slow

**Causes and solutions:**

1. **High network latency**: Use a VPN or dedicated connection
2. **Increase timeouts**: Some applications have configurable timeouts
3. **Check for packet loss**: `ping -c 100 server` and look at packet loss

#### "Card not present" on server

**Solutions:**

1. Verify card is detected locally:
```bash
# On client machine
pcsc_scan
```

2. Check client logs:
```bash
journalctl -u rsc-client -f
```

3. Try reinserting the card

### Debug Mode

Enable verbose logging for troubleshooting:

```bash
# Server
rsc-server --log-level debug ...

# Client
rsc-client --log-level debug ...
```

For even more detail:
```bash
rsc-server --log-level trace ...
```

### Log Files

**Systemd journal:**
```bash
# Server logs
journalctl -u rsc-server -f

# Client logs
journalctl -u rsc-client -f

# Filter by time
journalctl -u rsc-server --since "10 minutes ago"
```

---

## Security Best Practices

### Certificate Management

1. **Protect the CA private key**
   - Store on encrypted filesystem
   - Consider using an HSM
   - Never transfer over unencrypted channels

2. **Use strong keys**
   - Minimum 4096-bit RSA or 256-bit ECC
   - Use SHA-256 or better for signatures

3. **Set appropriate certificate validity**
   - CA: 10 years
   - Server/Client: 1-2 years
   - Plan for rotation

4. **Unique client certificates**
   - Each client should have its own certificate
   - Makes revocation easier
   - Enables audit trails

### Network Security

1. **Firewall the server**
   - Only allow connections from trusted networks
   - Use VPN if accessing over internet

2. **Don't expose to internet**
   - Run over VPN or private network when possible
   - If internet-exposed, use fail2ban or similar

3. **Monitor connections**
   - Check logs regularly
   - Alert on failed authentication attempts

### Operational Security

1. **Keep software updated**
   ```bash
   # Check for updates
   rsc-server --version
   ```

2. **Audit logs regularly**
   ```bash
   # Look for suspicious activity
   journalctl -u rsc-server | grep -i "error\|fail\|denied"
   ```

3. **Backup certificates**
   - Keep secure backups of CA key
   - Document certificate generation process

---

## FAQ

### General Questions

**Q: Does my PIN go over the network?**

A: No! The PIN is entered locally and never transmitted. Only encrypted APDU commands (which don't contain the PIN in plaintext) are sent over the network.

**Q: Can multiple clients connect to one server?**

A: Yes, each client gets its own virtual smartcard reader on the server.

**Q: What smartcards are supported?**

A: Any smartcard that works with PC/SC, including:
- Yubikey (all models with smartcard functionality)
- OpenPGP cards
- PIV cards
- JavaCards
- Most bank/government ID cards

**Q: Does it work with Windows clients?**

A: Currently, only Linux clients are fully supported. Windows and macOS clients are planned for future versions.

**Q: What's the performance impact?**

A: Typical overhead is 10-50ms per operation on a LAN. For most use cases (SSH, git signing), this is imperceptible.

### Technical Questions

**Q: What protocol does Remote Smartcard use?**

A: gRPC over HTTP/2 with TLS 1.3. The protocol is efficient and supports bidirectional streaming for low-latency APDU forwarding.

**Q: Can I use it without TLS?**

A: Not recommended, but for testing you can use `http://` instead of `https://` and omit TLS options. Never do this in production.

**Q: How does reconnection work?**

A: The client automatically reconnects with exponential backoff (1s, 2s, 4s, ... up to 60s). Session state is preserved when possible.

**Q: Can I filter which readers are shared?**

A: Currently all local readers are shared. Reader filtering is planned for a future version.

---

## Getting Help

- **Documentation**: https://github.com/alun-hub/remote-smartcard/docs
- **Issues**: https://github.com/alun-hub/remote-smartcard/issues
- **Discussions**: https://github.com/alun-hub/remote-smartcard/discussions

When reporting issues, please include:
1. OS and version (client and server)
2. rsc-client/rsc-server version (`--version`)
3. Smartcard type
4. Relevant log output (with `--log-level debug`)
