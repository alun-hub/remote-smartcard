# Remote Smartcard (rsc)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org/)

Use your local smartcard or Yubikey on remote servers as if it were physically connected.

## Overview

Remote Smartcard enables transparent smartcard access over the network. Applications on a remote server see a virtual smartcard reader with your card, allowing you to:

- **SSH authentication** with Yubikey from remote servers
- **Git commit signing** from cloud development environments
- **Browser client certificates** on remote desktops
- **VPN authentication** and code signing from anywhere

## Features

- **Transparent**: Applications see a standard PC/SC reader - no modifications needed
- **Secure**: Mutual TLS (mTLS) encryption, PIN never leaves your local machine
- **Reliable**: Automatic reconnection with exponential backoff
- **Efficient**: gRPC/protobuf for minimal overhead
- **Multi-reader**: Support for multiple smartcard readers
- **Cross-platform**: Linux server and client (Windows/macOS clients planned)

## Quick Start

### Server (where you want to use the smartcard)

```bash
# Install
curl -LO https://github.com/alun-hub/remote-smartcard/releases/latest/download/rsc-server-linux-amd64
chmod +x rsc-server-linux-amd64
sudo mv rsc-server-linux-amd64 /usr/local/bin/rsc-server

# Install vpcd (virtual smartcard reader)
sudo apt-get install vsmartcard-vpcd  # Debian/Ubuntu

# Generate certificates and start
mkdir ~/certs && cd ~/certs
openssl req -x509 -newkey rsa:4096 -keyout server.key -out server.crt \
    -days 365 -nodes -subj "/CN=$(hostname)"

rsc-server --port 8443 --tls-cert server.crt --tls-key server.key
```

### Client (where your smartcard is connected)

```bash
# Install
curl -LO https://github.com/alun-hub/remote-smartcard/releases/latest/download/rsc-client-linux-amd64
chmod +x rsc-client-linux-amd64
sudo mv rsc-client-linux-amd64 /usr/local/bin/rsc-client

# Copy server certificate and connect
scp server:~/certs/server.crt ~/
rsc-client --server https://SERVER_IP:8443 --tls-ca ~/server.crt
```

### Verify

On the server:
```bash
pcsc_scan
# Reader 0: Virtual PCD (Yubico YubiKey)
#   Card state: Card inserted
#   ATR: 3B 8D 80 01 ...
```

## Documentation

- **[User Guide](docs/USER_GUIDE.md)** - Comprehensive installation, configuration, and usage examples
- **[Fedora/Rocky Setup](docs/FEDORA_ROCKY_SETUP.md)** - Specific guide for Fedora, Rocky Linux, RHEL, AlmaLinux
- **[Setup Guide](SETUP.md)** - Detailed step-by-step setup instructions
- **[Architecture](ARCHITECTURE.md)** - Technical architecture and design
- **[Development](DEVELOPMENT.md)** - Development guide and contribution information
- **[API Reference](docs/API.md)** - gRPC protocol documentation
- **[Troubleshooting](docs/TROUBLESHOOTING.md)** - Common issues and solutions

## Installation

### From Binary Release

Download from [Releases](https://github.com/alun-hub/remote-smartcard/releases):

```bash
# Server
tar xzf rsc-server-linux-amd64.tar.gz
sudo mv rsc-server /usr/local/bin/

# Client
tar xzf rsc-client-linux-amd64.tar.gz
sudo mv rsc-client /usr/local/bin/
```

### From Package

**Debian/Ubuntu:**
```bash
sudo dpkg -i rsc-server_0.1.0_amd64.deb  # Server
sudo dpkg -i rsc-client_0.1.0_amd64.deb  # Client
```

**RHEL/Fedora:**
```bash
sudo rpm -i rsc-server-0.1.0-1.x86_64.rpm  # Server
sudo rpm -i rsc-client-0.1.0-1.x86_64.rpm  # Client
```

### From Source

```bash
# Prerequisites
sudo apt-get install protobuf-compiler libpcsclite-dev

# Build
git clone https://github.com/alun-hub/remote-smartcard.git
cd remote-smartcard
cargo build --release

# Install
sudo cp target/release/rsc-server /usr/local/bin/
sudo cp target/release/rsc-client /usr/local/bin/
```

## Usage Examples

### SSH with Yubikey

```bash
# On remote server (with rsc-server running)
ssh -I /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so user@target-host
# PIN prompt appears on your LOCAL machine!
```

### Git Signing

```bash
# Configure git on remote server
git config --global user.signingkey YOUR_KEY_ID
git config --global commit.gpgsign true

# Commits are now signed with your local Yubikey
git commit -m "Signed from remote"
```

### Running as Service

```bash
# Server
sudo systemctl enable rsc-server
sudo systemctl start rsc-server

# Client
sudo systemctl enable rsc-client
sudo systemctl start rsc-client
```

## Architecture

```
┌─────────────┐          TLS/gRPC          ┌─────────────┐
│   Client    │◄────────────────────────────►│   Server    │
│             │                              │             │
│  Yubikey ───┤                              ├─── vpcd ────┤
│    pcscd    │                              │    pcscd    │
└─────────────┘                              └─────────────┘
                                                    │
                                              Applications
                                             (SSH, GPG, etc)
```

## Configuration

### Server Options

| Option | Default | Description |
|--------|---------|-------------|
| `--port` | 8443 | Port to listen on |
| `--bind` | 0.0.0.0 | Address to bind to |
| `--tls-cert` | - | Server certificate (PEM) |
| `--tls-key` | - | Server private key (PEM) |
| `--tls-ca` | - | CA for client verification |
| `--log-level` | info | Log level (error/warn/info/debug/trace) |

### Client Options

| Option | Default | Description |
|--------|---------|-------------|
| `--server` | http://127.0.0.1:8443 | Server URL (use https:// for TLS) |
| `--tls-ca` | - | CA certificate for server verification |
| `--tls-cert` | - | Client certificate for mTLS |
| `--tls-key` | - | Client private key for mTLS |
| `--reconnect-delay` | 1 | Initial reconnect delay (seconds) |
| `--reconnect-max-delay` | 60 | Max reconnect delay (seconds) |
| `--no-reconnect` | false | Disable automatic reconnection |

## Security

- **TLS 1.3** with strong cipher suites
- **Mutual TLS (mTLS)** - both client and server authenticate
- **PIN never transmitted** - entered locally, only encrypted APDUs sent
- **Session isolation** - each client gets separate session

See [Security Documentation](docs/USER_GUIDE.md#security-best-practices) for best practices.

## Supported Smartcards

Any PC/SC compatible smartcard:
- Yubikey (all models with smartcard functionality)
- OpenPGP cards
- PIV cards
- JavaCards
- Most bank/government ID cards

## Roadmap

- [x] Basic APDU forwarding
- [x] TLS/mTLS security
- [x] Automatic reconnection
- [x] vpcd integration
- [x] Systemd integration
- [ ] Windows client
- [ ] macOS client
- [ ] Web UI for monitoring
- [ ] Multiple simultaneous clients per reader

## Contributing

Contributions are welcome! See [DEVELOPMENT.md](DEVELOPMENT.md) for guidelines.

```bash
# Setup development environment
git clone https://github.com/alun-hub/remote-smartcard.git
cd remote-smartcard
cargo build
cargo test
```

## License

MIT License - see [LICENSE-MIT](LICENSE-MIT)

## Acknowledgments

Built with:
- [pcsc-lite](https://pcsclite.apdu.fr/) - PC/SC implementation
- [vsmartcard](https://frankmorgner.github.io/vsmartcard/) - Virtual smartcard
- [tonic](https://github.com/hyperium/tonic) - gRPC for Rust
- [tokio](https://tokio.rs/) - Async runtime

## Support

- **Issues**: [GitHub Issues](https://github.com/alun-hub/remote-smartcard/issues)
- **Discussions**: [GitHub Discussions](https://github.com/alun-hub/remote-smartcard/discussions)
