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

# Get the actual user (not root)
ACTUAL_USER="${SUDO_USER:-$USER}"
ACTUAL_HOME=$(getent passwd "$ACTUAL_USER" | cut -d: -f6)

if [ -z "$ACTUAL_USER" ] || [ "$ACTUAL_USER" = "root" ]; then
    echo "Error: Please run with sudo as a regular user, not as root directly"
    exit 1
fi

# Function to wait for apt locks to be released
wait_for_apt() {
    local max_wait=60
    local waited=0
    while fuser /var/lib/dpkg/lock-frontend >/dev/null 2>&1 || \
          fuser /var/lib/apt/lists/lock >/dev/null 2>&1 || \
          fuser /var/cache/apt/archives/lock >/dev/null 2>&1; do
        if [ $waited -eq 0 ]; then
            echo "  Waiting for other package managers to finish..."
        fi
        sleep 2
        waited=$((waited + 2))
        if [ $waited -ge $max_wait ]; then
            echo "  Timeout waiting for apt locks. Attempting to clear..."
            pkill -9 apt-get 2>/dev/null || true
            pkill -9 dpkg 2>/dev/null || true
            rm -f /var/lib/dpkg/lock-frontend /var/lib/apt/lists/lock /var/cache/apt/archives/lock
            dpkg --configure -a 2>/dev/null || true
            break
        fi
    done
}

echo "[1/7] Updating system packages..."
wait_for_apt
apt-get update -qq

echo "[2/7] Installing minimal dependencies..."
wait_for_apt
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    sqlite3 \
    libsqlite3-dev

echo "[3/7] Installing Node.js 20..."
if ! command -v node &> /dev/null; then
    curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
    wait_for_apt
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nodejs
fi
echo "  Node.js version: $(node --version)"

echo "[4/7] Installing Rust..."
CARGO_HOME="$ACTUAL_HOME/.cargo"
if [ ! -f "$CARGO_HOME/bin/rustc" ]; then
    sudo -u "$ACTUAL_USER" bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
fi
# Ensure default toolchain is set
sudo -u "$ACTUAL_USER" bash -c "source $CARGO_HOME/env && rustup default stable" 2>/dev/null || true
export PATH="$CARGO_HOME/bin:$PATH"
echo "  Rust version: $($CARGO_HOME/bin/rustc --version 2>/dev/null || echo 'installed')"

echo "[5/7] Creating RouterUI directory structure..."
mkdir -p $ROUTERUI_DIR/{config,frontend/build,backend}
chown -R "$ACTUAL_USER:$ACTUAL_USER" $ROUTERUI_DIR

echo "[6/7] Building RouterUI (this may take a few minutes)..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Build backend as the actual user (not root)
echo "  Building backend..."
cp -r "$SCRIPT_DIR/backend/"* $ROUTERUI_DIR/backend/
chown -R "$ACTUAL_USER:$ACTUAL_USER" $ROUTERUI_DIR/backend
sudo -u "$ACTUAL_USER" bash -c "cd $ROUTERUI_DIR/backend && source $CARGO_HOME/env && cargo build --release"
cp $ROUTERUI_DIR/backend/target/release/routerui-api $ROUTERUI_DIR/

# Build frontend
echo "  Building frontend..."
cp -r "$SCRIPT_DIR/frontend/"* $ROUTERUI_DIR/frontend/
chown -R "$ACTUAL_USER:$ACTUAL_USER" $ROUTERUI_DIR/frontend
cd $ROUTERUI_DIR/frontend
sudo -u "$ACTUAL_USER" npm install --silent 2>/dev/null || npm install --silent
sudo -u "$ACTUAL_USER" npm run build --silent 2>/dev/null || npm run build --silent
cp -r build/* $ROUTERUI_DIR/frontend/build/ 2>/dev/null || cp -r dist/* $ROUTERUI_DIR/frontend/build/ 2>/dev/null || true

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
