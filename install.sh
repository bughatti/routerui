#!/bin/bash
# RouterUI Easy Install Script
# Usage: curl -sSL <url> | bash
# Or: ./install.sh

set -e

ROUTERUI_DIR="/opt/routerui"
ROUTERUI_USER="routerui"

echo "======================================"
echo "  RouterUI Installer"
echo "======================================"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Please run as root (sudo ./install.sh)"
    exit 1
fi

# Detect OS
if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS=$ID
    VERSION=$VERSION_ID
else
    echo "Cannot detect OS"
    exit 1
fi

echo "[1/8] Updating system packages..."
apt-get update -qq
apt-get upgrade -y -qq

echo "[2/8] Installing dependencies..."
apt-get install -y -qq \
    curl \
    git \
    build-essential \
    pkg-config \
    libssl-dev \
    sqlite3 \
    libsqlite3-dev \
    net-tools \
    iproute2 \
    iptables \
    dnsmasq \
    ufw

echo "[3/8] Installing Node.js 20..."
if ! command -v node &> /dev/null; then
    curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
    apt-get install -y -qq nodejs
fi
echo "Node.js version: $(node --version)"

echo "[4/8] Installing Rust..."
if ! command -v rustc &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
export PATH="$HOME/.cargo/bin:$PATH"
echo "Rust version: $(rustc --version)"

echo "[5/8] Creating RouterUI directory structure..."
mkdir -p $ROUTERUI_DIR/{config,frontend,backend}

echo "[6/8] Copying RouterUI files..."
# If running from the repo directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -d "$SCRIPT_DIR/backend" ] && [ -d "$SCRIPT_DIR/frontend" ]; then
    cp -r "$SCRIPT_DIR/backend/"* $ROUTERUI_DIR/backend/
    cp -r "$SCRIPT_DIR/frontend/"* $ROUTERUI_DIR/frontend/
else
    echo "Error: Cannot find backend/frontend directories"
    echo "Please run this script from the RouterUI repository root"
    exit 1
fi

echo "[7/8] Building RouterUI..."
# Build backend
cd $ROUTERUI_DIR/backend
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release
cp target/release/routerui-api $ROUTERUI_DIR/

# Build frontend
cd $ROUTERUI_DIR/frontend
npm install --silent
npm run build --silent
mkdir -p $ROUTERUI_DIR/frontend/build
cp -r dist/* $ROUTERUI_DIR/frontend/build/ 2>/dev/null || cp -r build/* $ROUTERUI_DIR/frontend/build/ 2>/dev/null || true

echo "[8/8] Setting up systemd service..."
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
# Uncomment for mock mode:
#Environment=ROUTERUI_MOCK=true
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable routerui
systemctl start routerui

echo ""
echo "======================================"
echo "  RouterUI Installation Complete!"
echo "======================================"
echo ""
echo "Access RouterUI at: http://localhost:3080"
echo ""
echo "Default credentials:"
echo "  Username: admin"
echo "  Password: admin"
echo ""
echo "Commands:"
echo "  Start:   sudo systemctl start routerui"
echo "  Stop:    sudo systemctl stop routerui"
echo "  Status:  sudo systemctl status routerui"
echo "  Logs:    sudo journalctl -u routerui -f"
echo ""
echo "To enable mock mode, edit /etc/systemd/system/routerui.service"
echo "and uncomment the ROUTERUI_MOCK=true line"
echo ""
