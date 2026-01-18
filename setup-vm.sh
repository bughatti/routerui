#!/bin/bash
# Post-Ubuntu-install setup script
# Run this FIRST after installing Ubuntu to prepare for RouterUI
# Usage: sudo ./setup-vm.sh

set -e

echo "======================================"
echo "  VM Environment Setup"
echo "======================================"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Please run as root (sudo ./setup-vm.sh)"
    exit 1
fi

echo "[1/4] Installing VirtualBox Guest Additions dependencies..."
apt-get update -qq
apt-get install -y -qq \
    build-essential \
    dkms \
    linux-headers-$(uname -r)

echo "[2/4] Installing Guest Additions (for shared folders)..."
# Mount and install if CD is present
if [ -b /dev/cdrom ]; then
    mkdir -p /mnt/cdrom
    mount /dev/cdrom /mnt/cdrom 2>/dev/null || true
    if [ -f /mnt/cdrom/VBoxLinuxAdditions.run ]; then
        /mnt/cdrom/VBoxLinuxAdditions.run --nox11 || true
        umount /mnt/cdrom
    fi
fi

echo "[3/4] Setting up shared folder mount..."
# Create mount point for shared folder
mkdir -p /mnt/routerui-share
# Add current user to vboxsf group
usermod -aG vboxsf $SUDO_USER 2>/dev/null || true

echo "[4/4] Configuring network interfaces..."
# Show current interfaces
ip addr show

echo ""
echo "======================================"
echo "  VM Setup Complete!"
echo "======================================"
echo ""
echo "Next steps:"
echo "1. Shut down the VM"
echo "2. In VirtualBox, add a shared folder:"
echo "   - Folder Path: C:\\Users\\Liquid\\trey\\routerui"
echo "   - Folder Name: routerui"
echo "   - Check 'Auto-mount' and 'Make Permanent'"
echo "3. Start the VM again"
echo "4. The shared folder will be at /media/sf_routerui"
echo "5. Run: sudo /media/sf_routerui/install-mock.sh"
echo ""
