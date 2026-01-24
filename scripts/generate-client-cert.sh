#!/bin/bash
#
# Generate Client Certificate for Remote Smartcard
# Run this on the server where the CA is located
#

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Default paths
CA_DIR="/etc/rsc-server/certs"
OUTPUT_DIR="/tmp"

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  -n, --name NAME      Client name/CN (required)"
    echo "  -c, --ca-dir DIR     CA directory (default: ${CA_DIR})"
    echo "  -o, --output DIR     Output directory (default: ${OUTPUT_DIR})"
    echo "  -d, --days DAYS      Certificate validity in days (default: 365)"
    echo "  -h, --help           Show this help"
    echo ""
    echo "Example:"
    echo "  $0 --name laptop-home"
    echo "  $0 --name workstation --days 730"
}

# Parse arguments
CLIENT_NAME=""
DAYS=365

while [[ $# -gt 0 ]]; do
    case $1 in
        -n|--name)
            CLIENT_NAME="$2"
            shift 2
            ;;
        -c|--ca-dir)
            CA_DIR="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -d|--days)
            DAYS="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Validate
if [ -z "${CLIENT_NAME}" ]; then
    log_error "Client name is required"
    usage
    exit 1
fi

if [ ! -f "${CA_DIR}/ca.crt" ] || [ ! -f "${CA_DIR}/ca.key" ]; then
    log_error "CA certificate or key not found in ${CA_DIR}"
    exit 1
fi

# Generate certificate
log_info "Generating certificate for client: ${CLIENT_NAME}"

# Generate private key
openssl genrsa -out "${OUTPUT_DIR}/${CLIENT_NAME}.key" 4096
log_info "Generated private key: ${OUTPUT_DIR}/${CLIENT_NAME}.key"

# Generate CSR
openssl req -new \
    -key "${OUTPUT_DIR}/${CLIENT_NAME}.key" \
    -out "${OUTPUT_DIR}/${CLIENT_NAME}.csr" \
    -subj "/C=SE/O=RemoteSmartcard/CN=${CLIENT_NAME}"

# Sign with CA
openssl x509 -req \
    -in "${OUTPUT_DIR}/${CLIENT_NAME}.csr" \
    -CA "${CA_DIR}/ca.crt" \
    -CAkey "${CA_DIR}/ca.key" \
    -CAcreateserial \
    -out "${OUTPUT_DIR}/${CLIENT_NAME}.crt" \
    -days "${DAYS}" \
    -sha256

# Cleanup CSR
rm -f "${OUTPUT_DIR}/${CLIENT_NAME}.csr"

# Set permissions
chmod 600 "${OUTPUT_DIR}/${CLIENT_NAME}.key"
chmod 644 "${OUTPUT_DIR}/${CLIENT_NAME}.crt"

log_info "Generated certificate: ${OUTPUT_DIR}/${CLIENT_NAME}.crt"

echo ""
echo "=========================================="
echo " Certificate Generated Successfully!"
echo "=========================================="
echo ""
echo "Files created:"
echo "  - ${OUTPUT_DIR}/${CLIENT_NAME}.key (private key)"
echo "  - ${OUTPUT_DIR}/${CLIENT_NAME}.crt (certificate)"
echo ""
echo "Copy these files to the client:"
echo "  scp ${OUTPUT_DIR}/${CLIENT_NAME}.key client:/etc/rsc-client/certs/client.key"
echo "  scp ${OUTPUT_DIR}/${CLIENT_NAME}.crt client:/etc/rsc-client/certs/client.crt"
echo "  scp ${CA_DIR}/ca.crt client:/etc/rsc-client/certs/ca.crt"
echo ""
echo "Then on the client, set permissions:"
echo "  sudo chown rsc-client:rsc-client /etc/rsc-client/certs/*"
echo "  sudo chmod 600 /etc/rsc-client/certs/client.key"
echo ""
