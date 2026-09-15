# RouterUI

A self-hosted web interface that turns a plain Linux box into a capable home
router — DHCP, DNS, NAT, and a stateful firewall out of the box, with everything
else (VPN, ad-blocking, intrusion detection, DPI, containers) available as
opt-in add-ons.

The goal is **prosumer-grade** capability — think UniFi-class features on
hardware you already own — without the complexity or cost of enterprise gear.
It is **not** meant to compete with Cisco/Juniper enterprise routers.

- **Backend:** Rust (axum, tokio, SQLite) — a single static binary.
- **Frontend:** SvelteKit (static SPA), served by the backend.
- **No Python, no heavyweight runtime.** It configures the standard Linux
  networking stack (`dnsmasq`, `iptables`/`ipset`, `tc`, `wg`, …) directly.

> ⚠️ RouterUI manages firewall, routing, and DHCP for a whole network and runs
> with root privileges. Run it on a dedicated router box, not your workstation.

---

## Router first, everything else is an add-on

On install you get a **working router** — nothing optional is enabled until you
ask for it. The web UI's core section covers the router itself; add-ons appear
only once installed.

### Core (always available)

| Feature | What it does |
|---|---|
| **DHCP & DNS** | `dnsmasq`-based leases, static reservations, local DNS records |
| **NAT & Firewall** | Stateful firewall **on by default**; LAN→WAN masquerade; safe defaults (WAN can't reach the router) |
| **Networks (VLANs)** | 802.1Q networks, each with its own subnet, DHCP, and optional isolation |
| **Guest network** | Isolated network for visitors; Wi-Fi guest SSID with client isolation when a radio is present |
| **Port forwarding** | DNAT rules to expose a LAN service, with input validation and auto-rollback |
| **Traffic (QoS)** | Per-client bandwidth limits and a Smart Queue (CAKE) to kill bufferbloat |
| **Traffic Insight** | Per-device usage, live connections (with reverse-DNS), real-time bandwidth, and top domains per device |
| **Dynamic DNS** | Keeps a hostname pointed at a changing WAN IP (DuckDNS, Cloudflare, No-IP, or a custom URL) |
| **UPnP / NAT-PMP** | Optional automatic port mapping for LAN devices (off by default, `secure_mode`) |
| **Dual-WAN failover** | Monitors the primary uplink and fails over to a backup (e.g. LTE/USB) |
| **WAN speed test** | On-demand throughput test (librespeed) |
| **Diagnostics** | ping, traceroute, DNS lookup, Wake-on-LAN, traffic graphs, system logs, backup/restore |

### Add-ons (installed on demand)

| Add-on | Purpose |
|---|---|
| **AdGuard Home** | Network-wide DNS filtering / ad-blocking |
| **WireGuard** | Self-hosted VPN server; add peers and get QR-code configs |
| **Tailscale** | Zero-config mesh VPN for remote access |
| **CrowdSec** | Collaborative intrusion detection + IP banning |
| **ClamAV** | On-box antivirus scanning |
| **ntopng (DPI)** | Deep traffic analysis — per-application classification, top talkers, history |
| **Docker** | Container runtime for extra services |
| **Jellyfin** | Media server (via Docker) |
| **Pi-hole** | Alternative DNS ad-blocker |

---

## Requirements

- A Linux host (developed and tested on Ubuntu) with **two network interfaces**
  — one for the WAN uplink, one for the LAN.
- Root access.
- Enough disk for the base install (add-ons like Docker/ntopng need more).

The base install pulls only router-core packages (`dnsmasq`, `iptables`,
`ipset`, `conntrack`, `vnstat`, `vlan`, …) plus the build toolchain; add-ons
install their own dependencies when you enable them.

## Install

```bash
curl -sSL https://raw.githubusercontent.com/bughatti/routerui/master/install.sh | sudo bash
```

The installer builds the backend and frontend from source, installs a hardened
systemd service, and enables the router core. On first launch, open
`http://<router-lan-ip>:3080` (default `http://192.168.1.1:3080`) and complete
the setup wizard to pick your WAN/LAN interfaces and create the admin account.

## Development

```bash
# Backend
cd backend && cargo build --release

# Frontend
cd frontend && npm install && npm run build
```

Run the backend with `ROUTERUI_MOCK=1` to serve realistic mock data without
touching the system — useful for UI work off a router. See
[TESTING.md](TESTING.md) for environment notes.

## Security

- Session-based auth with a central gate on every protected route; the setup
  routes lock once an admin exists.
- User input that reaches `iptables`, `ip`, `dnsmasq`, or `tc` is validated to
  prevent command/config injection.
- The systemd unit is sandboxed (`ProtectHome`, `PrivateTmp`,
  `RestrictSUIDSGID`, and more).
- No secrets are baked into the binary; generated credentials live outside the
  source tree.

Found a vulnerability? Please report it privately — see
[SECURITY.md](SECURITY.md).

## Privacy note

Traffic Insight can show an admin every domain each device resolves. It ships
with an on/off toggle and a retention cap, and per-device DNS logging is only
enabled while insight is on. Use it responsibly on networks you administer.

## License

Licensed under the **GNU Affero General Public License v3.0** — see
[LICENSE](LICENSE). In short: you're free to use, modify, and self-host it, but
if you distribute it or run a modified version as a network service, you must
make your source available under the same license.
