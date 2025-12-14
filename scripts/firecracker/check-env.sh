#!/bin/bash
# Hodei Firecracker Environment Checker
# Verifies that all requirements for Firecracker are met

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

ERRORS=0
WARNINGS=0

check_pass() {
    echo -e "${GREEN}✓${NC} $1"
}

check_fail() {
    echo -e "${RED}✗${NC} $1"
    ((ERRORS++))
}

check_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
    ((WARNINGS++))
}

echo "=== Hodei Firecracker Environment Check ==="
echo

# Check KVM
echo "Checking KVM..."
if [[ -e /dev/kvm ]]; then
    if [[ -r /dev/kvm ]] && [[ -w /dev/kvm ]]; then
        check_pass "/dev/kvm is accessible"
    else
        check_fail "/dev/kvm exists but is not readable/writable"
        echo "    Try: sudo chmod 666 /dev/kvm"
        echo "    Or add user to kvm group: sudo usermod -aG kvm \$USER"
    fi
else
    check_fail "/dev/kvm does not exist"
    echo "    KVM is required for Firecracker"
    echo "    Check if your CPU supports virtualization (VT-x/AMD-V)"
    echo "    Try: sudo modprobe kvm_intel (or kvm_amd)"
fi

# Check Firecracker binary
echo
echo "Checking Firecracker..."
FC_PATH="${HODEI_FC_FIRECRACKER_PATH:-/usr/bin/firecracker}"
if [[ -x "${FC_PATH}" ]]; then
    FC_VERSION=$("${FC_PATH}" --version 2>/dev/null | head -1 || echo "unknown")
    check_pass "Firecracker found: ${FC_VERSION}"
else
    check_fail "Firecracker not found at ${FC_PATH}"
    echo "    Download from: https://github.com/firecracker-microvm/firecracker/releases"
fi

# Check Jailer binary
echo
echo "Checking Jailer..."
JAILER_PATH="${HODEI_FC_JAILER_PATH:-/usr/bin/jailer}"
if [[ -x "${JAILER_PATH}" ]]; then
    check_pass "Jailer found at ${JAILER_PATH}"
else
    check_warn "Jailer not found at ${JAILER_PATH}"
    echo "    Jailer is optional but recommended for production"
fi

# Check kernel
echo
echo "Checking kernel image..."
KERNEL_PATH="${HODEI_FC_KERNEL_PATH:-/var/lib/hodei/vmlinux}"
if [[ -f "${KERNEL_PATH}" ]]; then
    KERNEL_SIZE=$(du -h "${KERNEL_PATH}" | cut -f1)
    check_pass "Kernel found: ${KERNEL_PATH} (${KERNEL_SIZE})"
else
    check_fail "Kernel not found at ${KERNEL_PATH}"
    echo "    Download a compatible kernel from Firecracker releases"
    echo "    Or build one following: https://github.com/firecracker-microvm/firecracker/blob/main/docs/rootfs-and-kernel-setup.md"
fi

# Check rootfs
echo
echo "Checking rootfs image..."
ROOTFS_PATH="${HODEI_FC_ROOTFS_PATH:-/var/lib/hodei/rootfs.ext4}"
if [[ -f "${ROOTFS_PATH}" ]]; then
    ROOTFS_SIZE=$(du -h "${ROOTFS_PATH}" | cut -f1)
    check_pass "Rootfs found: ${ROOTFS_PATH} (${ROOTFS_SIZE})"
else
    check_warn "Rootfs not found at ${ROOTFS_PATH}"
    echo "    Build with: sudo ./scripts/firecracker/build-rootfs.sh"
fi

# Check data directory
echo
echo "Checking data directory..."
DATA_DIR="${HODEI_FC_DATA_DIR:-/var/lib/hodei/firecracker}"
if [[ -d "${DATA_DIR}" ]]; then
    if [[ -w "${DATA_DIR}" ]]; then
        check_pass "Data directory writable: ${DATA_DIR}"
    else
        check_fail "Data directory not writable: ${DATA_DIR}"
    fi
else
    check_warn "Data directory does not exist: ${DATA_DIR}"
    echo "    Will be created automatically when provider starts"
fi

# Check network capabilities
echo
echo "Checking network capabilities..."
if command -v ip &> /dev/null; then
    check_pass "ip command available"
else
    check_fail "ip command not found (iproute2 package)"
fi

# Check if we can create TAP devices (requires root or CAP_NET_ADMIN)
if [[ $EUID -eq 0 ]]; then
    check_pass "Running as root (can create TAP devices)"
else
    if capsh --print 2>/dev/null | grep -q "cap_net_admin"; then
        check_pass "CAP_NET_ADMIN capability available"
    else
        check_warn "Not running as root and no CAP_NET_ADMIN"
        echo "    TAP device creation may fail"
        echo "    Run Hodei server as root or with CAP_NET_ADMIN"
    fi
fi

# Summary
echo
echo "=== Summary ==="
if [[ ${ERRORS} -eq 0 ]] && [[ ${WARNINGS} -eq 0 ]]; then
    echo -e "${GREEN}All checks passed!${NC}"
    echo "Firecracker provider is ready to use."
elif [[ ${ERRORS} -eq 0 ]]; then
    echo -e "${YELLOW}${WARNINGS} warning(s), 0 errors${NC}"
    echo "Firecracker provider should work but may have limited functionality."
else
    echo -e "${RED}${ERRORS} error(s), ${WARNINGS} warning(s)${NC}"
    echo "Please fix the errors before using Firecracker provider."
fi

echo
echo "Environment variables for Hodei:"
echo "  HODEI_FC_ENABLED=1"
echo "  HODEI_FC_FIRECRACKER_PATH=${FC_PATH}"
echo "  HODEI_FC_KERNEL_PATH=${KERNEL_PATH}"
echo "  HODEI_FC_ROOTFS_PATH=${ROOTFS_PATH}"
echo "  HODEI_FC_DATA_DIR=${DATA_DIR}"

exit ${ERRORS}
