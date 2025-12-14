#!/bin/bash
# Hodei Firecracker Rootfs Builder
# Creates a minimal Alpine Linux rootfs with Hodei worker agent pre-installed
#
# Usage: ./build-rootfs.sh [output_path]
# Default output: /var/lib/hodei/rootfs.ext4

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Configuration
ROOTFS_SIZE_MB="${ROOTFS_SIZE_MB:-512}"
ALPINE_VERSION="${ALPINE_VERSION:-3.19}"
ALPINE_MIRROR="${ALPINE_MIRROR:-https://dl-cdn.alpinelinux.org/alpine}"
OUTPUT_PATH="${1:-/var/lib/hodei/rootfs.ext4}"
WORK_DIR="${WORK_DIR:-/tmp/hodei-rootfs-build}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

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

check_dependencies() {
    local deps=("wget" "mkfs.ext4" "mount" "umount" "chroot")
    for dep in "${deps[@]}"; do
        if ! command -v "$dep" &> /dev/null; then
            log_error "Required command not found: $dep"
            exit 1
        fi
    done
}

cleanup() {
    log_info "Cleaning up..."
    if mountpoint -q "${WORK_DIR}/rootfs" 2>/dev/null; then
        umount "${WORK_DIR}/rootfs" || true
    fi
    if [[ -n "${LOOP_DEV}" ]] && [[ -e "${LOOP_DEV}" ]]; then
        losetup -d "${LOOP_DEV}" || true
    fi
}

trap cleanup EXIT

create_rootfs_image() {
    log_info "Creating rootfs image (${ROOTFS_SIZE_MB}MB)..."
    
    mkdir -p "${WORK_DIR}"
    rm -f "${WORK_DIR}/rootfs.ext4"
    
    # Create sparse file
    dd if=/dev/zero of="${WORK_DIR}/rootfs.ext4" bs=1M count=0 seek="${ROOTFS_SIZE_MB}" 2>/dev/null
    
    # Create ext4 filesystem
    mkfs.ext4 -F -L "hodei-rootfs" "${WORK_DIR}/rootfs.ext4" &>/dev/null
    
    # Setup loop device and mount
    LOOP_DEV=$(losetup -f --show "${WORK_DIR}/rootfs.ext4")
    mkdir -p "${WORK_DIR}/rootfs"
    mount "${LOOP_DEV}" "${WORK_DIR}/rootfs"
    
    log_info "Rootfs image created and mounted at ${WORK_DIR}/rootfs"
}

install_alpine() {
    log_info "Installing Alpine Linux ${ALPINE_VERSION}..."
    
    local rootfs="${WORK_DIR}/rootfs"
    local arch="x86_64"
    
    # Download Alpine minirootfs
    local alpine_url="${ALPINE_MIRROR}/v${ALPINE_VERSION}/releases/${arch}/alpine-minirootfs-${ALPINE_VERSION}.0-${arch}.tar.gz"
    local tarball="${WORK_DIR}/alpine-minirootfs.tar.gz"
    
    if [[ ! -f "${tarball}" ]]; then
        log_info "Downloading Alpine minirootfs..."
        wget -q -O "${tarball}" "${alpine_url}" || {
            log_error "Failed to download Alpine minirootfs from ${alpine_url}"
            exit 1
        }
    fi
    
    # Extract to rootfs
    tar -xzf "${tarball}" -C "${rootfs}"
    
    # Setup DNS
    echo "nameserver 8.8.8.8" > "${rootfs}/etc/resolv.conf"
    
    # Setup APK repositories
    cat > "${rootfs}/etc/apk/repositories" << EOF
${ALPINE_MIRROR}/v${ALPINE_VERSION}/main
${ALPINE_MIRROR}/v${ALPINE_VERSION}/community
EOF
    
    log_info "Alpine Linux installed"
}

install_packages() {
    log_info "Installing required packages..."
    
    local rootfs="${WORK_DIR}/rootfs"
    
    # Install packages via chroot
    chroot "${rootfs}" /sbin/apk update
    chroot "${rootfs}" /sbin/apk add --no-cache \
        busybox-initscripts \
        openrc \
        ca-certificates \
        curl \
        iproute2 \
        iptables
    
    log_info "Packages installed"
}

configure_system() {
    log_info "Configuring system..."
    
    local rootfs="${WORK_DIR}/rootfs"
    
    # Set hostname
    echo "hodei-worker" > "${rootfs}/etc/hostname"
    
    # Configure /etc/hosts
    cat > "${rootfs}/etc/hosts" << EOF
127.0.0.1   localhost
::1         localhost
EOF
    
    # Configure inittab for serial console
    cat > "${rootfs}/etc/inittab" << EOF
::sysinit:/sbin/openrc sysinit
::sysinit:/sbin/openrc boot
::wait:/sbin/openrc default
ttyS0::respawn:/sbin/getty -L ttyS0 115200 vt100
::ctrlaltdel:/sbin/reboot
::shutdown:/sbin/openrc shutdown
EOF
    
    # Enable serial console
    mkdir -p "${rootfs}/etc/init.d"
    
    # Create network setup script
    cat > "${rootfs}/etc/init.d/hodei-network" << 'EOF'
#!/sbin/openrc-run

description="Hodei Network Setup"

depend() {
    need localmount
    before hodei-agent
}

start() {
    ebegin "Configuring network from kernel args"
    
    # Parse kernel command line for network config
    eval $(cat /proc/cmdline | tr ' ' '\n' | grep -E '^HODEI_')
    
    if [ -n "${HODEI_VM_IP}" ] && [ -n "${HODEI_GATEWAY}" ]; then
        ip addr add "${HODEI_VM_IP}/30" dev eth0 2>/dev/null || true
        ip link set eth0 up
        ip route add default via "${HODEI_GATEWAY}" 2>/dev/null || true
        echo "nameserver 8.8.8.8" > /etc/resolv.conf
    fi
    
    eend $?
}
EOF
    chmod +x "${rootfs}/etc/init.d/hodei-network"
    
    # Create Hodei agent init script
    cat > "${rootfs}/etc/init.d/hodei-agent" << 'EOF'
#!/sbin/openrc-run

description="Hodei Worker Agent"
command="/usr/bin/hodei-worker"
command_background="yes"
pidfile="/run/hodei-agent.pid"
output_log="/var/log/hodei/agent.log"
error_log="/var/log/hodei/agent.log"

depend() {
    need net hodei-network
}

start_pre() {
    mkdir -p /var/log/hodei
    
    # Parse kernel command line for agent config
    eval $(cat /proc/cmdline | tr ' ' '\n' | grep -E '^HODEI_')
    
    # Build command args
    command_args=""
    [ -n "${HODEI_WORKER_ID}" ] && command_args="${command_args} --worker-id=${HODEI_WORKER_ID}"
    [ -n "${HODEI_SERVER_ADDRESS}" ] && command_args="${command_args} --server-address=${HODEI_SERVER_ADDRESS}"
    [ -n "${HODEI_OTP_TOKEN}" ] && command_args="${command_args} --otp-token=${HODEI_OTP_TOKEN}"
}
EOF
    chmod +x "${rootfs}/etc/init.d/hodei-agent"
    
    # Enable services
    chroot "${rootfs}" rc-update add hodei-network default
    chroot "${rootfs}" rc-update add hodei-agent default
    
    # Create log directory
    mkdir -p "${rootfs}/var/log/hodei"
    
    # Set root password (empty for VM access)
    chroot "${rootfs}" passwd -d root
    
    log_info "System configured"
}

install_hodei_agent() {
    log_info "Installing Hodei worker agent..."
    
    local rootfs="${WORK_DIR}/rootfs"
    local agent_binary="${PROJECT_ROOT}/target/release/hodei-worker"
    
    # Check if agent binary exists
    if [[ -f "${agent_binary}" ]]; then
        cp "${agent_binary}" "${rootfs}/usr/bin/hodei-worker"
        chmod +x "${rootfs}/usr/bin/hodei-worker"
        log_info "Hodei agent installed from ${agent_binary}"
    else
        log_warn "Hodei agent binary not found at ${agent_binary}"
        log_warn "Creating placeholder. Build with: cargo build --release -p hodei-worker"
        
        # Create placeholder script
        cat > "${rootfs}/usr/bin/hodei-worker" << 'EOF'
#!/bin/sh
echo "Hodei worker agent placeholder"
echo "Build the real agent with: cargo build --release -p hodei-worker"
echo "Arguments: $@"
# Keep running to prevent restart loop
while true; do sleep 3600; done
EOF
        chmod +x "${rootfs}/usr/bin/hodei-worker"
    fi
}

finalize_rootfs() {
    log_info "Finalizing rootfs..."
    
    local rootfs="${WORK_DIR}/rootfs"
    
    # Clean up
    rm -rf "${rootfs}/var/cache/apk/*"
    rm -rf "${rootfs}/tmp/*"
    
    # Sync and unmount
    sync
    umount "${rootfs}"
    losetup -d "${LOOP_DEV}"
    LOOP_DEV=""
    
    # Move to output location
    mkdir -p "$(dirname "${OUTPUT_PATH}")"
    mv "${WORK_DIR}/rootfs.ext4" "${OUTPUT_PATH}"
    
    log_info "Rootfs created at ${OUTPUT_PATH}"
    log_info "Size: $(du -h "${OUTPUT_PATH}" | cut -f1)"
}

print_usage() {
    cat << EOF
Hodei Firecracker Rootfs Builder

Usage: $0 [output_path]

Options:
  output_path    Path for the output rootfs image (default: /var/lib/hodei/rootfs.ext4)

Environment Variables:
  ROOTFS_SIZE_MB    Size of rootfs in MB (default: 512)
  ALPINE_VERSION    Alpine Linux version (default: 3.19)
  WORK_DIR          Working directory (default: /tmp/hodei-rootfs-build)

Example:
  sudo $0 /var/lib/hodei/rootfs.ext4
  sudo ROOTFS_SIZE_MB=1024 $0 ./rootfs.ext4

EOF
}

main() {
    if [[ "$1" == "-h" ]] || [[ "$1" == "--help" ]]; then
        print_usage
        exit 0
    fi
    
    log_info "=== Hodei Firecracker Rootfs Builder ==="
    log_info "Output: ${OUTPUT_PATH}"
    log_info "Size: ${ROOTFS_SIZE_MB}MB"
    log_info "Alpine: ${ALPINE_VERSION}"
    echo
    
    check_root
    check_dependencies
    create_rootfs_image
    install_alpine
    install_packages
    configure_system
    install_hodei_agent
    finalize_rootfs
    
    echo
    log_info "=== Build Complete ==="
    log_info "Rootfs ready at: ${OUTPUT_PATH}"
    log_info ""
    log_info "To use with Firecracker:"
    log_info "  export HODEI_FC_ROOTFS_PATH=${OUTPUT_PATH}"
    log_info ""
}

main "$@"
