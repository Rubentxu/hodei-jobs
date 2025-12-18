#!/bin/bash
#
# Certificate Generation Script for Hodei Platform
# This script creates a complete PKI hierarchy and worker certificates
#
# Usage: ./generate-certificates.sh <output_dir>
# Example: ./generate-certificates.sh ./certs
#
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
OUTPUT_DIR="${1:-./certs}"
CA_DIR="${OUTPUT_DIR}/ca"
WORKER_DIR="${OUTPUT_DIR}/workers"
SERVER_DIR="${OUTPUT_DIR}/server"

# Certificate validity periods (in days)
ROOT_CA_DAYS=3650    # 10 years
INTERMEDIATE_CA_DAYS=1095  # 3 years
WORKER_CERT_DAYS=90   # 90 days
SERVER_CERT_DAYS=365  # 1 year

# Key sizes
RSA_KEY_SIZE=4096
RSA_WORKER_KEY_SIZE=2048

echo -e "${GREEN}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║     Hodei Platform PKI Certificate Generation                ║${NC}"
echo -e "${GREEN}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Create directory structure
echo -e "${YELLOW}[1/6] Creating directory structure...${NC}"
mkdir -p "${CA_DIR}"/{root,intermediate,csr,newcerts}
mkdir -p "${WORKER_DIR}"
mkdir -p "${SERVER_DIR}"

# Initialize CA database
touch "${CA_DIR}/root/index.txt"
echo "01" > "${CA_DIR}/root/serial"
touch "${CA_DIR}/intermediate/index.txt"
echo "01" > "${CA_DIR}/intermediate/serial"

# Create OpenSSL configuration files
echo -e "${YELLOW}[2/6] Creating OpenSSL configuration files...${NC}"

# Root CA config
cat > "${CA_DIR}/root/openssl-root-ca.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_ca
prompt = no

[req_distinguished_name]
C = US
ST = California
L = San Francisco
O = Hodei Platform
OU = Security
CN = Hodei Platform Root CA

[v3_ca]
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always,issuer
basicConstraints = critical,CA:true
keyUsage = critical, digitalSignature, cRLSign, keyCertSign
EOF

# Intermediate CA config
cat > "${CA_DIR}/intermediate/openssl-intermediate-ca.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_ca
prompt = no

[req_distinguished_name]
C = US
ST = California
L = San Francisco
O = Hodei Platform
OU = Security
CN = Hodei Platform Intermediate CA

[v3_ca]
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always,issuer
basicConstraints = critical,CA:true,pathlen:0
keyUsage = critical, digitalSignature, cRLSign, keyCertSign
crlDistributionPoints = URI:http://crl.hodei-platform.local/intermediate.crl
authorityInfoAccess = OCSP;URI:http://ocsp.hodei-platform.local/

[v3_intermediate_ca]
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always,issuer
basicConstraints = critical,CA:true,pathlen:0
keyUsage = critical, digitalSignature, cRLSign, keyCertSign

[usr_cert]
basicConstraints = CA:FALSE
nsComment = "Hodei Platform Certificate"
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid,issuer
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth
crlDistributionPoints = URI:http://crl.hodei-platform.local/intermediate.crl
authorityInfoAccess = OCSP;URI:http://ocsp.hodei-platform.local/

EOF

# Worker certificate config template (used for each worker)
cat > "${CA_DIR}/worker-cert-template.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = US
ST = California
L = San Francisco
O = Hodei Platform
OU = Workers
CN = WORKER_ID_PLACEHOLDER

[v3_req]
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
subjectAltName = @alt_names

[alt_names]
URI.1 = urn:worker:WORKER_ID_PLACEHOLDER
EOF

echo -e "${GREEN}✓ Directory structure and configs created${NC}"
echo ""

# Generate Root CA
echo -e "${YELLOW}[3/6] Generating Root CA (10 years)...${NC}"
openssl genrsa -out "${CA_DIR}/root/ca-key.pem" ${RSA_KEY_SIZE}
openssl req -new -x509 -days ${ROOT_CA_DAYS} -key "${CA_DIR}/root/ca-key.pem" \
    -out "${CA_DIR}/root/ca-cert.pem" -config "${CA_DIR}/root/openssl-root-ca.cnf"

echo -e "${GREEN}✓ Root CA generated${NC}"
echo -e "   ${GREEN}✓${NC} Private key: ${CA_DIR}/root/ca-key.pem"
echo -e "   ${GREEN}✓${NC} Certificate: ${CA_DIR}/root/ca-cert.pem"
echo ""

# Generate Intermediate CA
echo -e "${YELLOW}[4/6] Generating Intermediate CA (3 years)...${NC}"
openssl genrsa -out "${CA_DIR}/intermediate/intermediate-key.pem" ${RSA_KEY_SIZE}
openssl req -new -key "${CA_DIR}/intermediate/intermediate-key.pem" \
    -out "${CA_DIR}/intermediate/intermediate.csr" \
    -config "${CA_DIR}/intermediate/openssl-intermediate-ca.cnf"
openssl ca -batch -days ${INTERMEDIATE_CA_DAYS} -config "${CA_DIR}/root/openssl-root-ca.cnf" \
    -in "${CA_DIR}/intermediate/intermediate.csr" \
    -out "${CA_DIR}/intermediate/intermediate-cert.pem"

echo -e "${GREEN}✓ Intermediate CA generated${NC}"
echo -e "   ${GREEN}✓${NC} Private key: ${CA_DIR}/intermediate/intermediate-key.pem"
echo -e "   ${GREEN}✓${NC} Certificate: ${CA_DIR}/intermediate/intermediate-cert.pem"
echo ""

# Create CA chain (root + intermediate)
echo -e "${YELLOW}[5/6] Creating CA certificate chain...${NC}"
cat "${CA_DIR}/intermediate/intermediate-cert.pem" "${CA_DIR}/root/ca-cert.pem" > \
    "${CA_DIR}/ca-chain.pem"
echo -e "${GREEN}✓ CA chain created: ${CA_DIR}/ca-chain.pem${NC}"
echo ""

# Generate a sample worker certificate
echo -e "${YELLOW}[6/6] Generating sample worker certificate...${NC}"
WORKER_ID="${WORKER_ID:-worker-$(uuidgen | cut -d'-' -f1)}"
echo "Worker ID: ${WORKER_ID}"

# Create worker config from template
sed "s/WORKER_ID_PLACEHOLDER/${WORKER_ID}/g" \
    "${CA_DIR}/worker-cert-template.cnf" > "${CA_DIR}/worker-${WORKER_ID}.cnf"

# Generate worker key and CSR
openssl genrsa -out "${WORKER_DIR}/${WORKER_ID}-key.pem" ${RSA_WORKER_KEY_SIZE}
openssl req -new -key "${WORKER_DIR}/${WORKER_ID}-key.pem" \
    -out "${CA_DIR}/csr/${WORKER_ID}.csr" \
    -config "${CA_DIR}/worker-${WORKER_ID}.cnf"

# Sign worker certificate
openssl ca -batch -days ${WORKER_CERT_DAYS} \
    -config "${CA_DIR}/intermediate/openssl-intermediate-ca.cnf" \
    -in "${CA_DIR}/csr/${WORKER_ID}.csr" \
    -out "${WORKER_DIR}/${WORKER_ID}-cert.pem"

echo -e "${GREEN}✓ Sample worker certificate generated${NC}"
echo -e "   ${GREEN}✓${NC} Worker ID: ${WORKER_ID}"
echo -e "   ${GREEN}✓${NC} Private key: ${WORKER_DIR}/${WORKER_ID}-key.pem"
echo -e "   ${GREEN}✓${NC} Certificate: ${WORKER_DIR}/${WORKER_ID}-cert.pem"
echo ""

# Generate server certificate
echo -e "${YELLOW}[Bonus] Generating server certificate...${NC}"
SERVER_NAME="${SERVER_NAME:-api.hodei-platform.local}"
cat > "${SERVER_DIR}/openssl-server.cnf" <<EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = US
ST = California
L = San Francisco
O = Hodei Platform
OU = Platform
CN = ${SERVER_NAME}

[v3_req]
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = ${SERVER_NAME}
DNS.2 = *.hodei-platform.local
EOF

openssl genrsa -out "${SERVER_DIR}/server-key.pem" ${RSA_KEY_SIZE}
openssl req -new -key "${SERVER_DIR}/server-key.pem" \
    -out "${SERVER_DIR}/server.csr" \
    -config "${SERVER_DIR}/openssl-server.cnf"
openssl ca -batch -days ${SERVER_CERT_DAYS} \
    -config "${CA_DIR}/intermediate/openssl-intermediate-ca.cnf" \
    -in "${SERVER_DIR}/server.csr" \
    -out "${SERVER_DIR}/server-cert.pem"

# Create server cert chain
cat "${SERVER_DIR}/server-cert.pem" "${CA_DIR}/intermediate/intermediate-cert.pem" \
    "${CA_DIR}/root/ca-cert.pem" > "${SERVER_DIR}/server-chain.pem"

echo -e "${GREEN}✓ Server certificate generated${NC}"
echo -e "   ${GREEN}✓${NC} Server name: ${SERVER_NAME}"
echo -e "   ${GREEN}✓${NC} Private key: ${SERVER_DIR}/server-key.pem"
echo -e "   ${GREEN}✓${NC} Certificate: ${SERVER_DIR}/server-cert.pem"
echo ""

# Create worker installation package
echo -e "${YELLOW}Creating worker installation package...${NC}"
WORKER_PKG="${OUTPUT_DIR}/worker-${WORKER_ID}.tar.gz"
tar -czf "${WORKER_PKG}" \
    -C "${WORKER_DIR}" \
    "${WORKER_ID}-key.pem" \
    "${WORKER_ID}-cert.pem" \
    -C "${CA_DIR}" \
    "ca-chain.pem"

echo -e "${GREEN}✓ Worker package created: ${WORKER_PKG}${NC}"
echo ""

# Print summary
echo -e "${GREEN}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                   Certificate Generation Complete              ║${NC}"
echo -e "${GREEN}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${GREEN}📁 Output Directory:${NC} ${OUTPUT_DIR}"
echo ""
echo -e "${YELLOW}CA Certificates:${NC}"
echo "  • Root CA:           ${CA_DIR}/root/ca-cert.pem"
echo "  • Intermediate CA:   ${CA_DIR}/intermediate/intermediate-cert.pem"
echo "  • CA Chain:          ${CA_DIR}/ca-chain.pem"
echo ""
echo -e "${YELLOW}Worker Certificates:${NC}"
echo "  • Sample Worker:     ${WORKER_ID}"
echo "  • Private Key:       ${WORKER_DIR}/${WORKER_ID}-key.pem"
echo "  • Certificate:       ${WORKER_DIR}/${WORKER_ID}-cert.pem"
echo "  • Installation Pkg:  ${WORKER_PKG}"
echo ""
echo -e "${YELLOW}Server Certificates:${NC}"
echo "  • Server Name:       ${SERVER_NAME}"
echo "  • Private Key:       ${SERVER_DIR}/server-key.pem"
echo "  • Certificate:       ${SERVER_DIR}/server-cert.pem"
echo "  • Cert Chain:        ${SERVER_DIR}/server-chain.pem"
echo ""
echo -e "${YELLOW}Certificate Validity:${NC}"
echo "  • Root CA:           ${ROOT_CA_DAYS} days (10 years)"
echo "  • Intermediate CA:   ${INTERMEDIATE_CA_DAYS} days (3 years)"
echo "  • Worker Cert:       ${WORKER_CERT_DAYS} days (90 days)"
echo "  • Server Cert:       ${SERVER_CERT_DAYS} days (1 year)"
echo ""
echo -e "${YELLOW}🔒 Security Reminders:${NC}"
echo "  1. Keep private keys secure (0600 permissions)"
echo "  2. Store Root CA private key in secure location (HSM recommended)"
echo "  3. Distribute worker packages via secure channel"
echo "  4. Rotate worker certificates every 60 days"
echo "  5. Revoke compromised certificates immediately"
echo ""
echo -e "${GREEN}✅ PKI hierarchy created successfully!${NC}"
