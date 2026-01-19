#!/bin/bash
# RouterUI Install Script
# Usage: curl -sSL https://raw.githubusercontent.com/USER/routerui/main/install.sh | sudo bash

set -e

# Configuration
ROUTERUI_VERSION="${ROUTERUI_VERSION:-latest}"
ROUTERUI_DIR="/opt/routerui"
GITHUB_REPO="USER/routerui"  # Change this to actual repo

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo ""
echo -e "${GREEN}======================================"
echo "       RouterUI Installer"
echo -e "======================================${NC}"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}Error: Please run as root${NC}"
    echo "Usage: sudo ./install.sh"
    exit 1
fi

# Detect OS
if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS=$ID
else
    echo -e "${RED}Error: Cannot detect OS. This script requires Debian/Ubuntu.${NC}"
    exit 1
fi

if [[ "$OS" != "debian" && "$OS" != "ubuntu" ]]; then
    echo -e "${YELLOW}Warning: This script is designed for Debian/Ubuntu.${NC}"
    echo "Detected: $OS"
    read -p "Continue anyway? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Detect architecture
ARCH=$(uname -m)
case $ARCH in
    x86_64)
        ARCH="x86_64"
        ;;
    aarch64|arm64)
        ARCH="aarch64"
        ;;
    armv7l)
        ARCH="armv7"
        ;;
    *)
        echo -e "${RED}Error: Unsupported architecture: $ARCH${NC}"
        exit 1
        ;;
esac

echo "Detected: $OS ($ARCH)"
echo ""

# Step 1: Install system packages
echo -e "${GREEN}[1/5]${NC} Installing system packages..."
apt-get update -qq

# Pre-configure iptables-persistent to not prompt
echo iptables-persistent iptables-persistent/autosave_v4 boolean true | debconf-set-selections 2>/dev/null || true
echo iptables-persistent iptables-persistent/autosave_v6 boolean true | debconf-set-selections 2>/dev/null || true

DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    dnsmasq \
    iptables \
    iptables-persistent \
    vnstat \
    curl \
    wget \
    sqlite3 \
    net-tools \
    iproute2 \
    > /dev/null 2>&1

echo "  - dnsmasq (DHCP/DNS)"
echo "  - iptables-persistent (firewall)"
echo "  - vnstat (traffic monitoring)"

# Disable dnsmasq for now (setup wizard will configure and enable it)
systemctl stop dnsmasq 2>/dev/null || true
systemctl disable dnsmasq 2>/dev/null || true

# Enable vnstat
systemctl enable vnstat > /dev/null 2>&1
systemctl start vnstat > /dev/null 2>&1

# Step 2: Enable IP forwarding
echo -e "${GREEN}[2/5]${NC} Enabling IP forwarding..."
echo 1 > /proc/sys/net/ipv4/ip_forward
if ! grep -q "^net.ipv4.ip_forward=1" /etc/sysctl.conf 2>/dev/null; then
    echo "net.ipv4.ip_forward=1" >> /etc/sysctl.conf
fi
mkdir -p /etc/sysctl.d
echo "net.ipv4.ip_forward=1" > /etc/sysctl.d/99-routerui.conf

# Step 3: Create directory structure
echo -e "${GREEN}[3/5]${NC} Creating RouterUI directories..."
mkdir -p $ROUTERUI_DIR/{config,frontend}

# Step 4: Download RouterUI
echo -e "${GREEN}[4/5]${NC} Downloading RouterUI..."

# Determine download URL
if [ "$ROUTERUI_VERSION" = "latest" ]; then
    RELEASE_URL="https://github.com/$GITHUB_REPO/releases/latest/download"
else
    RELEASE_URL="https://github.com/$GITHUB_REPO/releases/download/$ROUTERUI_VERSION"
fi

BINARY_NAME="routerui-api-linux-$ARCH"
FRONTEND_NAME="routerui-frontend.tar.gz"

# Download backend binary
echo "  Downloading backend..."
if ! wget -q --show-progress -O "$ROUTERUI_DIR/routerui-api" "$RELEASE_URL/$BINARY_NAME" 2>/dev/null; then
    # Fallback: try without architecture suffix
    if ! wget -q --show-progress -O "$ROUTERUI_DIR/routerui-api" "$RELEASE_URL/routerui-api" 2>/dev/null; then
        echo -e "${YELLOW}  Download failed. Trying to build from source...${NC}"
        BUILD_FROM_SOURCE=true
    fi
fi

# Download frontend
echo "  Downloading frontend..."
if ! wget -q --show-progress -O "/tmp/$FRONTEND_NAME" "$RELEASE_URL/$FRONTEND_NAME" 2>/dev/null; then
    echo -e "${YELLOW}  Download failed. Trying to build from source...${NC}"
    BUILD_FROM_SOURCE=true
fi

# If downloads failed, try building from source
if [ "$BUILD_FROM_SOURCE" = true ]; then
    echo ""
    echo -e "${YELLOW}Pre-built binaries not available. Building from source...${NC}"
    echo "This will take several minutes."
    echo ""

    # Check if we're in the repo directory
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" 2>/dev/null && pwd)" || SCRIPT_DIR="."

    if [ ! -d "$SCRIPT_DIR/backend" ] || [ ! -d "$SCRIPT_DIR/frontend" ]; then
        echo "Cloning repository..."
        apt-get install -y -qq git > /dev/null 2>&1
        rm -rf /tmp/routerui-build
        git clone --depth 1 "https://github.com/$GITHUB_REPO.git" /tmp/routerui-build 2>/dev/null || {
            echo -e "${RED}Error: Could not clone repository.${NC}"
            echo "Please clone manually and run install.sh from the repo directory."
            exit 1
        }
        SCRIPT_DIR="/tmp/routerui-build"
    fi

    # Install Rust if needed
    if ! command -v cargo &> /dev/null; then
        echo "Installing Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y > /dev/null 2>&1
        source "$HOME/.cargo/env"
    fi
    export PATH="$HOME/.cargo/bin:$PATH"

    # Install Node.js if needed
    if ! command -v npm &> /dev/null; then
        echo "Installing Node.js..."
        curl -fsSL https://deb.nodesource.com/setup_20.x | bash - > /dev/null 2>&1
        apt-get install -y -qq nodejs > /dev/null 2>&1
    fi

    # Build backend
    echo "Building backend (this takes a few minutes)..."
    cp -r "$SCRIPT_DIR/backend/"* $ROUTERUI_DIR/backend/ 2>/dev/null || mkdir -p $ROUTERUI_DIR/backend && cp -r "$SCRIPT_DIR/backend/"* $ROUTERUI_DIR/backend/
    cd $ROUTERUI_DIR/backend
    cargo build --release 2>/dev/null
    cp target/release/routerui-api $ROUTERUI_DIR/

    # Build frontend
    echo "Building frontend..."
    cp -r "$SCRIPT_DIR/frontend/"* $ROUTERUI_DIR/frontend/ 2>/dev/null || true
    cd $ROUTERUI_DIR/frontend
    npm install --silent 2>/dev/null
    npm run build --silent 2>/dev/null
    mkdir -p $ROUTERUI_DIR/frontend/build
    cp -r dist/* $ROUTERUI_DIR/frontend/build/ 2>/dev/null || cp -r build/* $ROUTERUI_DIR/frontend/build/ 2>/dev/null || true

    # Cleanup
    rm -rf /tmp/routerui-build
else
    # Extract frontend
    chmod +x "$ROUTERUI_DIR/routerui-api"
    tar -xzf "/tmp/$FRONTEND_NAME" -C "$ROUTERUI_DIR/frontend" 2>/dev/null
    rm -f "/tmp/$FRONTEND_NAME"

    # Ensure build directory exists
    if [ ! -d "$ROUTERUI_DIR/frontend/build" ]; then
        mkdir -p "$ROUTERUI_DIR/frontend/build"
        mv $ROUTERUI_DIR/frontend/*/* "$ROUTERUI_DIR/frontend/build/" 2>/dev/null || true
    fi
fi

# Step 5: Create systemd service
echo -e "${GREEN}[5/5]${NC} Setting up systemd service..."
cat > /etc/systemd/system/routerui.service << 'EOF'
[Unit]
Description=RouterUI Web Interface
After=network.target

[Service]
Type=simple
ExecStart=/opt/routerui/routerui-api
WorkingDirectory=/opt/routerui
Environment=DATABASE_URL=sqlite:/opt/routerui/config/routerui.db?mode=rwc
Environment=FRONTEND_DIR=/opt/routerui/frontend/build
Environment=ROUTERUI_PORT=3080
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable routerui > /dev/null 2>&1
systemctl start routerui

# Wait for service to start
sleep 2

# Get server IP
SERVER_IP=$(hostname -I | awk '{print $1}')

# Check if service is running
if systemctl is-active --quiet routerui; then
    STATUS="${GREEN}running${NC}"
else
    STATUS="${RED}not running (check: journalctl -u routerui)${NC}"
fi

echo ""
echo -e "${GREEN}======================================"
echo "     Installation Complete!"
echo -e "======================================${NC}"
echo ""
echo "RouterUI Status: $STATUS"
echo ""
echo -e "Open your browser to complete setup:"
echo ""
echo -e "  ${GREEN}http://${SERVER_IP}:3080${NC}"
echo ""
echo "The setup wizard will:"
echo "  1. Create your admin account"
echo "  2. Configure WAN/LAN interfaces"
echo "  3. Set up DHCP, DNS, and NAT"
echo ""
echo "Commands:"
echo "  Status:  systemctl status routerui"
echo "  Logs:    journalctl -u routerui -f"
echo "  Restart: systemctl restart routerui"
echo ""
