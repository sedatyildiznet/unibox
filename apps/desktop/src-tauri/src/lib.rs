mod desktop;
use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use unibox_core::{
    parse_registry, BootstrapResult, ClientHttpRequest, ClientHttpResponse, ConnectorDefinition,
    ConnectorStatus, MatrixSession, RuntimeManager, RuntimeStatus,
};

const REGISTRY_RAW: &str = include_str!("../../../../registry/stable.json");

struct AppState {
    runtime: RuntimeManager,
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

#[tauri::command]
fn connector_registry(state: State<'_, AppState>) -> Vec<ConnectorDefinition> {
    state.registry.clone()
}

#[tauri::command]
async fn runtime_status(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || tauri::async_runtime::block_on(runtime.status()))
        .await
        .map_err(|_| "Unable to check the local engine.".to_string())
}

#[tauri::command]
async fn bootstrap_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<BootstrapResult, String> {
    let script = normalize_windows_path(
        app.path()
            .resource_dir()
            .map_err(|error| error.to_string())?
            .join("resources")
            .join("runtime")
            .join("bootstrap.ps1"),
    );
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || runtime.bootstrap(&script))
        .await
        .map_err(|_| "Local engine setup was interrupted. Please retry.".to_string())?
        .map_err(|_| "Local engine setup could not finish. Please retry.".to_string())
}

#[tauri::command]
async fn start_runtime(state: State<'_, AppState>) -> Result<String, String> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || runtime.start().map_err(|error| error.to_string()))
        .await
        .map_err(|_| "The local operation was interrupted.".to_string())?
}

#[tauri::command]
async fn stop_runtime(state: State<'_, AppState>) -> Result<String, String> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || runtime.stop().map_err(|error| error.to_string()))
        .await
        .map_err(|_| "The local operation was interrupted.".to_string())?
}

#[tauri::command]
async fn matrix_session(state: State<'_, AppState>) -> Result<MatrixSession, String> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runtime.matrix_session().map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The local operation was interrupted.".to_string())?
}

#[tauri::command]
async fn connector_status(
    id: String,
    state: State<'_, AppState>,
) -> Result<ConnectorStatus, String> {
    let connector = state.connector(&id)?;
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(runtime.connector_status(&connector)))
        .await
        .map_err(|_| "The local operation was interrupted.".to_string())?
}

#[tauri::command]
async fn connector_install(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let connector = state.connector(&id)?;
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runtime
            .install_connector(&connector)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The local operation was interrupted.".to_string())?
}

#[tauri::command]
async fn connector_start(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let connector = state.connector(&id)?;
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runtime
            .start_connector(&connector)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The local operation was interrupted.".to_string())?
}

#[tauri::command]
async fn connector_stop(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let connector = state.connector(&id)?;
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runtime
            .stop_connector(&connector)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The local operation was interrupted.".to_string())?
}

#[tauri::command]
async fn connector_update(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let connector = state.connector(&id)?;
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runtime
            .update_connector(&connector)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "The local operation was interrupted.".to_string())?
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

#[tauri::command]
async fn runtime_maintenance(
    app: AppHandle,
    operation: String,
    destination: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let script = normalize_windows_path(
        app.path()
            .resource_dir()
            .map_err(|_| "Application resources are unavailable.")?
            .join("resources/runtime/unibox-maintenance.py"),
    );
    let active = app
        .state::<desktop::DesktopState>()
        .maintenance_active
        .clone();
    if active.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Another maintenance operation is running.".to_string());
    }
    let runtime = state.runtime.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        runtime.maintenance(&script, &operation, &PathBuf::from(destination))
    })
    .await;
    active.store(false, std::sync::atomic::Ordering::SeqCst);
    result.map_err(|_| "Local maintenance was interrupted.".to_string())?
        .map_err(|_| "Maintenance could not finish. Check free space, the local engine, and backup version compatibility.".to_string())
}

#[tauri::command]
async fn export_diagnostics(destination: String, state: State<'_, AppState>) -> Result<(), String> {
    if !PathBuf::from(&destination).is_absolute() || !destination.ends_with(".json") {
        return Err("Choose a JSON report file.".to_string());
    }
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let status = tauri::async_runtime::block_on(runtime.status());
        let report = serde_json::json!({
            "schema": 1, "platform": status.platform,
            "wsl_available": status.wsl_available, "distro_installed": status.distro_installed,
            "distro_running": status.distro_running, "messaging_ready": status.synapse_ready,
            "session_ready": status.matrix_session_ready,
            "bootstrap_state": status.bootstrap.map(|value| value.state),
        });
        // Export only allowlisted checks. Never export raw logs, paths, tokens or sessions.
        std::fs::write(
            destination,
            serde_json::to_vec_pretty(&report).map_err(|_| "Unable to create report.")?,
        )
        .map_err(|_| "Unable to save report.".to_string())
    })
    .await
    .map_err(|_| "Diagnostic export was interrupted.".to_string())?
}

#[tauri::command]
async fn restart_windows(state: State<'_, AppState>) -> Result<(), String> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || runtime.restart_windows())
        .await
        .map_err(|_| "Windows restart was interrupted.".to_string())?
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
            let data_root =
                normalize_windows_path(app.path().app_local_data_dir()?.join("runtime"));
            let runtime = RuntimeManager::new(data_root)?;
            let registry = parse_registry(REGISTRY_RAW)?;
            app.manage(AppState { runtime, registry });
            desktop::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<desktop::DesktopState>();
                if state
                    .maintenance_active
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    api.prevent_close();
                    return;
                }
                let close_to_tray = state
                    .preferences
                    .lock()
                    .map(|value| value.close_to_tray)
                    .unwrap_or(false);
                if state.tray_available && close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            desktop::desktop_preferences,
            desktop::save_desktop_preferences,
            connector_registry,
            runtime_status,
            bootstrap_runtime,
            restart_windows,
            runtime_maintenance,
            export_diagnostics,
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

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn powershell_resource_paths_accept_spaces_and_unicode() {
        assert_eq!(
            normalize_windows_path(PathBuf::from(
                r"\\?\C:\Users\Şedat Yıldız\Unibox\bootstrap.ps1"
            )),
            PathBuf::from(r"C:\Users\Şedat Yıldız\Unibox\bootstrap.ps1")
        );
        assert_eq!(
            normalize_windows_path(PathBuf::from(r"\\?\UNC\server\share\bootstrap.ps1")),
            PathBuf::from(r"\\server\share\bootstrap.ps1")
        );
    }
}
