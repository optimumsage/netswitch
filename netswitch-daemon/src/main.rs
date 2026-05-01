mod network;
mod routing;
mod ipc;

use std::time::Duration;
use tokio::time;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    let version = env!("CARGO_PKG_VERSION");
    println!("Netswitch Daemon v{} started", version);

    let state = Arc::new(Mutex::new(ipc::DaemonState {
        version: version.to_string(),
        interfaces: vec![],
        current_active: None,
        custom_order: vec![],
    }));

    let server_state = state.clone();
    tokio::spawn(async move {
        ipc::start_server(server_state).await;
    });

    let mut current_active: Option<String> = None;
    let mut last_order: Vec<String> = vec![];

    loop {
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

        time::sleep(Duration::from_secs(2)).await;
    }
}
