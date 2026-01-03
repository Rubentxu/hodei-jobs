#!/bin/bash
# Restore script for Hodei Job Platform
# This script restores backups of PostgreSQL database
# Note: Redis is no longer used (replaced by NATS JetStream for event streaming)

set -e

# Configuration
NAMESPACE="${NAMESPACE:-hodei-jobs}"
RELEASE_NAME="${RELEASE_NAME:-hodei-jobs-platform}"
BACKUP_FILE="${BACKUP_FILE:-}"
S3_BUCKET="${S3_BUCKET:-hodei-backups}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  -f, --file BACKUP_FILE    Local backup file to restore (required if not using S3)"
    echo "  -b, --bucket S3_BUCKET    S3 bucket name (optional, uses S3_BUCKET env var by default)"
    echo "  -n, --namespace NAMESPACE Kubernetes namespace (default: hodei-jobs)"
    echo "  -r, --release RELEASE     Helm release name (default: hodei-jobs-platform)"
    echo "  -h, --help                Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 --file /tmp/postgres_20231215_120000.sql.gz"
    echo "  $0 --bucket my-backup-bucket"
    echo "  $0 -b my-backup-bucket -n production -r hodei-prod"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -f|--file)
            BACKUP_FILE="$2"
            shift 2
            ;;
        -b|--bucket)
            S3_BUCKET="$2"
            shift 2
            ;;
        -n|--namespace)
            NAMESPACE="$2"
            shift 2
            ;;
        -r|--release)
            RELEASE_NAME="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Check if backup file is provided
if [ -z "$BACKUP_FILE" ] && [ -z "$S3_BUCKET" ]; then
    error "Either BACKUP_FILE or S3_BUCKET must be specified"
    usage
    exit 1
fi

# Download from S3 if bucket is specified
if [ -n "$S3_BUCKET" ] && [ -z "$BACKUP_FILE" ]; then
    log "Fetching backup files from S3 bucket: ${S3_BUCKET}"

    # List available backups
    log "Available backups:"
    aws s3 ls "s3://${S3_BUCKET}/hodei-backups/" | tail -10

    echo ""
    read -p "Enter the backup filename to restore: " backup_filename

    if [ -z "$backup_filename" ]; then
        error "Backup filename is required"
        exit 1
    fi

    BACKUP_FILE="/tmp/${backup_filename}"
    log "Downloading backup from S3..."
    aws s3 cp "s3://${S3_BUCKET}/hodei-backups/${backup_filename}" "${BACKUP_FILE}"

    if [ ! -f "${BACKUP_FILE}" ]; then
        error "Failed to download backup file"
        exit 1
    fi

    log "Backup file downloaded: ${BACKUP_FILE}"
fi

# Determine backup type
if [[ "$BACKUP_FILE" == *.gz ]]; then
    BACKUP_TYPE=$(basename "$BACKUP_FILE" | cut -d'_' -f1)
    TEMP_FILE=$(mktemp)
    gunzip -c "$BACKUP_FILE" > "$TEMP_FILE"
    BACKUP_FILE="$TEMP_FILE"
    log "Backup file decompressed"
elif [[ "$BACKUP_FILE" == *.sql ]]; then
    BACKUP_TYPE="postgres"
else
    error "Unknown backup file format. Expected .sql or .gz"
    exit 1
fi

log "Starting restore process for ${BACKUP_TYPE}..."

# Restore PostgreSQL
if [[ "$BACKUP_TYPE" == "postgres" ]]; then
    log "Restoring PostgreSQL database..."

    if ! kubectl get deployment -n "${NAMESPACE}" "${RELEASE_NAME}-postgresql" > /dev/null 2>&1; then
        error "PostgreSQL deployment not found in namespace ${NAMESPACE}"
        exit 1
    fi

    POSTGRES_POD=$(kubectl get pods -n "${NAMESPACE}" -l app.kubernetes.io/name="${RELEASE_NAME}-postgresql" -o jsonpath='{.items[0].metadata.name}')

    warn "This will overwrite the existing database. Are you sure? (yes/no)"
    read -r confirmation
    if [ "$confirmation" != "yes" ]; then
        log "Restore cancelled by user"
        exit 0
    fi

    log "Dropping existing database..."
    kubectl exec -n "${NAMESPACE}" "${POSTGRES_POD}" -- psql -U postgres -c "DROP DATABASE IF EXISTS hodei_jobs;" || true

    log "Creating new database..."
    kubectl exec -n "${NAMESPACE}" "${POSTGRES_POD}" -- psql -U postgres -c "CREATE DATABASE hodei_jobs;"

    log "Restoring database from backup..."
    kubectl exec -n "${NAMESPACE}" "${POSTGRES_POD}" -- psql -U postgres -d hodei_jobs < "$BACKUP_FILE"

    log "PostgreSQL restore completed successfully"

else
    error "Unknown backup type: ${BACKUP_TYPE}"
    exit 1
fi

# Cleanup temporary file if created
if [ -n "$TEMP_FILE" ]; then
    rm -f "$TEMP_FILE"
fi

log "Restore process completed successfully!"
log "Please verify the application is working correctly."

# Check if pods are running
log "Checking pod status..."
kubectl get pods -n "${NAMESPACE}" -l app.kubernetes.io/name="${RELEASE_NAME}"

log "Restore operation completed!"
