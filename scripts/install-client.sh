#!/bin/bash
#
# Remote Smartcard Client Installation Script
# This script installs rsc-client on a Linux system
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
RSC_USER="rsc-client"
RSC_GROUP="rsc-client"
CONFIG_DIR="/etc/rsc-client"
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
            ;;
        redhat)
            dnf install -y pcsc-lite pcsc-lite-ccid opensc
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
        useradd --system --no-create-home --shell /usr/sbin/nologin --groups scard "${RSC_USER}" 2>/dev/null || \
        useradd --system --no-create-home --shell /usr/sbin/nologin "${RSC_USER}"
        log_info "Created user ${RSC_USER}"
    fi

    # Add to scard group for PC/SC access
    usermod -a -G scard "${RSC_USER}" 2>/dev/null || true
}

create_directories() {
    log_info "Creating directories..."

    mkdir -p "${CONFIG_DIR}"
    mkdir -p "${CERT_DIR}"

    chown -R "${RSC_USER}:${RSC_GROUP}" "${CONFIG_DIR}"
    chmod 750 "${CONFIG_DIR}"
    chmod 700 "${CERT_DIR}"
}

install_binary() {
    log_info "Installing binary..."

    if [ -f "./target/release/rsc-client" ]; then
        cp "./target/release/rsc-client" "${BIN_DIR}/"
    elif [ -f "./rsc-client" ]; then
        cp "./rsc-client" "${BIN_DIR}/"
    else
        log_error "Cannot find rsc-client binary. Build with 'cargo build --release' first."
        exit 1
    fi

    chmod 755 "${BIN_DIR}/rsc-client"
    log_info "Installed rsc-client to ${BIN_DIR}"
}

install_systemd_service() {
    log_info "Installing systemd service..."

    if [ -f "./systemd/rsc-client.service" ]; then
        cp "./systemd/rsc-client.service" "${SYSTEMD_DIR}/"
    else
        log_error "Cannot find systemd service file"
        exit 1
    fi

    systemctl daemon-reload
    log_info "Installed systemd service"
}

print_next_steps() {
    echo ""
    echo "=========================================="
    echo " Installation Complete!"
    echo "=========================================="
    echo ""
    echo "Next steps:"
    echo ""
    echo "1. Copy certificates from server:"
    echo "   scp server:${CERT_DIR}/ca.crt ${CERT_DIR}/"
    echo "   scp server:/tmp/client.crt ${CERT_DIR}/"
    echo "   scp server:/tmp/client.key ${CERT_DIR}/"
    echo ""
    echo "2. Set correct permissions:"
    echo "   sudo chown ${RSC_USER}:${RSC_GROUP} ${CERT_DIR}/*"
    echo "   sudo chmod 600 ${CERT_DIR}/client.key"
    echo "   sudo chmod 644 ${CERT_DIR}/ca.crt ${CERT_DIR}/client.crt"
    echo ""
    echo "3. Edit the systemd service file with your server address:"
    echo "   sudo nano ${SYSTEMD_DIR}/rsc-client.service"
    echo "   Change: --server https://your-server.example.com:8443"
    echo ""
    echo "4. Verify smartcard is accessible:"
    echo "   pcsc_scan"
    echo ""
    echo "5. Start the service:"
    echo "   sudo systemctl enable rsc-client"
    echo "   sudo systemctl start rsc-client"
    echo ""
    echo "6. Check status:"
    echo "   sudo systemctl status rsc-client"
    echo "   sudo journalctl -u rsc-client -f"
    echo ""
}

# Main
main() {
    log_info "Remote Smartcard Client Installation"
    echo ""

    check_root
    detect_distro
    install_dependencies
    create_user
    create_directories
    install_binary
    install_systemd_service
    print_next_steps
}

main "$@"
