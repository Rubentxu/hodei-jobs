#!/bin/bash
# Backup script for Hodei Job Platform
# This script creates backups of PostgreSQL database
# Note: Redis is no longer used (replaced by NATS JetStream for event streaming)

set -e

# Configuration
NAMESPACE="${NAMESPACE:-hodei-jobs}"
RELEASE_NAME="${RELEASE_NAME:-hodei-jobs-platform}"
BACKUP_DIR="${BACKUP_DIR:-/tmp/hodei-backup}"
S3_BUCKET="${S3_BUCKET:-hodei-backups}"
RETENTION_DAYS="${RETENTION_DAYS:-30}"

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

# Create backup directory
mkdir -p "${BACKUP_DIR}"

log "Starting backup process..."

# Backup PostgreSQL
log "Backing up PostgreSQL database..."
if kubectl get deployment -n "${NAMESPACE}" "${RELEASE_NAME}-postgresql" > /dev/null 2>&1; then
    POSTGRES_POD=$(kubectl get pods -n "${NAMESPACE}" -l app.kubernetes.io/name="${RELEASE_NAME}-postgresql" -o jsonpath='{.items[0].metadata.name}')
    TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    BACKUP_FILE="${BACKUP_DIR}/postgres_${TIMESTAMP}.sql"

    kubectl exec -n "${NAMESPACE}" "${POSTGRES_POD}" -- pg_dump -U postgres -d hodei_jobs > "${BACKUP_FILE}"

    if [ -f "${BACKUP_FILE}" ]; then
        log "PostgreSQL backup created: ${BACKUP_FILE}"

        # Compress backup
        gzip "${BACKUP_FILE}"
        log "PostgreSQL backup compressed"
    else
        error "Failed to create PostgreSQL backup"
        exit 1
    fi
else
    warn "PostgreSQL deployment not found, skipping database backup"
fi

# Upload to S3 (if configured)
if [ -n "${S3_BUCKET}" ]; then
    log "Uploading backups to S3 bucket: ${S3_BUCKET}"

    for file in "${BACKUP_DIR}"/*.gz; do
        if [ -f "$file" ]; then
            aws s3 cp "$file" "s3://${S3_BUCKET}/hodei-backups/$(basename "$file")"
            log "Uploaded: $file"
        fi
    done
else
    warn "S3_BUCKET not configured, skipping cloud upload"
fi

# Cleanup old local backups
log "Cleaning up backups older than ${RETENTION_DAYS} days..."
find "${BACKUP_DIR}" -type f -name "*.gz" -mtime +${RETENTION_DAYS} -delete

# Cleanup old S3 backups
if [ -n "${S3_BUCKET}" ]; then
    log "Cleaning up old S3 backups..."
    cutoff_date=$(date -d "${RETENTION_DAYS} days ago" +%s)
    aws s3 ls "s3://${S3_BUCKET}/hodei-backups/" | while read -r line; do
        file_date=$(echo "$line" | awk '{print $1 " " $2}')
        file_timestamp=$(date -d "$file_date" +%s 2>/dev/null || echo 0)
        if [ "$file_timestamp" -lt "$cutoff_date" ]; then
            filename=$(echo "$line" | awk '{print $4}')
            aws s3 rm "s3://${S3_BUCKET}/hodei-backups/${filename}"
            log "Deleted old S3 backup: ${filename}"
        fi
    done
fi

log "Backup process completed successfully!"

# Display backup summary
log "Backup summary:"
ls -lh "${BACKUP_DIR}"/*.gz 2>/dev/null || echo "No backup files found"
