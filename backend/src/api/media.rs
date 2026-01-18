use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::mock;
use super::AuthUser;

// Config - these could be moved to a config file later
const MEDIA_PATH: &str = "/mnt/external/media1/media";
const RADARR_URL: &str = "http://localhost:7878";
const RADARR_API_KEY: &str = "66fc15a8af02444bb787e5f4d9e585b4";
const SONARR_URL: &str = "http://localhost:8989";
const SONARR_API_KEY: &str = "e3f602d269a349dabfc9e9a3ac995f76";
const JELLYFIN_URL: &str = "http://10.22.22.185:8096";
const JELLYFIN_API_KEY: &str = "72972c09f8794beab6da4af991cff9a3";

#[derive(Debug, Serialize)]
pub struct MediaOverview {
    pub storage: StorageInfo,
    pub library: LibraryCounts,
    pub recent_movies: Vec<MediaItem>,
    pub recent_shows: Vec<MediaItem>,
    pub jellyfin: Option<JellyfinStats>,
}

#[derive(Debug, Serialize)]
pub struct JellyfinStats {
    pub movie_count: u64,
    pub series_count: u64,
    pub episode_count: u64,
    pub active_streams: u64,
    pub server_name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct StorageInfo {
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub percent_used: f64,
    pub mount_point: String,
}

#[derive(Debug, Serialize)]
pub struct LibraryCounts {
    pub movies: u64,
    pub tv_shows: u64,
}

#[derive(Debug, Serialize)]
pub struct MediaItem {
    pub title: String,
    pub date: String,
    pub status: String,
    pub quality: String,
    pub size_mb: u64,
}

pub async fn overview(
    AuthUser(_user): AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::media::overview()));
    }

    let storage = get_storage_info();
    let library = get_library_counts();
    let recent_movies = get_recent_movies().await;
    let recent_shows = get_recent_shows().await;
    let jellyfin = get_jellyfin_stats().await;

    Ok(Json(serde_json::to_value(MediaOverview {
        storage,
        library,
        recent_movies,
        recent_shows,
        jellyfin,
    }).unwrap()))
}

async fn get_jellyfin_stats() -> Option<JellyfinStats> {
    let client = reqwest::Client::new();

    // Get system info
    let system_url = format!("{}/System/Info?api_key={}", JELLYFIN_URL, JELLYFIN_API_KEY);
    let system_info: Option<JellyfinSystemInfo> = client.get(&system_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok();

    // Get library counts
    let counts_url = format!("{}/Items/Counts?api_key={}", JELLYFIN_URL, JELLYFIN_API_KEY);
    let counts: Option<JellyfinCounts> = client.get(&counts_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok();

    // Get active sessions
    let sessions_url = format!("{}/Sessions?api_key={}", JELLYFIN_URL, JELLYFIN_API_KEY);
    let sessions: Vec<JellyfinSession> = client.get(&sessions_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .json()
        .await
        .unwrap_or_default();

    let active_streams = sessions.iter()
        .filter(|s| s.now_playing_item.is_some())
        .count() as u64;

    Some(JellyfinStats {
        movie_count: counts.as_ref().map(|c| c.movie_count).unwrap_or(0),
        series_count: counts.as_ref().map(|c| c.series_count).unwrap_or(0),
        episode_count: counts.as_ref().map(|c| c.episode_count).unwrap_or(0),
        active_streams,
        server_name: system_info.as_ref().map(|s| s.server_name.clone()).unwrap_or_default(),
        version: system_info.as_ref().map(|s| s.version.clone()).unwrap_or_default(),
    })
}

#[derive(Debug, Deserialize)]
struct JellyfinSystemInfo {
    #[serde(rename = "ServerName")]
    server_name: String,
    #[serde(rename = "Version")]
    version: String,
}

#[derive(Debug, Deserialize)]
struct JellyfinCounts {
    #[serde(rename = "MovieCount")]
    movie_count: u64,
    #[serde(rename = "SeriesCount")]
    series_count: u64,
    #[serde(rename = "EpisodeCount")]
    episode_count: u64,
}

#[derive(Debug, Deserialize)]
struct JellyfinSession {
    #[serde(rename = "NowPlayingItem")]
    now_playing_item: Option<serde_json::Value>,
}

fn get_storage_info() -> StorageInfo {
    let output = Command::new("df")
        .args(["-B1", "/mnt/external"])
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                let total: u64 = parts[1].parse().unwrap_or(0);
                let used: u64 = parts[2].parse().unwrap_or(0);
                let free: u64 = parts[3].parse().unwrap_or(0);
                let percent: f64 = parts[4].trim_end_matches('%').parse().unwrap_or(0.0);

                return StorageInfo {
                    total_gb: total as f64 / 1_073_741_824.0,
                    used_gb: used as f64 / 1_073_741_824.0,
                    free_gb: free as f64 / 1_073_741_824.0,
                    percent_used: percent,
                    mount_point: parts[5].to_string(),
                };
            }
        }
    }

    StorageInfo {
        total_gb: 0.0,
        used_gb: 0.0,
        free_gb: 0.0,
        percent_used: 0.0,
        mount_point: "unknown".to_string(),
    }
}

fn get_library_counts() -> LibraryCounts {
    let movies = Command::new("ls")
        .args(["-1", &format!("{}/movies", MEDIA_PATH)])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count() as u64)
        .unwrap_or(0);

    let tv_shows = Command::new("ls")
        .args(["-1", &format!("{}/shows", MEDIA_PATH)])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count() as u64)
        .unwrap_or(0);

    LibraryCounts { movies, tv_shows }
}

async fn get_recent_movies() -> Vec<MediaItem> {
    // Try Radarr API first
    let url = format!("{}/api/v3/history?pageSize=10&sortKey=date&sortDirection=descending&apikey={}",
        RADARR_URL, RADARR_API_KEY);

    if let Ok(resp) = reqwest::get(&url).await {
        if let Ok(data) = resp.json::<RadarrHistoryResponse>().await {
            return data.records.into_iter()
                .filter(|r| r.event_type == "downloadFolderImported" || r.event_type == "grabbed")
                .take(10)
                .map(|r| MediaItem {
                    title: r.movie.as_ref().map(|m| m.title.clone()).unwrap_or_else(|| r.source_title.clone()),
                    date: r.date.chars().take(10).collect(),
                    status: match r.event_type.as_str() {
                        "downloadFolderImported" => "Imported".to_string(),
                        "grabbed" => "Downloading".to_string(),
                        _ => r.event_type,
                    },
                    quality: r.quality.as_ref().map(|q| q.quality.name.clone()).unwrap_or_default(),
                    size_mb: 0,
                })
                .collect();
        }
    }

    // Fallback: get recent files from filesystem
    get_recent_files_from_fs("movies")
}

async fn get_recent_shows() -> Vec<MediaItem> {
    // Try Sonarr API first
    let url = format!("{}/api/v3/history?pageSize=10&sortKey=date&sortDirection=descending&apikey={}",
        SONARR_URL, SONARR_API_KEY);

    if let Ok(resp) = reqwest::get(&url).await {
        if let Ok(data) = resp.json::<SonarrHistoryResponse>().await {
            return data.records.into_iter()
                .filter(|r| r.event_type == "downloadFolderImported" || r.event_type == "grabbed")
                .take(10)
                .map(|r| {
                    let title = if let Some(series) = &r.series {
                        if let (Some(s), Some(e)) = (r.episode.as_ref().and_then(|ep| Some(ep.season_number)),
                                                      r.episode.as_ref().and_then(|ep| Some(ep.episode_number))) {
                            format!("{} S{:02}E{:02}", series.title, s, e)
                        } else {
                            series.title.clone()
                        }
                    } else {
                        r.source_title.clone()
                    };

                    MediaItem {
                        title,
                        date: r.date.chars().take(10).collect(),
                        status: match r.event_type.as_str() {
                            "downloadFolderImported" => "Imported".to_string(),
                            "grabbed" => "Downloading".to_string(),
                            _ => r.event_type,
                        },
                        quality: r.quality.as_ref().map(|q| q.quality.name.clone()).unwrap_or_default(),
                        size_mb: 0,
                    }
                })
                .collect();
        }
    }

    // Fallback: get recent files from filesystem
    get_recent_files_from_fs("shows")
}

fn get_recent_files_from_fs(folder: &str) -> Vec<MediaItem> {
    let output = Command::new("ls")
        .args(["-lt", "--time-style=+%Y-%m-%d", &format!("{}/{}", MEDIA_PATH, folder)])
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        return text.lines()
            .skip(1) // skip total line
            .take(10)
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(7, ' ').collect();
                if parts.len() >= 7 {
                    Some(MediaItem {
                        title: parts[6].to_string(),
                        date: parts[5].to_string(),
                        status: "On Disk".to_string(),
                        quality: String::new(),
                        size_mb: 0,
                    })
                } else {
                    None
                }
            })
            .collect();
    }

    Vec::new()
}

// Radarr API response structures
#[derive(Debug, Deserialize)]
struct RadarrHistoryResponse {
    records: Vec<RadarrHistoryRecord>,
}

#[derive(Debug, Deserialize)]
struct RadarrHistoryRecord {
    #[serde(rename = "eventType")]
    event_type: String,
    #[serde(rename = "sourceTitle")]
    source_title: String,
    date: String,
    quality: Option<QualityWrapper>,
    movie: Option<RadarrMovie>,
}

#[derive(Debug, Deserialize)]
struct RadarrMovie {
    title: String,
}

// Sonarr API response structures
#[derive(Debug, Deserialize)]
struct SonarrHistoryResponse {
    records: Vec<SonarrHistoryRecord>,
}

#[derive(Debug, Deserialize)]
struct SonarrHistoryRecord {
    #[serde(rename = "eventType")]
    event_type: String,
    #[serde(rename = "sourceTitle")]
    source_title: String,
    date: String,
    quality: Option<QualityWrapper>,
    series: Option<SonarrSeries>,
    episode: Option<SonarrEpisode>,
}

#[derive(Debug, Deserialize)]
struct SonarrSeries {
    title: String,
}

#[derive(Debug, Deserialize)]
struct SonarrEpisode {
    #[serde(rename = "seasonNumber")]
    season_number: u32,
    #[serde(rename = "episodeNumber")]
    episode_number: u32,
}

#[derive(Debug, Deserialize)]
struct QualityWrapper {
    quality: QualityInfo,
}

#[derive(Debug, Deserialize)]
struct QualityInfo {
    name: String,
}

// ============ JELLYFIN NOTIFICATION SETUP ============

#[derive(Debug, Serialize)]
pub struct NotificationStatus {
    pub radarr_configured: bool,
    pub sonarr_configured: bool,
    pub radarr_notification_id: Option<i64>,
    pub sonarr_notification_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ArrNotification {
    id: i64,
    name: String,
    implementation: String,
}

// Check if Jellyfin notifications are configured in Radarr/Sonarr
pub async fn check_jellyfin_notifications(
    _user: AuthUser,
) -> Result<Json<NotificationStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(NotificationStatus {
            radarr_configured: true,
            sonarr_configured: true,
            radarr_notification_id: Some(1),
            sonarr_notification_id: Some(1),
        }));
    }

    let client = reqwest::Client::new();

    // Check Radarr
    let radarr_url = format!("{}/api/v3/notification?apikey={}", RADARR_URL, RADARR_API_KEY);
    let radarr_notifications: Vec<ArrNotification> = client.get(&radarr_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Radarr connection failed: {}", e)))?
        .json()
        .await
        .unwrap_or_default();

    let radarr_jellyfin = radarr_notifications.iter()
        .find(|n| n.implementation == "Emby" || n.implementation == "Jellyfin");

    // Check Sonarr
    let sonarr_url = format!("{}/api/v3/notification?apikey={}", SONARR_URL, SONARR_API_KEY);
    let sonarr_notifications: Vec<ArrNotification> = client.get(&sonarr_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Sonarr connection failed: {}", e)))?
        .json()
        .await
        .unwrap_or_default();

    let sonarr_jellyfin = sonarr_notifications.iter()
        .find(|n| n.implementation == "Emby" || n.implementation == "Jellyfin");

    Ok(Json(NotificationStatus {
        radarr_configured: radarr_jellyfin.is_some(),
        sonarr_configured: sonarr_jellyfin.is_some(),
        radarr_notification_id: radarr_jellyfin.map(|n| n.id),
        sonarr_notification_id: sonarr_jellyfin.map(|n| n.id),
    }))
}

// Add Jellyfin notification to Radarr and Sonarr
pub async fn setup_jellyfin_notifications(
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let client = reqwest::Client::new();

    // Jellyfin notification payload for Radarr/Sonarr
    // Using "Emby" implementation which works for Jellyfin
    let notification_payload = serde_json::json!({
        "name": "Jellyfin",
        "implementation": "Emby",
        "configContract": "MediaBrowserSettings",
        "fields": [
            {"name": "host", "value": JELLYFIN_URL},
            {"name": "apiKey", "value": JELLYFIN_API_KEY},
            {"name": "sendNotifications", "value": false},
            {"name": "updateLibrary", "value": true}
        ],
        "onGrab": false,
        "onDownload": true,
        "onUpgrade": true,
        "onRename": true,
        "onMovieDelete": true,
        "onMovieFileDelete": true,
        "onMovieFileDeleteForUpgrade": true,
        "onSeriesDelete": true,
        "onEpisodeFileDelete": true,
        "onEpisodeFileDeleteForUpgrade": true,
        "includeHealthWarnings": false,
        "supportsOnGrab": true,
        "supportsOnDownload": true,
        "supportsOnUpgrade": true,
        "supportsOnRename": true
    });

    let mut results = serde_json::json!({
        "radarr": {"success": false, "message": ""},
        "sonarr": {"success": false, "message": ""}
    });

    // Add to Radarr
    let radarr_url = format!("{}/api/v3/notification?apikey={}", RADARR_URL, RADARR_API_KEY);
    match client.post(&radarr_url)
        .json(&notification_payload)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                results["radarr"]["success"] = serde_json::json!(true);
                results["radarr"]["message"] = serde_json::json!("Jellyfin notification added to Radarr");
            } else {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                results["radarr"]["message"] = serde_json::json!(format!("Failed: {} - {}", status, body));
            }
        }
        Err(e) => {
            results["radarr"]["message"] = serde_json::json!(format!("Connection error: {}", e));
        }
    }

    // Add to Sonarr
    let sonarr_url = format!("{}/api/v3/notification?apikey={}", SONARR_URL, SONARR_API_KEY);
    match client.post(&sonarr_url)
        .json(&notification_payload)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                results["sonarr"]["success"] = serde_json::json!(true);
                results["sonarr"]["message"] = serde_json::json!("Jellyfin notification added to Sonarr");
            } else {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                results["sonarr"]["message"] = serde_json::json!(format!("Failed: {} - {}", status, body));
            }
        }
        Err(e) => {
            results["sonarr"]["message"] = serde_json::json!(format!("Connection error: {}", e));
        }
    }

    Ok(Json(results))
}
