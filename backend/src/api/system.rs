use axum::{http::StatusCode, Json};

use crate::mock;
use crate::system;
use super::AuthUser;

pub async fn status(
    AuthUser(_user): AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::system::status()));
    }

    system::get_system_status()
        .map(|s| Json(serde_json::to_value(s).unwrap()))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn interfaces(
    AuthUser(_user): AuthUser,
) -> Result<Json<Vec<system::NetworkInterface>>, (StatusCode, String)> {
    system::get_interfaces()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn services(
    AuthUser(_user): AuthUser,
) -> Result<Json<Vec<system::ServiceStatus>>, (StatusCode, String)> {
    system::get_services()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

use serde::Serialize;
use std::process::Command;

#[derive(Serialize)]
pub struct UpdateCheckResult {
    pub output: String,
    pub updates: Vec<String>,
}

#[derive(Serialize)]
pub struct UpdateInstallResult {
    pub output: String,
    pub success: bool,
}

pub async fn check_updates(
    AuthUser(_user): AuthUser,
) -> Result<Json<UpdateCheckResult>, (StatusCode, String)> {
    // Run apt update and list upgradable packages
    let update_output = Command::new("sudo")
        .args(["apt", "update"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let list_output = Command::new("apt")
        .args(["list", "--upgradable"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let update_str = String::from_utf8_lossy(&update_output.stdout).to_string()
        + &String::from_utf8_lossy(&update_output.stderr).to_string();
    let list_str = String::from_utf8_lossy(&list_output.stdout).to_string();
    
    let updates: Vec<String> = list_str
        .lines()
        .filter(|line| line.contains("upgradable"))
        .map(|s| s.to_string())
        .collect();
    
    let output = format!("=== Checking for updates ===\n{}\n\n=== Available updates ===\n{}", 
        update_str, list_str);
    
    Ok(Json(UpdateCheckResult { output, updates }))
}

pub async fn install_updates(
    AuthUser(_user): AuthUser,
) -> Result<Json<UpdateInstallResult>, (StatusCode, String)> {
    // Run apt upgrade with -y flag
    let output = Command::new("sudo")
        .args(["apt", "upgrade", "-y"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    
    let full_output = format!("=== Installing updates ===\n{}\n{}\n\n=== Update complete ===", 
        stdout, stderr);
    
    Ok(Json(UpdateInstallResult { 
        output: full_output, 
        success: output.status.success() 
    }))
}
