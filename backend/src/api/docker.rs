use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::mock;

// ============ DATA STRUCTURES ============

#[derive(Debug, Serialize)]
pub struct DockerStatus {
    pub installed: bool,
    pub running: bool,
    pub version: String,
    pub containers_running: u32,
    pub containers_stopped: u32,
    pub images_count: u32,
    pub volumes_count: u32,
}

#[derive(Debug, Serialize)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub ports: Vec<String>,
    pub created: String,
    pub cpu_percent: Option<f64>,
    pub memory_usage: Option<String>,
    pub memory_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Image {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
    pub created: String,
}

#[derive(Debug, Serialize)]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
}

#[derive(Debug, Serialize)]
pub struct Network {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
}

#[derive(Debug, Deserialize)]
pub struct ContainerAction {
    pub id: String,
    pub action: String, // start, stop, restart, remove, pause, unpause
}

#[derive(Debug, Deserialize)]
pub struct ContainerLogsRequest {
    pub id: String,
    pub lines: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ContainerLogs {
    pub id: String,
    pub logs: String,
}

#[derive(Debug, Deserialize)]
pub struct ImageAction {
    pub id: String,
    pub action: String, // remove
}

#[derive(Debug, Deserialize)]
pub struct PullImage {
    pub image: String,
}

// ============ HELPER FUNCTIONS ============

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn parse_docker_json<T: for<'de> Deserialize<'de>>(json_str: &str) -> Option<T> {
    serde_json::from_str(json_str).ok()
}

// ============ API ENDPOINTS ============

pub async fn status() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::docker::status()));
    }

    let installed = Command::new("which")
        .args(["docker"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !installed {
        return Ok(Json(serde_json::to_value(DockerStatus {
            installed: false,
            running: false,
            version: String::new(),
            containers_running: 0,
            containers_stopped: 0,
            images_count: 0,
            volumes_count: 0,
        }).unwrap()));
    }

    let running = docker_available();

    // Get version
    let version = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // Get container counts
    let containers_running = Command::new("docker")
        .args(["ps", "-q"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count() as u32)
        .unwrap_or(0);

    let containers_all = Command::new("docker")
        .args(["ps", "-aq"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count() as u32)
        .unwrap_or(0);

    let containers_stopped = containers_all.saturating_sub(containers_running);

    // Get image count
    let images_count = Command::new("docker")
        .args(["images", "-q"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count() as u32)
        .unwrap_or(0);

    // Get volume count
    let volumes_count = Command::new("docker")
        .args(["volume", "ls", "-q"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count() as u32)
        .unwrap_or(0);

    Ok(Json(serde_json::to_value(DockerStatus {
        installed,
        running,
        version,
        containers_running,
        containers_stopped,
        images_count,
        volumes_count,
    }).unwrap()))
}

pub async fn containers() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::docker::containers()));
    }
    if !docker_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Docker is not running".to_string()));
    }

    // Get container list with stats
    let output = Command::new("docker")
        .args(["ps", "-a", "--format", "{{json .}}"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut containers = Vec::new();

    // Get stats for running containers
    let stats_output = Command::new("docker")
        .args(["stats", "--no-stream", "--format", "{{json .}}"])
        .output()
        .ok();

    let mut stats_map: std::collections::HashMap<String, (f64, String, f64)> = std::collections::HashMap::new();
    if let Some(stats_out) = stats_output {
        let stats_text = String::from_utf8_lossy(&stats_out.stdout);
        for line in stats_text.lines() {
            if let Ok(stat) = serde_json::from_str::<serde_json::Value>(line) {
                let id = stat["ID"].as_str().unwrap_or("").to_string();
                let cpu = stat["CPUPerc"].as_str()
                    .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
                    .unwrap_or(0.0);
                let mem = stat["MemUsage"].as_str().unwrap_or("").to_string();
                let mem_perc = stat["MemPerc"].as_str()
                    .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
                    .unwrap_or(0.0);
                stats_map.insert(id, (cpu, mem, mem_perc));
            }
        }
    }

    for line in text.lines() {
        if line.is_empty() {
            continue;
        }

        if let Ok(container) = serde_json::from_str::<serde_json::Value>(line) {
            let id = container["ID"].as_str().unwrap_or("").to_string();
            let name = container["Names"].as_str().unwrap_or("").to_string();
            let image = container["Image"].as_str().unwrap_or("").to_string();
            let status = container["Status"].as_str().unwrap_or("").to_string();
            let state = container["State"].as_str().unwrap_or("").to_string();
            let ports = container["Ports"].as_str().unwrap_or("").to_string();
            let created = container["CreatedAt"].as_str().unwrap_or("").to_string();

            let ports_vec: Vec<String> = if ports.is_empty() {
                vec![]
            } else {
                ports.split(',').map(|s| s.trim().to_string()).collect()
            };

            let (cpu, mem, mem_perc) = stats_map.get(&id).cloned().unwrap_or((0.0, String::new(), 0.0));

            containers.push(Container {
                id,
                name,
                image,
                status,
                state,
                ports: ports_vec,
                created,
                cpu_percent: if cpu > 0.0 { Some(cpu) } else { None },
                memory_usage: if !mem.is_empty() { Some(mem) } else { None },
                memory_percent: if mem_perc > 0.0 { Some(mem_perc) } else { None },
            });
        }
    }

    Ok(Json(serde_json::to_value(containers).unwrap()))
}

pub async fn container_action(
    Json(payload): Json<ContainerAction>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "success": true,
            "action": payload.action,
            "container": payload.id,
            "mock": true
        })));
    }

    if !docker_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Docker is not running".to_string()));
    }

    let action = match payload.action.as_str() {
        "start" | "stop" | "restart" | "pause" | "unpause" => payload.action.as_str(),
        "remove" => "rm",
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid action".to_string())),
    };

    // Validate container ID
    if !payload.id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err((StatusCode::BAD_REQUEST, "Invalid container ID".to_string()));
    }

    let mut args = vec![action];
    if action == "rm" {
        args.push("-f"); // Force remove
    }
    args.push(&payload.id);

    let output = Command::new("docker")
        .args(&args)
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "action": payload.action,
        "container": payload.id
    })))
}

pub async fn container_logs(
    Json(payload): Json<ContainerLogsRequest>,
) -> Result<Json<ContainerLogs>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(ContainerLogs {
            id: payload.id,
            logs: "2026-01-18T10:00:00Z Mock container started\n2026-01-18T10:00:01Z Running...\n".to_string(),
        }));
    }

    if !docker_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Docker is not running".to_string()));
    }

    // Validate container ID
    if !payload.id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err((StatusCode::BAD_REQUEST, "Invalid container ID".to_string()));
    }

    let lines = payload.lines.unwrap_or(100);
    let lines_str = lines.to_string();

    let output = Command::new("docker")
        .args(["logs", "--tail", &lines_str, "--timestamps", &payload.id])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Docker logs go to both stdout and stderr
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let logs = format!("{}{}", stdout, stderr);

    Ok(Json(ContainerLogs {
        id: payload.id,
        logs,
    }))
}

pub async fn images() -> Result<Json<Vec<Image>>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![
            Image { id: "abc123".to_string(), repository: "linuxserver/radarr".to_string(), tag: "latest".to_string(), size: "150MB".to_string(), created: "2 days ago".to_string() },
            Image { id: "def456".to_string(), repository: "linuxserver/sonarr".to_string(), tag: "latest".to_string(), size: "180MB".to_string(), created: "2 days ago".to_string() },
        ]));
    }

    if !docker_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Docker is not running".to_string()));
    }

    let output = Command::new("docker")
        .args(["images", "--format", "{{json .}}"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut images = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            continue;
        }

        if let Ok(image) = serde_json::from_str::<serde_json::Value>(line) {
            images.push(Image {
                id: image["ID"].as_str().unwrap_or("").to_string(),
                repository: image["Repository"].as_str().unwrap_or("").to_string(),
                tag: image["Tag"].as_str().unwrap_or("").to_string(),
                size: image["Size"].as_str().unwrap_or("").to_string(),
                created: image["CreatedSince"].as_str().unwrap_or("").to_string(),
            });
        }
    }

    Ok(Json(images))
}

pub async fn image_action(
    Json(payload): Json<ImageAction>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "success": true,
            "action": "remove",
            "image": payload.id,
            "mock": true
        })));
    }

    if !docker_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Docker is not running".to_string()));
    }

    if payload.action != "remove" {
        return Err((StatusCode::BAD_REQUEST, "Invalid action".to_string()));
    }

    // Validate image ID
    if !payload.id.chars().all(|c| c.is_alphanumeric() || c == ':' || c == '/' || c == '_' || c == '-' || c == '.') {
        return Err((StatusCode::BAD_REQUEST, "Invalid image ID".to_string()));
    }

    let output = Command::new("docker")
        .args(["rmi", "-f", &payload.id])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "action": "remove",
        "image": payload.id
    })))
}

pub async fn pull_image(
    Json(payload): Json<PullImage>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "success": true,
            "image": payload.image,
            "mock": true
        })));
    }

    if !docker_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Docker is not running".to_string()));
    }

    // Validate image name
    if !payload.image.chars().all(|c| c.is_alphanumeric() || c == ':' || c == '/' || c == '_' || c == '-' || c == '.') {
        return Err((StatusCode::BAD_REQUEST, "Invalid image name".to_string()));
    }

    // Note: This is a synchronous pull - for large images, might want to make async
    let output = Command::new("docker")
        .args(["pull", &payload.image])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "image": payload.image
    })))
}

pub async fn volumes() -> Result<Json<Vec<Volume>>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![
            Volume { name: "radarr_config".to_string(), driver: "local".to_string(), mountpoint: "/var/lib/docker/volumes/radarr_config/_data".to_string() },
        ]));
    }

    if !docker_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Docker is not running".to_string()));
    }

    let output = Command::new("docker")
        .args(["volume", "ls", "--format", "{{json .}}"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut volumes = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            continue;
        }

        if let Ok(vol) = serde_json::from_str::<serde_json::Value>(line) {
            volumes.push(Volume {
                name: vol["Name"].as_str().unwrap_or("").to_string(),
                driver: vol["Driver"].as_str().unwrap_or("").to_string(),
                mountpoint: vol["Mountpoint"].as_str().unwrap_or("").to_string(),
            });
        }
    }

    Ok(Json(volumes))
}

pub async fn networks() -> Result<Json<Vec<Network>>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![
            Network { id: "abc123".to_string(), name: "bridge".to_string(), driver: "bridge".to_string(), scope: "local".to_string() },
            Network { id: "def456".to_string(), name: "host".to_string(), driver: "host".to_string(), scope: "local".to_string() },
        ]));
    }

    if !docker_available() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Docker is not running".to_string()));
    }

    let output = Command::new("docker")
        .args(["network", "ls", "--format", "{{json .}}"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut networks = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            continue;
        }

        if let Ok(net) = serde_json::from_str::<serde_json::Value>(line) {
            networks.push(Network {
                id: net["ID"].as_str().unwrap_or("").to_string(),
                name: net["Name"].as_str().unwrap_or("").to_string(),
                driver: net["Driver"].as_str().unwrap_or("").to_string(),
                scope: net["Scope"].as_str().unwrap_or("").to_string(),
            });
        }
    }

    Ok(Json(networks))
}
