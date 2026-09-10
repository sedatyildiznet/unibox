mod native_runtime;

use native_runtime::NativeRuntimeManager;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
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

fn repair_telegram_root_credentials(data_root: &str) -> Result<(), String> {
    let connector_root = PathBuf::from(data_root).join("connectors").join("telegram");
    let settings_path = connector_root.join("unibox-settings.json");
    let config_path = connector_root.join("config.yaml");

    let settings_raw = fs::read_to_string(&settings_path)
        .map_err(|error| format!("failed to read Telegram local settings: {error}"))?;
    let settings: Value = serde_json::from_str(&settings_raw)
        .map_err(|error| format!("invalid Telegram local settings: {error}"))?;
    let api_id = settings
        .get("api_id")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or_else(|| "Telegram API ID is not configured".to_string())?;
    let api_hash = settings
        .get("api_hash")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.len() == 32 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
        .ok_or_else(|| "Telegram API hash is not configured".to_string())?;

    let config_raw = fs::read_to_string(&config_path)
        .map_err(|error| format!("failed to read Telegram config: {error}"))?;
    let mut config: serde_yaml::Value = serde_yaml::from_str(&config_raw)
        .map_err(|error| format!("invalid Telegram config: {error}"))?;
    let root = config
        .as_mapping_mut()
        .ok_or_else(|| "Telegram config root is not a mapping".to_string())?;
    root.insert(
        serde_yaml::Value::String("api_id".to_string()),
        serde_yaml::Value::Number(api_id.into()),
    );
    root.insert(
        serde_yaml::Value::String("api_hash".to_string()),
        serde_yaml::Value::String(api_hash.to_string()),
    );
    fs::write(&config_path, serde_yaml::to_string(&config).map_err(|error| error.to_string())?)
        .map_err(|error| format!("failed to write Telegram config: {error}"))?;
    Ok(())
}

#[tauri::command]
fn connector_registry(state: State<'_, AppState>) -> Vec<ConnectorDefinition> {
    state
        .registry
        .iter()
        .filter(|connector| state.runtime.connector_available(connector))
        .cloned()
        .collect()
}

#[tauri::command]
fn connector_requirements(id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let connector = state.connector(&id)?;
    Ok(state.runtime.connector_requirements(&connector))
}

#[tauri::command]
fn connector_configure(
    id: String,
    settings: Value,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let connector = state.connector(&id)?;
    state
        .runtime
        .configure_connector(&connector, settings)
        .map_err(|error| error.to_string())
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

    let _ = state.runtime.stop_connector(&connector);
    let mut last_error = String::new();
    let mut telegram_repaired = false;
    for attempt in 0..6u64 {
        match state.runtime.install_connector(&connector).await {
            Ok(message) => return Ok(message),
            Err(error) => {
                let message = error.to_string();
                let lower = message.to_ascii_lowercase();

                if connector.id == "telegram"
                    && !telegram_repaired
                    && (lower.contains("api_hash is required") || lower.contains("api_id is required"))
                {
                    let runtime = state.runtime.status().await;
                    repair_telegram_root_credentials(&runtime.data_root)?;
                    telegram_repaired = true;
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    continue;
                }

                let transient_lock = lower.contains("being used by another process")
                    || lower.contains("process cannot access the file")
                    || lower.contains("os error 32")
                    || lower.contains("sharing violation");
                if !transient_lock {
                    return Err(message);
                }
                last_error = message;
                tokio::time::sleep(Duration::from_millis(300 * (attempt + 1))).await;
            }
        }
    }

    Err(format!(
        "{} connector setup could not complete after automatic retries. Details: {}",
        connector.name, last_error
    ))
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
            let resource_root = normalize_windows_path(
                app.path()
                    .resource_dir()?
                    .join("resources")
                    .join("native-v034"),
            );
            let runtime = NativeRuntimeManager::new(data_root, resource_root)?;
            let registry = parse_registry(REGISTRY_RAW)?;
            app.manage(AppState { runtime, registry });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connector_registry,
            connector_requirements,
            connector_configure,
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
