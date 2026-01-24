#!/bin/bash
#
# Build release binaries for Remote Smartcard
#

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

# Get version from Cargo.toml
VERSION=$(grep -m1 'version = ' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
log_info "Building Remote Smartcard v${VERSION}"

# Build release
log_info "Building release binaries..."
cargo build --release --workspace

# Create output directory
OUTPUT_DIR="release-${VERSION}"
mkdir -p "${OUTPUT_DIR}"

log_info "Copying binaries..."
cp target/release/rsc-server "${OUTPUT_DIR}/"
cp target/release/rsc-client "${OUTPUT_DIR}/"
cp target/release/rsc-keygen "${OUTPUT_DIR}/"

log_info "Copying support files..."
cp -r systemd "${OUTPUT_DIR}/"
cp -r scripts "${OUTPUT_DIR}/"
cp README.md "${OUTPUT_DIR}/"
cp CHANGELOG.md "${OUTPUT_DIR}/"
cp LICENSE-MIT "${OUTPUT_DIR}/"

# Create documentation bundle
mkdir -p "${OUTPUT_DIR}/docs"
cp docs/*.md "${OUTPUT_DIR}/docs/"
cp SETUP.md "${OUTPUT_DIR}/docs/"
cp ARCHITECTURE.md "${OUTPUT_DIR}/docs/"
cp DEVELOPMENT.md "${OUTPUT_DIR}/docs/"

# Create tarballs
log_info "Creating tarballs..."

# Server package
tar czf "rsc-server-${VERSION}-linux-amd64.tar.gz" \
    -C "${OUTPUT_DIR}" \
    rsc-server \
    rsc-keygen \
    systemd/rsc-server.service \
    scripts/install-server.sh \
    scripts/generate-client-cert.sh \
    README.md \
    CHANGELOG.md \
    LICENSE-MIT \
    docs/

# Client package
tar czf "rsc-client-${VERSION}-linux-amd64.tar.gz" \
    -C "${OUTPUT_DIR}" \
    rsc-client \
    systemd/rsc-client.service \
    scripts/install-client.sh \
    README.md \
    CHANGELOG.md \
    LICENSE-MIT \
    docs/

# Full package
tar czf "remote-smartcard-${VERSION}-linux-amd64.tar.gz" \
    -C "${OUTPUT_DIR}" \
    .

log_info "Build complete!"
echo ""
echo "Packages created:"
ls -lh *.tar.gz
echo ""
echo "Release directory: ${OUTPUT_DIR}/"
ls -la "${OUTPUT_DIR}/"
