#!/bin/bash
#
# Remote Smartcard Server Installation Script
# This script installs rsc-server on a Linux system
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
RSC_USER="rsc-server"
RSC_GROUP="rsc-server"
CONFIG_DIR="/etc/rsc-server"
CERT_DIR="${CONFIG_DIR}/certs"
BIN_DIR="/usr/local/bin"
SYSTEMD_DIR="/etc/systemd/system"

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root"
        exit 1
    fi
}

detect_distro() {
    if [ -f /etc/debian_version ]; then
        DISTRO="debian"
    elif [ -f /etc/redhat-release ]; then
        DISTRO="redhat"
    elif [ -f /etc/arch-release ]; then
        DISTRO="arch"
    else
        DISTRO="unknown"
    fi
    log_info "Detected distribution: $DISTRO"
}

install_dependencies() {
    log_info "Installing dependencies..."

    case $DISTRO in
        debian)
            apt-get update
            apt-get install -y pcscd libpcsclite1 libccid opensc
            # Try to install vpcd if available
            apt-get install -y vsmartcard-vpcd 2>/dev/null || log_warn "vpcd not in repos, may need manual install"
            ;;
        redhat)
            dnf install -y pcsc-lite pcsc-lite-ccid opensc
            dnf install -y vsmartcard-vpcd 2>/dev/null || log_warn "vpcd not in repos, may need manual install"
            ;;
        arch)
            pacman -Sy --noconfirm pcsclite ccid opensc
            ;;
        *)
            log_warn "Unknown distribution, please install PC/SC dependencies manually"
            ;;
    esac
}

create_user() {
    log_info "Creating system user ${RSC_USER}..."

    if id "${RSC_USER}" &>/dev/null; then
        log_info "User ${RSC_USER} already exists"
    else
        useradd --system --no-create-home --shell /usr/sbin/nologin "${RSC_USER}"
        log_info "Created user ${RSC_USER}"
    fi
}

create_directories() {
    log_info "Creating directories..."

    mkdir -p "${CONFIG_DIR}"
    mkdir -p "${CERT_DIR}"
    mkdir -p "/var/run/rsc-server"

    chown -R "${RSC_USER}:${RSC_GROUP}" "${CONFIG_DIR}"
    chown -R "${RSC_USER}:${RSC_GROUP}" "/var/run/rsc-server"
    chmod 750 "${CONFIG_DIR}"
    chmod 700 "${CERT_DIR}"
}

install_binary() {
    log_info "Installing binary..."

    if [ -f "./target/release/rsc-server" ]; then
        cp "./target/release/rsc-server" "${BIN_DIR}/"
    elif [ -f "./rsc-server" ]; then
        cp "./rsc-server" "${BIN_DIR}/"
    else
        log_error "Cannot find rsc-server binary. Build with 'cargo build --release' first."
        exit 1
    fi

    chmod 755 "${BIN_DIR}/rsc-server"
    log_info "Installed rsc-server to ${BIN_DIR}"
}

install_systemd_service() {
    log_info "Installing systemd service..."

    if [ -f "./systemd/rsc-server.service" ]; then
        cp "./systemd/rsc-server.service" "${SYSTEMD_DIR}/"
    else
        log_error "Cannot find systemd service file"
        exit 1
    fi

    systemctl daemon-reload
    log_info "Installed systemd service"
}

generate_certificates() {
    log_info "Generating certificates..."

    if [ -f "${CERT_DIR}/ca.crt" ]; then
        log_warn "Certificates already exist, skipping generation"
        return
    fi

    # Generate CA
    openssl genrsa -out "${CERT_DIR}/ca.key" 4096
    openssl req -x509 -new -nodes \
        -key "${CERT_DIR}/ca.key" \
        -sha256 -days 3650 \
        -out "${CERT_DIR}/ca.crt" \
        -subj "/C=SE/O=RemoteSmartcard/CN=RSC CA"

    # Generate server certificate
    HOSTNAME=$(hostname -f)
    openssl genrsa -out "${CERT_DIR}/server.key" 4096
    openssl req -new \
        -key "${CERT_DIR}/server.key" \
        -out "${CERT_DIR}/server.csr" \
        -subj "/C=SE/O=RemoteSmartcard/CN=${HOSTNAME}"

    # Create extension file for SANs
    cat > "${CERT_DIR}/server.ext" << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
DNS.1 = ${HOSTNAME}
DNS.2 = localhost
IP.1 = 127.0.0.1
EOF

    openssl x509 -req \
        -in "${CERT_DIR}/server.csr" \
        -CA "${CERT_DIR}/ca.crt" \
        -CAkey "${CERT_DIR}/ca.key" \
        -CAcreateserial \
        -out "${CERT_DIR}/server.crt" \
        -days 3650 \
        -sha256 \
        -extfile "${CERT_DIR}/server.ext"

    # Cleanup
    rm -f "${CERT_DIR}/server.csr" "${CERT_DIR}/server.ext"

    # Set permissions
    chmod 600 "${CERT_DIR}/ca.key" "${CERT_DIR}/server.key"
    chmod 644 "${CERT_DIR}/ca.crt" "${CERT_DIR}/server.crt"
    chown -R "${RSC_USER}:${RSC_GROUP}" "${CERT_DIR}"

    log_info "Certificates generated in ${CERT_DIR}"
    log_info "IMPORTANT: Copy ${CERT_DIR}/ca.crt to clients for verification"
}

print_next_steps() {
    echo ""
    echo "=========================================="
    echo " Installation Complete!"
    echo "=========================================="
    echo ""
    echo "Next steps:"
    echo ""
    echo "1. Edit the systemd service file to match your setup:"
    echo "   sudo nano ${SYSTEMD_DIR}/rsc-server.service"
    echo ""
    echo "2. Generate client certificates:"
    echo "   See: scripts/generate-client-cert.sh"
    echo ""
    echo "3. Copy CA certificate to clients:"
    echo "   scp ${CERT_DIR}/ca.crt client:/etc/rsc-client/certs/"
    echo ""
    echo "4. Start the service:"
    echo "   sudo systemctl enable rsc-server"
    echo "   sudo systemctl start rsc-server"
    echo ""
    echo "5. Check status:"
    echo "   sudo systemctl status rsc-server"
    echo "   sudo journalctl -u rsc-server -f"
    echo ""
}

# Main
main() {
    log_info "Remote Smartcard Server Installation"
    echo ""

    check_root
    detect_distro
    install_dependencies
    create_user
    create_directories
    install_binary
    install_systemd_service
    generate_certificates
    print_next_steps
}

main "$@"
