use default_net::Interface;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use std::collections::HashMap;
use std::process::Command;

pub fn get_interfaces() -> Vec<Interface> {
    let mut interfaces = default_net::get_interfaces();
    
    // Supplement with macOS friendly names
    #[cfg(target_os = "macos")]
    {
        let friendly_names = get_macos_friendly_names();
        for iface in &mut interfaces {
            if let Some(friendly) = friendly_names.get(&iface.name) {
                iface.friendly_name = Some(friendly.clone());
            } else if iface.name.starts_with("utun") {
                iface.friendly_name = Some("VPN (Tunnel)".to_string());
            } else if iface.name.starts_with("wg") {
                iface.friendly_name = Some("WireGuard".to_string());
            }
        }
    }
    
    interfaces
}

#[cfg(target_os = "macos")]
fn get_macos_friendly_names() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let output = Command::new("networksetup")
        .arg("-listnetworkserviceorder")
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut current_service = String::new();
        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("(") && !line.starts_with("(Hardware Port:") {
                if let Some(idx) = line.find(')') {
                    current_service = line[idx+1..].trim().to_string();
                    if current_service.contains('*') {
                        current_service = current_service.replace("*", "");
                    }
                }
            } else if line.starts_with("(Hardware Port:") {
                if let Some(device_idx) = line.find("Device: ") {
                    let device_part = &line[device_idx + 8..];
                    if let Some(end_idx) = device_part.find(')') {
                        let device_name = device_part[..end_idx].trim().to_string();
                        if !current_service.is_empty() {
                            map.insert(device_name, current_service.clone());
                        }
                    }
                }
            }
        }
    }
    map
}

pub fn check_internet_on_interface(iface: &Interface) -> bool {
    let local_ip = iface.ipv4.iter().find(|ip| !ip.addr.is_loopback() && !ip.addr.is_multicast() && !ip.addr.is_unspecified());
    
    let local_ip = match local_ip {
        Some(ip) => ip.addr,
        None => return false, // No usable IPv4 address
    };

    let domain = Domain::IPV4;
    let socket = match Socket::new(domain, Type::STREAM, Some(Protocol::TCP)) {
        Ok(s) => s,
        Err(_) => return false,
    };

    // Bind to the local IP of the interface
    let bind_addr = SocketAddr::new(IpAddr::V4(local_ip), 0);
    if socket.bind(&bind_addr.into()).is_err() {
        return false;
    }

    // Platform specific bindings for extra assurance
    #[cfg(target_os = "macos")]
    {
        if let Some(idx) = std::num::NonZeroU32::new(iface.index) {
            // macOS supports binding by interface index
            let _ = socket.bind_device_by_index_v4(Some(idx));
        }
    }

    #[cfg(target_os = "linux")]
    {
        let _ = socket.bind_device(Some(iface.name.as_bytes()));
    }

    // Attempt to connect to a reliable external server (Google DNS)
    let target = SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::new(8, 8, 8, 8)), 53);
    
    // 2-second timeout
    let timeout = Duration::from_secs(2);
    
    match socket.connect_timeout(&target.into(), timeout) {
        Ok(_) => true,
        Err(_) => false,
    }
}
