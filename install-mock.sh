#!/bin/bash
# RouterUI Easy Install Script - MOCK MODE
# For testing without real network hardware
# Usage: sudo ./install-mock.sh

set -e

ROUTERUI_DIR="/opt/routerui"

echo "======================================"
echo "  RouterUI Installer (Mock Mode)"
echo "======================================"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Please run as root (sudo ./install-mock.sh)"
    exit 1
fi

echo "[1/7] Updating system packages..."
apt-get update -qq

echo "[2/7] Installing minimal dependencies..."
apt-get install -y -qq \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    sqlite3 \
    libsqlite3-dev

echo "[3/7] Installing Node.js 20..."
if ! command -v node &> /dev/null; then
    curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
    apt-get install -y -qq nodejs
fi
echo "Node.js version: $(node --version)"

echo "[4/7] Installing Rust..."
if ! command -v rustc &> /dev/null; then
    sudo -u $SUDO_USER bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
fi
CARGO_HOME="/home/$SUDO_USER/.cargo"
export PATH="$CARGO_HOME/bin:$PATH"
echo "Rust version: $($CARGO_HOME/bin/rustc --version)"

echo "[5/7] Creating RouterUI directory structure..."
mkdir -p $ROUTERUI_DIR/{config,frontend/build,backend}

echo "[6/7] Copying and building RouterUI..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Build backend
cp -r "$SCRIPT_DIR/backend/"* $ROUTERUI_DIR/backend/
cd $ROUTERUI_DIR/backend
sudo -u $SUDO_USER bash -c "source $CARGO_HOME/env && cargo build --release"
cp target/release/routerui-api $ROUTERUI_DIR/

# Build frontend
cp -r "$SCRIPT_DIR/frontend/"* $ROUTERUI_DIR/frontend/
cd $ROUTERUI_DIR/frontend
npm install --silent
npm run build --silent
cp -r dist/* $ROUTERUI_DIR/frontend/build/ 2>/dev/null || cp -r build/* $ROUTERUI_DIR/frontend/build/ 2>/dev/null || true

echo "[7/7] Setting up systemd service (Mock Mode)..."
cat > /etc/systemd/system/routerui.service << 'EOF'
[Unit]
Description=RouterUI Web Interface (Mock Mode)
After=network.target

[Service]
Type=simple
ExecStart=/opt/routerui/routerui-api
WorkingDirectory=/opt/routerui
Environment=DATABASE_URL=sqlite:/opt/routerui/config/routerui.db?mode=rwc
Environment=FRONTEND_DIR=/opt/routerui/frontend/build
Environment=ROUTERUI_PORT=3080
Environment=ROUTERUI_MOCK=true
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
echo "  Running in MOCK MODE"
echo "======================================"
echo ""
echo "Access RouterUI at: http://localhost:3080"
echo ""
echo "Default credentials:"
echo "  Username: admin"
echo "  Password: admin"
echo ""
echo "Mock mode is ENABLED - all data is simulated"
echo "To disable mock mode, edit /etc/systemd/system/routerui.service"
echo ""
