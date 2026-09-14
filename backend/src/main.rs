mod api;
mod auth;
mod db;
mod mock;
mod models;
mod system;
mod validate;

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct AppState {
    pub db: sqlx::SqlitePool,
}

/// Central authentication gate. Everything under /api requires a valid admin
/// session except: the login endpoint, the setup status probe, and — only
/// while the setup wizard has not yet completed — the setup routes that create
/// the first admin. Non-/api paths are the static dashboard and pass through.
/// SPA fallback: returns index.html with a 200 status for any unmatched path.
async fn spa_index() -> Response {
    let dir = std::env::var("FRONTEND_DIR")
        .unwrap_or_else(|_| "/opt/routerui/frontend/build".to_string());
    match tokio::fs::read_to_string(format!("{}/index.html", dir)).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "UI not found").into_response(),
    }
}

async fn auth_gate(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();

    let is_public = !path.starts_with("/api/")
        || path == "/api/auth/login"
        || path == "/api/setup/status";

    let allow = if is_public {
        true
    } else if path.starts_with("/api/setup/") && !api::setup_complete(&state.db).await {
        // First-run: setup routes are open until the wizard records completion.
        true
    } else {
        match api::token_from_headers(req.headers()) {
            Some(token) => matches!(
                auth::validate_session(&state.db, &token).await,
                Ok(Some(user)) if user.enabled
            ),
            None => false,
        }
    };

    if allow {
        next.run(req).await
    } else {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"error":"authentication required"}"#))
            .unwrap()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "routerui_api=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_path = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:/opt/routerui/config/routerui.db?mode=rwc".to_string());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_path)
        .await?;

    db::migrate(&pool).await?;
    auth::create_default_admin(&pool).await?;

    let state = Arc::new(AppState { db: pool });

    // Restore router networking that lives outside /etc (VLAN sub-interfaces,
    // the guest network, QoS shaping, and the active WAN for failover) so it
    // survives a reboot without a separate systemd unit. These are cheap and
    // no-ops when nothing is configured.
    api::vlan::reapply_on_boot();
    api::guest::reapply_on_boot();
    api::qos::reapply_on_boot();
    api::failover::reapply_on_boot();
    api::traffic::reapply_on_boot();

    // Per-client bandwidth sampler: diff the accounting counters every 2s so the
    // traffic view has live rates. Cheap (a couple of iptables reads).
    tokio::spawn(async {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            tokio::task::spawn_blocking(api::traffic::sample_once).await.ok();
        }
    });

    // Dynamic DNS: refresh the provider on a timer so the record follows a
    // changing WAN IP without user interaction.
    tokio::spawn(async {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            tick.tick().await;
            let _ = api::ddns::run_updater_once().await;
        }
    });

    // Dual-WAN failover: probe the primary uplink and switch if it drops.
    tokio::spawn(async {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            tick.tick().await;
            let _ = api::failover::run_monitor_once().await;
        }
    });

    // Same-origin by default: the dashboard is served by this binary, so it
    // needs no CORS. A permissive policy previously let any website call the
    // API through a visitor's browser. Set ROUTERUI_CORS_ORIGIN to opt a
    // specific site in.
    let cors = match std::env::var("ROUTERUI_CORS_ORIGIN") {
        Ok(origin) if !origin.is_empty() => CorsLayer::new()
            .allow_origin(origin.parse::<axum::http::HeaderValue>().expect("invalid ROUTERUI_CORS_ORIGIN"))
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any),
        _ => CorsLayer::new(),
    };

    let frontend_dir = std::env::var("FRONTEND_DIR")
        .unwrap_or_else(|_| "/opt/routerui/frontend/build".to_string());

    let app = Router::new()
        // Setup wizard routes (no auth required)
        .route("/api/setup/status", get(api::setup::status))
        .route("/api/setup/interfaces", get(api::setup::get_interfaces))
        .route("/api/setup/admin", post(api::setup::create_admin))
        .route("/api/setup/configure-router", post(api::setup::configure_router))
        .route("/api/setup/network", post(api::setup::save_network_config))
        .route("/api/setup/complete", post(api::setup::complete))
        // Addons
        .route("/api/addons/status", get(api::addons::status))
        .route("/api/addons/list", get(api::addons::list))
        .route("/api/addons/install", post(api::addons::install))
        // Auth routes
        .route("/api/auth/login", post(api::auth::login))
        .route("/api/auth/logout", post(api::auth::logout))
        .route("/api/auth/me", get(api::auth::me))
        // User management
        .route("/api/users", get(api::users::list).post(api::users::create))
        .route("/api/users/{id}", get(api::users::get)
            .put(api::users::update)
            .delete(api::users::delete))
        // System status
        .route("/api/system/status", get(api::system::status))
        .route("/api/system/interfaces", get(api::system::interfaces))
        .route("/api/system/services", get(api::system::services))
        .route("/api/system/updates/check", post(api::system::check_updates))
        .route("/api/system/updates/install", post(api::system::install_updates))
        // Dashboard
        .route("/api/dashboard", get(api::dashboard::overview))
        // AdGuard Home
        .route("/api/adguard/overview", get(api::adguard::overview))
        .route("/api/adguard/protection", post(api::adguard::toggle_protection))
        .route("/api/adguard/querylog", get(api::adguard::query_log))
        .route("/api/adguard/filters", get(api::adguard::filters))
        .route("/api/adguard/filters/toggle", post(api::adguard::toggle_filter))
        .route("/api/adguard/rules/add", post(api::adguard::add_rule))
        .route("/api/adguard/rules/remove", post(api::adguard::remove_rule))
        // Firewall
        .route("/api/firewall/status", get(api::firewall::status))
        .route("/api/firewall/toggle", post(api::firewall::toggle))
        .route("/api/firewall/port-forwards", get(api::firewall::port_forwards))
        .route("/api/firewall/port-forwards/add", post(api::firewall::add_port_forward))
        .route("/api/firewall/port-forwards/remove", post(api::firewall::remove_port_forward))
        .route("/api/firewall/blocked-ips", get(api::firewall::blocked_ips))
        .route("/api/firewall/blocked-ips/add", post(api::firewall::add_blocked_ip))
        .route("/api/firewall/blocked-ips/remove", post(api::firewall::remove_blocked_ip))
        .route("/api/firewall/rules", get(api::firewall::raw_rules))
        .route("/api/firewall/dmz", get(api::firewall::dmz_status))
        .route("/api/firewall/dmz/set", post(api::firewall::set_dmz))
        .route("/api/firewall/pending", get(api::firewall::pending))
        .route("/api/firewall/confirm", post(api::firewall::confirm))
        .route("/api/firewall/revert", post(api::firewall::revert))
        // Protection
        .route("/api/protection/status", get(api::protection::status))
        .route("/api/protection/blocklists", get(api::protection::blocklists))
        .route("/api/protection/blocklists/toggle", post(api::protection::toggle_blocklist))
        .route("/api/protection/blocklists/update", post(api::protection::update_blocklists))
        .route("/api/protection/blocked-log", get(api::protection::blocked_log))
        .route("/api/protection/whitelist", get(api::protection::whitelist))
        .route("/api/protection/whitelist/add", post(api::protection::add_whitelist))
        .route("/api/protection/whitelist/remove", post(api::protection::remove_whitelist))
        .route("/api/protection/quick-allow", post(api::protection::quick_allow))
        .route("/api/protection/countries", get(api::protection::countries))
        .route("/api/protection/countries/toggle", post(api::protection::toggle_country))
        .route("/api/protection/enable-logging", post(api::protection::enable_logging))
        // Antivirus
        .route("/api/antivirus/status", get(api::antivirus::status))
        .route("/api/antivirus/update", post(api::antivirus::update_signatures))
        .route("/api/antivirus/scan", post(api::antivirus::start_scan))
        .route("/api/antivirus/quick-scan", post(api::antivirus::quick_scan))
        .route("/api/antivirus/history", get(api::antivirus::scan_history))
        .route("/api/antivirus/quarantine", get(api::antivirus::quarantine_list))
        .route("/api/antivirus/quarantine/action", post(api::antivirus::quarantine_action))
        .route("/api/antivirus/daemon", post(api::antivirus::toggle_daemon))
        // Network
        .route("/api/network/interfaces", get(api::network::interfaces))
        .route("/api/network/dhcp", get(api::network::dhcp_status))
        .route("/api/network/dhcp/config", post(api::network::update_dhcp_config))
        .route("/api/network/dhcp/static/add", post(api::network::add_static_lease))
        .route("/api/network/dhcp/static/remove", post(api::network::remove_static_lease))
        .route("/api/network/wifi", get(api::network::wifi_status))
        .route("/api/network/wifi/update", post(api::network::update_wifi))
        .route("/api/network/wifi/toggle", post(api::network::toggle_wifi))
        .route("/api/network/dns", get(api::network::dns_status))
        .route("/api/network/dns/local/add", post(api::network::add_local_dns))
        .route("/api/network/dns/local/remove", post(api::network::remove_local_dns))
        .route("/api/network/routes", get(api::network::routes))
        .route("/api/network/routes/add", post(api::network::add_route))
        .route("/api/network/routes/remove", post(api::network::remove_route))
        .route("/api/network/wol", get(api::network::wol_devices))
        .route("/api/network/wol/add", post(api::network::add_wol_device))
        .route("/api/network/wol/remove", post(api::network::remove_wol_device))
        .route("/api/network/wol/wake", post(api::network::wake_device))
        // Services Management
        .route("/api/services", get(api::services::list))
        .route("/api/services/all", get(api::services::list_all))
        .route("/api/services/action", post(api::services::action))
        .route("/api/services/logs", post(api::services::logs))
        .route("/api/services/status", post(api::services::status))
        // Docker
        .route("/api/docker/status", get(api::docker::status))
        .route("/api/docker/containers", get(api::docker::containers))
        .route("/api/docker/containers/action", post(api::docker::container_action))
        .route("/api/docker/containers/logs", post(api::docker::container_logs))
        .route("/api/docker/images", get(api::docker::images))
        .route("/api/docker/images/action", post(api::docker::image_action))
        .route("/api/docker/images/pull", post(api::docker::pull_image))
        .route("/api/docker/volumes", get(api::docker::volumes))
        .route("/api/docker/networks", get(api::docker::networks))
        // VPN (Tailscale + Gluetun/NordVPN)
        .route("/api/vpn/overview", get(api::vpn::overview))
        .route("/api/vpn/tailscale/status", get(api::vpn::tailscale_status))
        .route("/api/vpn/tailscale/devices", get(api::vpn::tailscale_devices))
        .route("/api/vpn/tailscale/connect", post(api::vpn::tailscale_connect))
        .route("/api/vpn/tailscale/disconnect", post(api::vpn::tailscale_disconnect))
        .route("/api/vpn/tailscale/logout", post(api::vpn::tailscale_logout))
        .route("/api/vpn/tailscale/exit-node", post(api::vpn::tailscale_set_exit_node))
        .route("/api/vpn/tailscale/netcheck", get(api::vpn::tailscale_netcheck))
        .route("/api/vpn/gluetun/status", get(api::vpn::gluetun_status))
        .route("/api/vpn/gluetun/restart", post(api::vpn::gluetun_restart))
        // WireGuard VPN server (self-hosted; installed as an addon)
        .route("/api/vpn/wireguard/status", get(api::wireguard::status))
        .route("/api/vpn/wireguard/disable", post(api::wireguard::disable))
        .route("/api/vpn/wireguard/peers", get(api::wireguard::list_peers))
        .route("/api/vpn/wireguard/peers/add", post(api::wireguard::add_peer))
        .route("/api/vpn/wireguard/peers/remove", post(api::wireguard::remove_peer))
        // VLANs / multiple networks
        .route("/api/vlan/list", get(api::vlan::list))
        .route("/api/vlan/add", post(api::vlan::add))
        .route("/api/vlan/remove", post(api::vlan::remove))
        // Guest network
        .route("/api/guest/status", get(api::guest::status))
        .route("/api/guest/config", post(api::guest::set_config))
        // QoS / per-client bandwidth
        .route("/api/qos/status", get(api::qos::status))
        .route("/api/qos/config", get(api::qos::get_config).post(api::qos::set_config))
        .route("/api/qos/apply", post(api::qos::apply))
        .route("/api/qos/clear", post(api::qos::clear))
        // Dynamic DNS
        .route("/api/ddns/config", get(api::ddns::get_config).post(api::ddns::set_config))
        .route("/api/ddns/status", get(api::ddns::status))
        .route("/api/ddns/update", post(api::ddns::update_now))
        // UPnP / NAT-PMP
        .route("/api/upnp/status", get(api::upnp::status))
        .route("/api/upnp/mappings", get(api::upnp::mappings))
        .route("/api/upnp/enable", post(api::upnp::enable))
        .route("/api/upnp/disable", post(api::upnp::disable))
        // Dual-WAN failover
        .route("/api/failover/status", get(api::failover::status))
        .route("/api/failover/config", post(api::failover::set_config))
        // Per-client traffic insight (L1 usage, L2 flows, L3 rate, L4 domains)
        .route("/api/traffic/clients", get(api::traffic::clients))
        .route("/api/traffic/connections", get(api::traffic::connections))
        .route("/api/traffic/domains", get(api::traffic::domains))
        .route("/api/traffic/settings", get(api::traffic::settings).post(api::traffic::set_settings))
        .route("/api/traffic/reset", post(api::traffic::reset))
        // Tools - Traffic Monitor
        .route("/api/tools/traffic", get(api::tools::traffic_stats))
        // Tools - Diagnostics
        .route("/api/tools/ping", post(api::tools::ping))
        .route("/api/tools/traceroute", post(api::tools::traceroute))
        .route("/api/tools/dns-lookup", post(api::tools::dns_lookup))
        .route("/api/tools/speed-test", post(api::tools::speed_test))
        // Tools - System Logs
        .route("/api/tools/logs", post(api::tools::logs))
        .route("/api/tools/logs/units", get(api::tools::log_units))
        // Tools - Backup/Restore
        .route("/api/tools/backup/create", post(api::tools::create_backup))
        .route("/api/tools/backup/list", get(api::tools::list_backups))
        .route("/api/tools/backup/download", post(api::tools::download_backup))
        .route("/api/tools/backup/restore", post(api::tools::restore_backup))
        .route("/api/tools/backup/delete", post(api::tools::delete_backup))
        // Security Monitor
        .route("/api/security/overview", get(api::security::overview))
        .route("/api/security/feed", get(api::security::live_feed))
        .route("/api/security/connections", get(api::security::connections))
        // Media Center
        .route("/api/media/overview", get(api::media::overview))
        // Built front-end assets (hashed JS/CSS). Real files, correct types.
        .nest_service("/_app", ServeDir::new(format!("{}/_app", frontend_dir)))
        // Middleware. The auth gate runs first (outermost of these three) and
        // rejects unauthenticated requests to protected routes before any
        // handler is reached.
        .layer(middleware::from_fn_with_state(state.clone(), auth_gate))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
        // Any non-API, non-asset path returns index.html with 200, so the
        // single-page app owns routing and deep links / refreshes (e.g.
        // /login, /firewall) work instead of returning 404.
        .fallback(spa_index);

    let port = std::env::var("ROUTERUI_PORT").unwrap_or_else(|_| "3080".to_string());
    let bind = std::env::var("ROUTERUI_BIND").unwrap_or_else(|_| "0.0.0.0".to_string());
    let addr = format!("{}:{}", bind, port);
    tracing::info!("Starting RouterUI on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
