# RouterUI Testing Options

## Requirements

RouterUI requires a Linux environment with:
- `iptables`, `ip`, `systemctl`, `hostnamectl` access
- Rust toolchain (to build backend)
- Node.js (to build frontend)
- Optional: Docker, ClamAV, AdGuard Home for those features

## Testing Environments

### WSL2 (Recommended for Development)

**Pros:**
- Full Linux environment with systemd support
- Can build Rust backend + Node.js frontend natively
- Network commands (`iptables`, `ip`) will execute but only affect WSL's virtual network
- Quick iteration cycle for development

**Cons:**
- Network interfaces are virtualized (won't see real hardware interfaces)
- iptables works but won't affect host Windows networking
- Can't actually route traffic

**Best for:** UI testing, API testing, development workflow

### Windows Docker

**Pros:**
- Containerized, easy to deploy
- Good for testing packaging/distribution

**Cons:**
- Linux containers run inside WSL2/Hyper-V anyway
- No systemd (systemctl commands won't work)
- Limited network access
- Would need custom Dockerfile

**Best for:** Packaging and distribution testing

### Ubuntu Docker on NAS

**Pros:**
- Running on actual Linux host
- Could use `--privileged --network=host` for more access
- More realistic environment

**Cons:**
- Docker containers don't have systemd by default
- Limited access to host iptables/networking unless privileged
- Container isolation breaks most router management features

**Best for:** Testing in a more realistic Linux environment

## Production Deployment

For actual production use, RouterUI needs to run directly on a Linux router:
- Bare metal Linux installation
- VM with network passthrough
- NOT in Docker (too many limitations with networking/systemd)

## Features That Require Real Hardware

These features will only work on a proper Linux router, not in containers or WSL:
- Real network interface management
- iptables firewall rules affecting actual traffic
- DHCP server management
- WiFi configuration
- Traffic routing between interfaces
