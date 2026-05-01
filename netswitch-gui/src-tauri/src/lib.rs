use serde::{Deserialize, Serialize};

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
        
        // On macOS/Linux, we use AppleScript or pkexec/sudo to prompt for password
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
            // Try pkexec (GUI) then sudo
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
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_daemon_status, 
            set_interface_order,
            install_daemon_service
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
