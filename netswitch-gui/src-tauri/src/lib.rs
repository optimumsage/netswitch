use serde::{Deserialize, Serialize};
use tauri::{Manager, menu::{Menu, MenuItem, PredefinedMenuItem}, tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState}};

#[derive(Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub version: String,
    pub interfaces: Vec<InterfaceInfo>,
    pub current_active: Option<String>,
    pub custom_order: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub friendly_name: String,
    pub has_internet: bool,
    pub is_primary: bool,
}

struct TrayStatusItem(MenuItem<tauri::Wry>);

#[tauri::command]
async fn get_daemon_status() -> Result<DaemonState, String> {
    let client = reqwest::Client::new();
    let res = client
        .get("http://127.0.0.1:51337/status")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let state = res.json::<DaemonState>().await.map_err(|e| e.to_string())?;
    Ok(state)
}

#[tauri::command]
async fn set_interface_order(order: Vec<String>) -> Result<DaemonState, String> {
    let client = reqwest::Client::new();
    // 1. Send the new order
    let _ = client
        .post("http://127.0.0.1:51337/order")
        .json(&serde_json::json!({ "order": order }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // 2. Immediately fetch the new state to confirm
    let res = client
        .get("http://127.0.0.1:51337/status")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let state = res.json::<DaemonState>().await.map_err(|e| e.to_string())?;
    Ok(state)
}

#[tauri::command]
fn update_tray_status(
    app: tauri::AppHandle, 
    active_interface: Option<String>, 
    friendly_name: Option<String>,
    status_item: tauri::State<'_, TrayStatusItem>
) {
    let name = friendly_name.or(active_interface);
    let title = match name {
        Some(ref n) => format!("Netswitch: {}", n),
        None => "Netswitch: Offline".to_string(),
    };
    
    if let Some(tray) = app.tray_by_id("main") {
        #[cfg(target_os = "macos")]
        let _ = tray.set_title(Some(title.clone()));
        
        let _ = tray.set_tooltip(Some(title.clone()));
    }

    let _ = status_item.0.set_text(title);
}

#[tauri::command]
async fn install_daemon_service(app: tauri::AppHandle) -> Result<(), String> {
    use std::process::Command;
    use tauri::Manager;

    let resource_path = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        let script_path = resource_path.join("bin/setup-daemon.ps1");
        let status = Command::new("powershell")
            .args(&[
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &format!("Start-Process powershell -ArgumentList '-ExecutionPolicy Bypass -File \"{}\"' -Verb RunAs", script_path.display()),
            ])
            .status()
            .map_err(|e| e.to_string())?;

        if !status.success() {
            return Err("Failed to launch PowerShell setup".to_string());
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let script_path = resource_path.join("bin/setup-daemon.sh");
        
        #[cfg(target_os = "macos")]
        {
            let osascript = format!(
                "do shell script \"bash '{}'\" with administrator privileges",
                script_path.display()
            );
            let status = Command::new("osascript")
                .args(&["-e", &osascript])
                .status()
                .map_err(|e| e.to_string())?;
            
            if !status.success() {
                return Err("Failed to execute setup with administrator privileges".to_string());
            }
        }

        #[cfg(target_os = "linux")]
        {
            let status = Command::new("pkexec")
                .arg("bash")
                .arg(script_path.to_str().unwrap())
                .status()
                .or_else(|_| {
                    Command::new("sudo")
                        .arg("bash")
                        .arg(script_path.to_str().unwrap())
                        .status()
                })
                .map_err(|e| e.to_string())?;
            
            if !status.success() {
                return Err("Failed to execute setup with root privileges".to_string());
            }
        }
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let status_i = MenuItem::with_id(app, "status", "Netswitch: Offline", false, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Netswitch", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&status_i, &sep, &show_i, &quit_i])?;

            app.manage(TrayStatusItem(status_i.clone()));

            let tray_builder = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(false);

            let tray_builder = if let Some(icon) = app.default_window_icon() {
                tray_builder.icon(icon.clone())
            } else {
                tray_builder
            };

            let _tray = tray_builder
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "quit" => {
                            app.exit(0);
                        }
                        "show" => {
                            let window = app.get_webview_window("main").unwrap();
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.unminimize();
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.unminimize();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                window.hide().unwrap();
                api.prevent_close();
            }
            tauri::WindowEvent::Resized(..) => {
                if window.is_minimized().unwrap_or(false) {
                    window.hide().unwrap();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_daemon_status, 
            set_interface_order,
            install_daemon_service,
            update_tray_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
