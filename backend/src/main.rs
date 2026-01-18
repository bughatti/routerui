mod api;
mod auth;
mod db;
mod mock;
mod models;
mod system;

use axum::{
    routing::{get, post},
    Router,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct AppState {
    pub db: sqlx::SqlitePool,
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

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let frontend_dir = std::env::var("FRONTEND_DIR")
        .unwrap_or_else(|_| "/opt/routerui/frontend/build".to_string());

    let app = Router::new()
        // Setup wizard routes (no auth required)
        .route("/api/setup/status", get(api::setup::status))
        .route("/api/setup/interfaces", get(api::setup::get_interfaces))
        .route("/api/setup/features", get(api::setup::get_features))
        .route("/api/setup/admin", post(api::setup::create_admin))
        .route("/api/setup/network", post(api::setup::save_network_config))
        .route("/api/setup/features/save", post(api::setup::save_features))
        .route("/api/setup/features/install", post(api::setup::install_feature))
        .route("/api/setup/complete", post(api::setup::complete))
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
        // Middleware
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
        .fallback_service(
            ServeDir::new(&frontend_dir)
                .not_found_service(ServeFile::new(format!("{}/index.html", frontend_dir)))
        );

    let port = std::env::var("ROUTERUI_PORT").unwrap_or_else(|_| "3080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting RouterUI on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
