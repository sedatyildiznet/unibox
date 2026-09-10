mod native_runtime;

use native_runtime::NativeRuntimeManager;
use serde_json::Value;
use std::path::PathBuf;
use tauri::{Manager, State};
use unibox_core::{
    parse_registry, ClientHttpRequest, ClientHttpResponse, ConnectorDefinition, ConnectorStatus,
    MatrixSession, RuntimeStatus,
};

const REGISTRY_RAW: &str = include_str!("../../../../registry/stable.json");

struct AppState {
    runtime: NativeRuntimeManager,
    registry: Vec<ConnectorDefinition>,
}

impl AppState {
    fn connector(&self, id: &str) -> Result<ConnectorDefinition, String> {
        self.registry
            .iter()
            .find(|connector| connector.id == id)
            .cloned()
            .ok_or_else(|| format!("unknown connector: {id}"))
    }
}

fn is_exposed_native_connector(connector: &ConnectorDefinition) -> bool {
    matches!(connector.id.as_str(), "whatsapp" | "telegram")
}

#[cfg(target_os = "windows")]
fn normalize_windows_path(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

#[cfg(not(target_os = "windows"))]
fn normalize_windows_path(path: PathBuf) -> PathBuf {
    path
}

#[tauri::command]
fn connector_registry(state: State<'_, AppState>) -> Vec<ConnectorDefinition> {
    state
        .registry
        .iter()
        .filter(|connector| is_exposed_native_connector(connector))
        .cloned()
        .collect()
}

#[tauri::command]
async fn runtime_status(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    Ok(state.runtime.status().await)
}

#[tauri::command]
async fn bootstrap_runtime(state: State<'_, AppState>) -> Result<String, String> {
    state
        .runtime
        .bootstrap()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_runtime(state: State<'_, AppState>) -> Result<String, String> {
    state
        .runtime
        .start()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn stop_runtime(state: State<'_, AppState>) -> Result<String, String> {
    state.runtime.stop().map_err(|error| error.to_string())
}

#[tauri::command]
fn matrix_session(state: State<'_, AppState>) -> Result<MatrixSession, String> {
    state
        .runtime
        .matrix_session()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn connector_status(id: String, state: State<'_, AppState>) -> Result<ConnectorStatus, String> {
    let connector = state.connector(&id)?;
    Ok(state.runtime.connector_status(&connector))
}

#[tauri::command]
async fn connector_install(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let connector = state.connector(&id)?;
    state
        .runtime
        .install_connector(&connector)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn connector_start(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let connector = state.connector(&id)?;
    state
        .runtime
        .start_connector(&connector)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn connector_stop(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let connector = state.connector(&id)?;
    state
        .runtime
        .stop_connector(&connector)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn connector_update(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let connector = state.connector(&id)?;
    state
        .runtime
        .update_connector(&connector)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn connector_provision(
    id: String,
    method: String,
    path: String,
    body: Option<Value>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let connector = state.connector(&id)?;
    state
        .runtime
        .provision_request(&connector, &method, &path, body)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn connector_client_http(
    request: ClientHttpRequest,
    state: State<'_, AppState>,
) -> Result<ClientHttpResponse, String> {
    state
        .runtime
        .client_http(request)
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_root = normalize_windows_path(app.path().app_local_data_dir()?);
            let resource_root =
                normalize_windows_path(app.path().resource_dir()?.join("resources").join("native"));
            let runtime = NativeRuntimeManager::new(data_root, resource_root)?;
            let registry = parse_registry(REGISTRY_RAW)?;
            app.manage(AppState { runtime, registry });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connector_registry,
            runtime_status,
            bootstrap_runtime,
            start_runtime,
            stop_runtime,
            matrix_session,
            connector_status,
            connector_install,
            connector_start,
            connector_stop,
            connector_update,
            connector_provision,
            connector_client_http
        ])
        .run(tauri::generate_context!())
        .expect("error while running Unibox");
}
