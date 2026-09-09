use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub close_to_tray: bool,
    pub run_at_startup: bool,
    pub start_minimized: bool,
}

pub struct DesktopState {
    pub preferences: Mutex<Preferences>,
    pub tray_available: bool,
    pub maintenance_active: Arc<AtomicBool>,
}

fn path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|root| root.join("desktop-settings.json"))
        .map_err(|_| "Settings folder is unavailable.".to_string())
}

#[tauri::command]
pub fn desktop_preferences(state: tauri::State<'_, DesktopState>) -> Result<Preferences, String> {
    state
        .preferences
        .lock()
        .map(|value| value.clone())
        .map_err(|_| "Settings are unavailable.".to_string())
}

#[tauri::command]
pub fn save_desktop_preferences(
    app: AppHandle,
    preferences: Preferences,
    state: tauri::State<'_, DesktopState>,
) -> Result<(), String> {
    let mut stored = state
        .preferences
        .lock()
        .map_err(|_| "Settings are unavailable.".to_string())?;
    if preferences.close_to_tray && !state.tray_available {
        return Err("The system tray is unavailable on this desktop.".to_string());
    }
    set_autostart(&preferences)?;
    let destination = path(&app)?;
    let raw = serde_json::to_vec_pretty(&preferences)
        .map_err(|_| "Unable to encode settings.".to_string())?;
    // Same-directory replacement; settings are small and contain no credentials.
    let temporary = destination.with_extension("tmp");
    std::fs::write(&temporary, raw).map_err(|_| "Unable to save settings.".to_string())?;
    std::fs::rename(&temporary, &destination)
        .map_err(|_| "Unable to save settings.".to_string())?;
    *stored = preferences;
    Ok(())
}

#[cfg(target_os = "windows")]
fn set_autostart(preferences: &Preferences) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut command = Command::new("reg.exe");
    command.creation_flags(0x08000000);
    if preferences.run_at_startup {
        let executable = std::env::current_exe().map_err(|_| "Application path is unavailable.")?;
        let value = format!(
            "\"{}\"{}",
            executable.display(),
            if preferences.start_minimized {
                " --minimized"
            } else {
                ""
            }
        );
        command.args([
            "add", key, "/v", "Unibox", "/t", "REG_SZ", "/d", &value, "/f",
        ]);
    } else {
        let exists = Command::new("reg.exe")
            .creation_flags(0x08000000)
            .args(["query", key, "/v", "Unibox"])
            .output()
            .map_err(|_| "Windows startup settings are unavailable.")?;
        if !exists.status.success() {
            return Ok(());
        }
        command.args(["delete", key, "/v", "Unibox", "/f"]);
    }
    let output = command
        .output()
        .map_err(|_| "Windows startup settings could not be changed.")?;
    if output.status.success() {
        Ok(())
    } else {
        Err("Windows startup settings could not be changed.".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn set_autostart(preferences: &Preferences) -> Result<(), String> {
    if preferences.run_at_startup {
        return Err("Automatic startup is currently available on Windows.".to_string());
    }
    Ok(())
}

fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    let preferences: Preferences = path(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default();
    let open = MenuItem::with_id(app, "open", "Open Unibox", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Unibox", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id("unibox")
        .menu(&menu)
        .tooltip("Unibox")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app),
            "quit" => {
                if app
                    .state::<DesktopState>()
                    .maintenance_active
                    .load(Ordering::SeqCst)
                {
                    show(app);
                } else {
                    app.exit(0);
                }
            }
            _ => (),
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray_available = builder.build(app).is_ok();
    let minimized =
        preferences.start_minimized || std::env::args().any(|argument| argument == "--minimized");
    app.manage(DesktopState {
        preferences: Mutex::new(preferences),
        tray_available,
        maintenance_active: Arc::new(AtomicBool::new(false)),
    });
    if tray_available && minimized {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }
    Ok(())
}
