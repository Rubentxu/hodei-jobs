# Firecracker Scripts

Scripts for setting up and managing Firecracker microVMs for Hodei.

## Prerequisites

- Linux host with KVM support
- Root access (for rootfs building and TAP device creation)
- Firecracker binary installed

## Scripts

### check-env.sh

Verifies that all Firecracker requirements are met.

```bash
./scripts/firecracker/check-env.sh
```

Checks:
- `/dev/kvm` accessibility
- Firecracker binary
- Jailer binary (optional)
- Kernel image
- Rootfs image
- Network capabilities

### build-rootfs.sh

Builds a minimal Alpine Linux rootfs with Hodei worker agent.

```bash
sudo ./scripts/firecracker/build-rootfs.sh [output_path]
```

**Options:**
- `output_path` - Output file (default: `/var/lib/hodei/rootfs.ext4`)

**Environment Variables:**
- `ROOTFS_SIZE_MB` - Size in MB (default: 512)
- `ALPINE_VERSION` - Alpine version (default: 3.19)
- `WORK_DIR` - Working directory (default: `/tmp/hodei-rootfs-build`)

**Example:**
```bash
sudo ROOTFS_SIZE_MB=1024 ./scripts/firecracker/build-rootfs.sh ./my-rootfs.ext4
```

## Getting Firecracker

Download from GitHub releases:

```bash
# Get latest release
FC_VERSION=$(curl -s https://api.github.com/repos/firecracker-microvm/firecracker/releases/latest | grep tag_name | cut -d '"' -f 4)

# Download
curl -L -o firecracker.tgz \
  "https://github.com/firecracker-microvm/firecracker/releases/download/${FC_VERSION}/firecracker-${FC_VERSION}-x86_64.tgz"

# Extract
tar -xzf firecracker.tgz

# Install
sudo mv release-${FC_VERSION}-x86_64/firecracker-${FC_VERSION}-x86_64 /usr/bin/firecracker
sudo mv release-${FC_VERSION}-x86_64/jailer-${FC_VERSION}-x86_64 /usr/bin/jailer
sudo chmod +x /usr/bin/firecracker /usr/bin/jailer
```

## Getting a Kernel

Download a pre-built kernel:

```bash
# Download Firecracker's sample kernel
curl -L -o vmlinux.bin \
  "https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.9/x86_64/vmlinux-5.10.217"

sudo mkdir -p /var/lib/hodei
sudo mv vmlinux.bin /var/lib/hodei/vmlinux
```

Or build your own following [Firecracker's kernel guide](https://github.com/firecracker-microvm/firecracker/blob/main/docs/rootfs-and-kernel-setup.md).

## Quick Setup

```bash
# 1. Check environment
./scripts/firecracker/check-env.sh

# 2. Install Firecracker (if needed)
# See "Getting Firecracker" above

# 3. Get kernel
sudo mkdir -p /var/lib/hodei
curl -L -o /tmp/vmlinux \
  "https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.9/x86_64/vmlinux-5.10.217"
sudo mv /tmp/vmlinux /var/lib/hodei/vmlinux

# 4. Build rootfs
sudo ./scripts/firecracker/build-rootfs.sh

# 5. Verify
./scripts/firecracker/check-env.sh

# 6. Enable Firecracker provider
export HODEI_FC_ENABLED=1
```

## Troubleshooting

### KVM not available

```bash
# Check CPU virtualization support
grep -E '(vmx|svm)' /proc/cpuinfo

# Load KVM module
sudo modprobe kvm_intel  # or kvm_amd

# Check KVM device
ls -la /dev/kvm

# Fix permissions
sudo chmod 666 /dev/kvm
# Or add user to kvm group
sudo usermod -aG kvm $USER
```

### TAP device creation fails

TAP devices require root or `CAP_NET_ADMIN`:

```bash
# Run as root
sudo ./hodei-server

# Or grant capability
sudo setcap cap_net_admin+ep ./hodei-server
```

### Firecracker fails to start

Check logs:
```bash
# VM logs are in data directory
cat /var/lib/hodei/firecracker/fc-*/console.log
```

Common issues:
- Kernel not compatible with rootfs
- Insufficient memory
- Network configuration issues
