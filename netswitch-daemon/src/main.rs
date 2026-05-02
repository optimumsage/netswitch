mod network;
mod routing;
mod ipc;

use std::time::Duration;
use tokio::time;
use std::sync::Arc;
use tokio::sync::Mutex;

use tokio_util::sync::CancellationToken;

#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, service_main);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let version = env!("CARGO_PKG_VERSION");
    let token = CancellationToken::new();
    
    #[cfg(windows)]
    {
        // If we're running as a service, the dispatcher will take over.
        // Otherwise, it returns an error, and we run as a normal CLI app.
        if let Err(e) = windows_service::service_dispatcher::start("NetswitchDaemon", ffi_service_main) {
            match e {
                windows_service::Error::Winapi(err) if err.raw_os_error() == Some(1063) => {
                    // ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: Not running as a service
                    println!("Netswitch Daemon v{} started (CLI mode)", version);
                    run_daemon(token).await;
                }
                _ => return Err(e.into()),
            }
        }
    }

    #[cfg(not(windows))]
    {
        println!("Netswitch Daemon v{} started", version);
        
        // Handle Ctrl+C
        let ctrl_c_token = token.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.expect("failed to listen for event");
            println!("Shutting down...");
            ctrl_c_token.cancel();
        });

        run_daemon(token).await;
    }

    Ok(())
}

#[cfg(windows)]
fn service_main(_arguments: Vec<std::ffi::OsString>) {
    use windows_service::{
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
    };

    let token = CancellationToken::new();
    let service_token = token.clone();

    let status_handle = service_control_handler::register("NetswitchDaemon", move |event| {
        match event {
            ServiceControl::Stop => {
                service_token.cancel();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    }).unwrap();

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    }).unwrap();

    // Start the tokio runtime for the daemon
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        run_daemon(token).await;
    });

    // Report stopped
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    }).unwrap();
}

async fn run_daemon(token: CancellationToken) {
    let version = env!("CARGO_PKG_VERSION");
    let state = Arc::new(Mutex::new(ipc::DaemonState {
        version: version.to_string(),
        interfaces: vec![],
        current_active: None,
        custom_order: vec![],
    }));

    let server_state = state.clone();
    let server_token = token.clone();
    tokio::spawn(async move {
        ipc::start_server(server_state, server_token).await;
    });

    let mut current_active: Option<String> = None;
    let mut last_order: Vec<String> = vec![];

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                println!("Daemon loop stopping...");
                break;
            }
            _ = time::sleep(Duration::from_secs(2)) => {
                let mut interfaces = network::get_interfaces();
                
                let custom_order = {
                    let s = state.lock().await;
                    s.custom_order.clone()
                };

                let order_changed = custom_order != last_order;

                interfaces.sort_by(|a, b| {
                    let pos_a = custom_order.iter().position(|name| name == &a.name).unwrap_or(usize::MAX);
                    let pos_b = custom_order.iter().position(|name| name == &b.name).unwrap_or(usize::MAX);
                    
                    if pos_a != pos_b {
                        pos_a.cmp(&pos_b)
                    } else {
                        a.name.cmp(&b.name)
                    }
                });

                let mut check_tasks = Vec::new();
                for iface in interfaces.clone() {
                    if iface.is_loopback() || iface.ipv4.is_empty() {
                        continue;
                    }
                    check_tasks.push(tokio::spawn(async move {
                        let has_internet = network::check_internet_on_interface(&iface);
                        (iface.name.clone(), has_internet)
                    }));
                }

                let results = futures::future::join_all(check_tasks).await;
                let checked_results: std::collections::HashMap<String, bool> = results
                    .into_iter()
                    .filter_map(|res| res.ok())
                    .collect();

                let mut interface_infos = Vec::new();
                let mut next_active = None;

                for iface in &interfaces {
                    if iface.is_loopback() || iface.ipv4.is_empty() {
                        continue;
                    }

                    if let Some(&has_internet) = checked_results.get(&iface.name) {
                        if has_internet && next_active.is_none() {
                            next_active = Some(iface.clone());
                        }

                        interface_infos.push(ipc::InterfaceInfo {
                            name: iface.name.clone(),
                            friendly_name: iface.friendly_name.clone().unwrap_or_else(|| iface.name.clone()),
                            has_internet,
                            is_primary: false,
                        });
                    }
                }

                let mut new_active_name = None;

                if interface_infos.is_empty() {
                    interface_infos.push(ipc::InterfaceInfo {
                        name: "mock0".to_string(),
                        friendly_name: "Mock Interface (Debug)".to_string(),
                        has_internet: true,
                        is_primary: true,
                    });
                    new_active_name = Some("mock0".to_string());
                }

                if let Some(active_iface) = next_active {
                    let active_name = active_iface.name.clone();
                    new_active_name = Some(active_name.clone());
                    
                    if current_active.as_deref() != Some(&active_name) || order_changed {
                        println!("Updating routing (Active={}, OrderChanged={})", active_name, order_changed);
                        
                        if let Err(e) = routing::set_primary_interface(&active_iface, &interfaces) {
                            eprintln!("Failed to switch interface: {}", e);
                        } else {
                            current_active = Some(active_name.clone());
                            last_order = custom_order.clone();
                        }
                    }
                } else if new_active_name.as_deref() != Some("mock0") {
                    // No real internet
                }

                for info in &mut interface_infos {
                    if Some(&info.name) == new_active_name.as_ref() {
                        info.is_primary = true;
                    }
                }

                let mut state_changed = false;
                {
                    let mut s = state.lock().await;
                    if s.interfaces != interface_infos || s.current_active != new_active_name || s.custom_order != custom_order {
                        s.interfaces = interface_infos;
                        s.current_active = new_active_name.clone();
                        state_changed = true;
                    }
                }

                if state_changed {
                    let s = state.lock().await;
                    println!("Daemon State Updated: Active={:?}, Order={:?}", s.current_active, s.custom_order);
                }
            }
        }
    }
}
