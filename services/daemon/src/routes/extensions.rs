//! Extension routes — manage plugins, MCP servers, commands, skills, hooks,
//! agents, rules, and LSP adapters.
//!
//! All routes delegate to `lochor_extensions::ExtensionRegistry` which owns
//! the in-memory state and permission bookkeeping. Persistence to SQLite is
//! planned for V1.1 (the registry already has the skeleton).
//!
//! Routes (API contract 06-api-contract.md):
//!   GET    /v1/extensions                     — list all installed
//!   POST   /v1/extensions/install             — install {source, scope}
//!   POST   /v1/extensions/{name}/enable       — enable by name
//!   POST   /v1/extensions/{name}/disable      — disable by name
//!   DELETE /v1/extensions/{name}              — uninstall by name
//!   GET    /v1/extensions/{name}/permissions  — list requested vs granted
//!   POST   /v1/extensions/{name}/permissions  — approve/deny one permission
//!   POST   /v1/extensions/reload              — hot-reload one or all

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use lochor_extensions::RegistryError;
use lochor_shared_types::{ExtensionScope, Permission};
use serde::Deserialize;
use std::sync::Arc;

use crate::DaemonState;

// ============================================================================
// Request bodies
// ============================================================================

#[derive(Deserialize)]
pub struct InstallBody {
    /// Filesystem path or download URL to the plugin directory.
    pub source: String,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "user".to_string()
}

#[derive(Deserialize)]
pub struct PermissionBody {
    pub permission: String,
    pub granted: bool,
}

// ============================================================================
// Helpers
// ============================================================================

fn parse_scope(s: &str) -> Result<ExtensionScope, &'static str> {
    match s {
        "global" => Ok(ExtensionScope::Global),
        "user" => Ok(ExtensionScope::User),
        "workspace" => Ok(ExtensionScope::Workspace),
        "session" => Ok(ExtensionScope::Session),
        _ => Err("scope must be 'global', 'user', 'workspace', or 'session'"),
    }
}

fn parse_permission(s: &str) -> Result<Permission, &'static str> {
    match s {
        "shell" => Ok(Permission::Shell),
        "files.read" => Ok(Permission::FilesRead),
        "files.write" => Ok(Permission::FilesWrite),
        "network" => Ok(Permission::Network),
        "extensions" => Ok(Permission::Extensions),
        "mcp" => Ok(Permission::Mcp),
        "preview" => Ok(Permission::Preview),
        "lsp" => Ok(Permission::Lsp),
        "env" => Ok(Permission::Env),
        _ => Err("unknown permission — see API contract for valid values"),
    }
}

fn registry_error_response(e: RegistryError) -> (StatusCode, Json<serde_json::Value>) {
    let (code, status) = match &e {
        RegistryError::NotFound(_) => ("not_found", StatusCode::NOT_FOUND),
        RegistryError::AlreadyInstalled(_) => ("conflict", StatusCode::CONFLICT),
        RegistryError::PermissionDenied(_) => ("permission_denied", StatusCode::FORBIDDEN),
        RegistryError::Manifest(_) | RegistryError::Io(_) => {
            ("install_error", StatusCode::UNPROCESSABLE_ENTITY)
        }
    };
    (
        status,
        Json(serde_json::json!({
            "error": { "code": code, "message": e.to_string() }
        })),
    )
}

fn entry_to_json(e: &lochor_extensions::ExtensionEntry) -> serde_json::Value {
    serde_json::json!({
        "id": e.id,
        "name": e.name,
        "version": e.version,
        "api_version": e.api_version,
        "kind": e.kind,
        "scope": e.scope,
        "manifest_path": e.manifest_path,
        "enabled": e.enabled,
        "permissions": {
            "requested": e.permissions.requested.iter().map(|(p, r)| {
                serde_json::json!({
                    "permission": format!("{p:?}"),
                    "reason": r.reason,
                    "scope": r.scope,
                    "require_approval": r.require_approval,
                })
            }).collect::<Vec<_>>(),
            "granted": e.permissions.granted.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
        }
    })
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /v1/extensions — list all installed extensions.
pub async fn list_extensions(
    State(s): State<Arc<DaemonState>>,
) -> Response {
    let entries = s.extensions.list();
    let json: Vec<serde_json::Value> = entries.iter().map(entry_to_json).collect();
    (StatusCode::OK, Json(json)).into_response()
}

/// POST /v1/extensions/install — install an extension from a local path.
///
/// Body: `{ source: "/path/to/plugin/dir", scope: "user" | "workspace" | "global" }`
/// URL-based install is not yet implemented (V1.1).
pub async fn install_extension(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<InstallBody>,
) -> Response {
    let scope = match parse_scope(&body.scope) {
        Ok(s) => s,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": msg }
                })),
            )
                .into_response();
        }
    };

    let dir = std::path::Path::new(&body.source);
    if !dir.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": { "code": "bad_request", "message": "source path does not exist or is not a directory" }
            })),
        )
            .into_response();
    }

    match s.extensions.install_from_dir(dir, scope) {
        Ok(entry) => (StatusCode::CREATED, Json(entry_to_json(&entry))).into_response(),
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// POST /v1/extensions/{name}/enable — enable an extension by name.
pub async fn enable_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.enable(&name) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "enabled", "name": name })),
        )
            .into_response(),
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// POST /v1/extensions/{name}/disable — disable an extension by name.
pub async fn disable_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.disable(&name) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "disabled", "name": name })),
        )
            .into_response(),
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// DELETE /v1/extensions/{name} — uninstall (remove) an extension by name.
pub async fn remove_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.remove(&name) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "removed", "name": name })),
        )
            .into_response(),
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// GET /v1/extensions/{name}/permissions — list requested vs granted permissions.
pub async fn get_extension_permissions(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.get(&name) {
        Some(entry) => {
            let json = serde_json::json!({
                "name": entry.name,
                "permissions": {
                    "requested": entry.permissions.requested.iter().map(|(p, r)| {
                        serde_json::json!({
                            "permission": format!("{p:?}"),
                            "reason": r.reason,
                            "scope": r.scope,
                            "require_approval": r.require_approval,
                        })
                    }).collect::<Vec<_>>(),
                    "granted": entry.permissions.granted.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
                }
            });
            (StatusCode::OK, Json(json)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": format!("extension not found: {name}") }
            })),
        )
            .into_response(),
    }
}

/// POST /v1/extensions/{name}/permissions — approve or deny a specific permission.
///
/// Body: `{ permission: "files.write", granted: true }`
pub async fn set_extension_permission(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
    Json(body): Json<PermissionBody>,
) -> Response {
    let perm = match parse_permission(&body.permission) {
        Ok(p) => p,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": msg }
                })),
            )
                .into_response();
        }
    };

    if body.granted {
        match s.extensions.grant_permission(&name, perm) {
            Ok(()) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "granted",
                    "name": name,
                    "permission": body.permission,
                })),
            )
                .into_response(),
            Err(e) => registry_error_response(e).into_response(),
        }
    } else {
        // Denying a permission is a no-op in the current registry (permissions
        // are addition-only). We still return OK for API consistency.
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "denied",
                "name": name,
                "permission": body.permission,
            })),
        )
            .into_response()
    }
}

/// POST /v1/extensions/reload — hot-reload one extension by name in the body,
/// or all extensions if no name is provided.
///
/// Body (optional): `{ name: "my-plugin" }`
pub async fn reload_extensions(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        match s.extensions.reload(name) {
            Ok(entry) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "reloaded",
                    "name": name,
                    "entry": entry_to_json(&entry),
                })),
            )
                .into_response(),
            Err(e) => registry_error_response(e).into_response(),
        }
    } else {
        // Reload all: iterate through entries, re-read each manifest.
        let names: Vec<String> = s.extensions.list().into_iter().map(|e| e.name).collect();
        let mut results: Vec<serde_json::Value> = Vec::new();
        for name in &names {
            match s.extensions.reload(name) {
                Ok(_entry) => results.push(serde_json::json!({
                    "name": name, "status": "reloaded"
                })),
                Err(e) => results.push(serde_json::json!({
                    "name": name, "status": "error", "error": e.to_string()
                })),
            }
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "completed", "results": results })),
        )
            .into_response()
    }
}
