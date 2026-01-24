# Fedora / Rocky Linux / RHEL Setup Guide

This guide covers installation and setup of Remote Smartcard on Fedora, Rocky Linux, RHEL, and AlmaLinux.

## Supported Versions

- Fedora 38, 39, 40+
- Rocky Linux 9.x
- RHEL 9.x
- AlmaLinux 9.x

## Prerequisites

### Enable Required Repositories

**Rocky Linux / RHEL / AlmaLinux:**
```bash
# Enable EPEL repository (for additional packages)
sudo dnf install epel-release

# Enable CRB (CodeReady Builder) for development packages
sudo dnf config-manager --set-enabled crb  # Rocky/Alma
# or
sudo subscription-manager repos --enable codeready-builder-for-rhel-9-$(arch)-rpms  # RHEL
```

**Fedora:**
No additional repositories needed.

## Installation Methods

### Method 1: Install from RPM Package (when releases are available)

> **Note**: Pre-built RPM packages are not yet available. Use Method 2 or 3 instead.

```bash
# Once releases are available, download and install:
# curl -LO https://github.com/alun-hub/remote-smartcard/releases/latest/download/rsc-server-0.1.0-1.el9.x86_64.rpm
# sudo dnf install ./rsc-server-0.1.0-1.el9.x86_64.rpm
```

### Method 2: Build RPM from Source

```bash
# Install build dependencies
sudo dnf groupinstall "Development Tools"
sudo dnf install rust cargo protobuf-compiler pcsc-lite-devel rpm-build rpmdevtools

# Setup RPM build environment
rpmdev-setuptree

# Clone repository
git clone https://github.com/alun-hub/remote-smartcard.git
cd remote-smartcard

# Create source tarball
VERSION=$(grep -m1 'version = ' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
mkdir -p remote-smartcard-${VERSION}
cp -r * remote-smartcard-${VERSION}/ 2>/dev/null || true
tar czf ~/rpmbuild/SOURCES/remote-smartcard-${VERSION}.tar.gz remote-smartcard-${VERSION}

# Copy spec file
cp packaging/rpm/remote-smartcard.spec ~/rpmbuild/SPECS/

# Build RPM
rpmbuild -ba ~/rpmbuild/SPECS/remote-smartcard.spec

# Install
sudo dnf install ~/rpmbuild/RPMS/x86_64/rsc-*.rpm
```

### Method 3: Build and Install from Source

```bash
# Install dependencies
sudo dnf install pcsc-lite pcsc-lite-devel pcsc-lite-ccid opensc protobuf-compiler

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Clone and build
git clone https://github.com/alun-hub/remote-smartcard.git
cd remote-smartcard
cargo build --release

# Install binaries
sudo cp target/release/rsc-server /usr/local/bin/
sudo cp target/release/rsc-client /usr/local/bin/

# Install systemd services
sudo cp systemd/rsc-server.service /etc/systemd/system/
sudo cp systemd/rsc-client.service /etc/systemd/system/
sudo systemctl daemon-reload
```

## Installing vpcd (Virtual Smartcard Reader)

vpcd is required on the server to create virtual smartcard readers.

### Fedora

```bash
# vpcd may be available in Fedora repos
sudo dnf search vsmartcard
sudo dnf install vsmartcard-vpcd  # If available
```

### Rocky Linux / RHEL / AlmaLinux

vpcd is not in the standard repositories. Build from source:

```bash
# Install build dependencies
sudo dnf install git autoconf automake libtool help2man pcsc-lite-devel

# Clone and build vsmartcard (which includes vpcd driver)
git clone https://github.com/frankmorgner/vsmartcard.git
cd vsmartcard/virtualsmartcard
autoreconf --install
./configure
make
sudo make install
sudo ldconfig

# Configure pcscd to load vpcd driver
# Note: LIBPATH may be /usr/lib64/... on 64-bit systems
sudo mkdir -p /etc/reader.conf.d
cat << 'EOF' | sudo tee /etc/reader.conf.d/vpcd.conf
FRIENDLYNAME "Virtual PCD"
DEVICENAME /dev/null:0x8C7B
LIBPATH /usr/lib64/pcsc/drivers/serial/libifdvpcd.so
CHANNELID 0x8C7B
EOF

# Restart pcscd to load the driver
sudo systemctl restart pcscd
```

### Verify vpcd Installation

```bash
# Check that vpcd driver library is installed
find /usr -name "libifdvpcd.so" 2>/dev/null
# Should show something like: /usr/lib64/pcsc/drivers/serial/libifdvpcd.so

# Check that pcscd sees the Virtual PCD reader
pcsc_scan
# Should show "Virtual PCD" in the reader list (will wait for card until rsc-server connects)

# Note: rsc-server connects to vpcd on port 35963 and emulates the card
```

## Server Setup

### 1. Create System User (if not using RPM)

```bash
sudo useradd -r -s /sbin/nologin -d /etc/rsc-server rsc-server
sudo mkdir -p /etc/rsc-server/certs
sudo chown -R rsc-server:rsc-server /etc/rsc-server
sudo chmod 700 /etc/rsc-server/certs
```

### 2. Generate Certificates

```bash
cd /etc/rsc-server/certs

# Generate CA
sudo openssl genrsa -out ca.key 4096
sudo openssl req -x509 -new -nodes -key ca.key -sha256 -days 3650 \
    -out ca.crt -subj "/CN=RSC CA"

# Generate server certificate
SERVER_IP=$(hostname -I | awk '{print $1}')
SERVER_NAME=$(hostname -f)

sudo openssl genrsa -out server.key 4096
sudo openssl req -new -key server.key -out server.csr \
    -subj "/CN=${SERVER_NAME}"

# Create SAN extension file
cat << EOF | sudo tee server.ext
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyEncipherment
subjectAltName = @alt_names

[alt_names]
DNS.1 = ${SERVER_NAME}
DNS.2 = localhost
IP.1 = ${SERVER_IP}
IP.2 = 127.0.0.1
EOF

sudo openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out server.crt -days 3650 -sha256 \
    -extfile server.ext

# Set permissions
sudo chown rsc-server:rsc-server *.key *.crt
sudo chmod 600 *.key
sudo chmod 644 *.crt

# Cleanup
sudo rm -f server.csr server.ext
```

### 3. Configure Firewall

```bash
# firewalld (default on Fedora/Rocky/RHEL)
sudo firewall-cmd --permanent --add-port=8443/tcp
sudo firewall-cmd --reload

# Verify
sudo firewall-cmd --list-ports
```

### 4. Configure SELinux (if enforcing)

```bash
# Check SELinux status
getenforce

# If Enforcing, allow rsc-server to bind to network
sudo semanage port -a -t http_port_t -p tcp 8443

# Create custom policy for rsc-server (if needed)
sudo ausearch -c 'rsc-server' --raw | audit2allow -M rsc-server-policy
sudo semodule -i rsc-server-policy.pp
```

### 5. Configure systemd Service

Edit `/etc/systemd/system/rsc-server.service`:

```ini
[Unit]
Description=Remote Smartcard Server
After=network-online.target pcscd.service
Wants=network-online.target
Requires=pcscd.service

[Service]
Type=simple
User=rsc-server
Group=rsc-server
ExecStart=/usr/bin/rsc-server \
    --port 8443 \
    --bind 0.0.0.0 \
    --tls-cert /etc/rsc-server/certs/server.crt \
    --tls-key /etc/rsc-server/certs/server.key \
    --tls-ca /etc/rsc-server/certs/ca.crt \
    --log-level info
Restart=on-failure
RestartSec=5

# Security
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadOnlyPaths=/etc/rsc-server

[Install]
WantedBy=multi-user.target
```

### 6. Start Server

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now pcscd
sudo systemctl enable --now rsc-server

# Check status
sudo systemctl status rsc-server

# View logs
sudo journalctl -u rsc-server -f
```

## Client Setup

### 1. Create System User (if not using RPM)

```bash
sudo useradd -r -s /sbin/nologin -d /etc/rsc-client rsc-client
sudo usermod -a -G scard rsc-client  # For PC/SC access
sudo mkdir -p /etc/rsc-client/certs
sudo chown -R rsc-client:rsc-client /etc/rsc-client
sudo chmod 700 /etc/rsc-client/certs
```

### 2. Generate Client Certificate (on server)

```bash
# On the server
cd /etc/rsc-server/certs

CLIENT_NAME="my-client"

sudo openssl genrsa -out ${CLIENT_NAME}.key 4096
sudo openssl req -new -key ${CLIENT_NAME}.key -out ${CLIENT_NAME}.csr \
    -subj "/CN=${CLIENT_NAME}"
sudo openssl x509 -req -in ${CLIENT_NAME}.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out ${CLIENT_NAME}.crt -days 365 -sha256

# Copy to client (securely)
# scp ca.crt ${CLIENT_NAME}.crt ${CLIENT_NAME}.key client:/etc/rsc-client/certs/
```

### 3. Copy Certificates to Client

```bash
# On client
sudo scp server:/etc/rsc-server/certs/ca.crt /etc/rsc-client/certs/
sudo scp server:/etc/rsc-server/certs/my-client.crt /etc/rsc-client/certs/client.crt
sudo scp server:/etc/rsc-server/certs/my-client.key /etc/rsc-client/certs/client.key

sudo chown rsc-client:rsc-client /etc/rsc-client/certs/*
sudo chmod 600 /etc/rsc-client/certs/client.key
sudo chmod 644 /etc/rsc-client/certs/ca.crt /etc/rsc-client/certs/client.crt
```

### 4. Configure systemd Service

Edit `/etc/systemd/system/rsc-client.service`:

```ini
[Unit]
Description=Remote Smartcard Client
After=network-online.target pcscd.service
Wants=network-online.target
Requires=pcscd.service

[Service]
Type=simple
User=rsc-client
Group=rsc-client
ExecStart=/usr/bin/rsc-client \
    --server https://your-server.example.com:8443 \
    --tls-ca /etc/rsc-client/certs/ca.crt \
    --tls-cert /etc/rsc-client/certs/client.crt \
    --tls-key /etc/rsc-client/certs/client.key \
    --log-level info
Restart=on-failure
RestartSec=5

# Security
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadOnlyPaths=/etc/rsc-client
SupplementaryGroups=scard

[Install]
WantedBy=multi-user.target
```

### 5. Start Client

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now pcscd
sudo systemctl enable --now rsc-client

# Check status
sudo systemctl status rsc-client

# View logs
sudo journalctl -u rsc-client -f
```

## Verification

### On the Server

```bash
# Check for virtual reader
pcsc_scan

# Expected output:
# Reader 0: Virtual PCD (Yubico YubiKey)
#   Card state: Card inserted
#   ATR: 3B 8D 80 01 ...
```

### Test Smartcard Operations

```bash
# List slots
pkcs11-tool --module /usr/lib64/opensc-pkcs11.so --list-slots

# List objects on card
pkcs11-tool --module /usr/lib64/opensc-pkcs11.so --list-objects
```

## Troubleshooting

### SELinux Denials

```bash
# Check for SELinux denials
sudo ausearch -m AVC -ts recent

# Generate and apply policy
sudo ausearch -c 'rsc-server' --raw | audit2allow -M rsc-server
sudo semodule -i rsc-server.pp
```

### pcscd Issues

```bash
# Check pcscd status
sudo systemctl status pcscd

# Restart with debug
sudo systemctl stop pcscd
sudo pcscd -f -d

# Check for readers
pcsc_scan
```

### Firewall Issues

```bash
# Check if port is open
sudo firewall-cmd --list-all

# Test connectivity from client
nc -zv server 8443
```

### Service Won't Start

```bash
# Check logs
sudo journalctl -u rsc-server -n 50 --no-pager

# Run manually for debugging
sudo -u rsc-server /usr/bin/rsc-server \
    --port 8443 \
    --tls-cert /etc/rsc-server/certs/server.crt \
    --tls-key /etc/rsc-server/certs/server.key \
    --log-level debug
```

## Package Information

After installation from RPM:

```bash
# Server package contents
rpm -ql rsc-server

# Client package contents
rpm -ql rsc-client

# Tools package contents
rpm -ql rsc-tools

# Package info
rpm -qi rsc-server
```
