use default_net::Interface;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::process::Command;

pub fn get_interfaces() -> Vec<Interface> {
    #[allow(unused_mut)]
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

    // Clean up Windows names (strip GUIDs)
    #[cfg(target_os = "windows")]
    {
        for iface in &mut interfaces {
            let name_to_clean = iface.friendly_name.as_ref().unwrap_or(&iface.name);
            if let Some(pos) = name_to_clean.find(" (") {
                if name_to_clean.contains('{') && name_to_clean.contains('}') {
                    iface.friendly_name = Some(name_to_clean[..pos].to_string());
                }
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

/// Well-known anycast resolvers used to probe for real internet reachability.
/// We try several so that a single provider being blocked (some networks drop
/// Google DNS, others Cloudflare) does not make every interface look offline.
const PROBE_TARGETS: &[([u8; 4], u16)] = &[
    ([1, 1, 1, 1], 53),   // Cloudflare
    ([8, 8, 8, 8], 53),   // Google
    ([9, 9, 9, 9], 53),   // Quad9
];

/// Per-target connect timeout. Kept short so a full probe stays responsive even
/// when every target is unreachable; targets are tried sequentially and we
/// return as soon as one succeeds.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1200);

/// Returns true if the given interface can establish an outbound TCP connection
/// to at least one well-known internet endpoint, with traffic forced out of
/// this specific interface (source-IP bind + platform device bind).
///
/// The targets are probed *concurrently* and we return as soon as one succeeds,
/// so a fully-unreachable interface costs ~one timeout rather than the sum of
/// all of them — keeping each poll cycle (and thus failover) responsive.
///
/// This performs blocking socket I/O and must be run off the async runtime
/// (e.g. via `tokio::task::spawn_blocking`).
pub fn check_internet_on_interface(iface: &Interface) -> bool {
    let local_ip = iface
        .ipv4
        .iter()
        .find(|ip| !ip.addr.is_loopback() && !ip.addr.is_multicast() && !ip.addr.is_unspecified());

    let local_ip = match local_ip {
        Some(ip) => ip.addr,
        None => return false, // No usable IPv4 address
    };

    let index = iface.index;
    let name = iface.name.clone();

    let (tx, rx) = std::sync::mpsc::channel();
    for (octets, port) in PROBE_TARGETS {
        let tx = tx.clone();
        let name = name.clone();
        let octets = *octets;
        let port = *port;
        std::thread::spawn(move || {
            let _ = tx.send(probe_target(index, &name, local_ip, octets, port));
        });
    }
    drop(tx); // so rx.recv() ends once all probe threads finish

    // First success wins. Bound the overall wait so a stuck probe can't hang
    // the cycle (a little slack over the per-target timeout).
    let deadline = std::time::Instant::now() + PROBE_TIMEOUT + Duration::from_millis(400);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(true) => return true,
            Ok(false) => continue, // one target failed; wait for the others
            Err(_) => return false, // all targets reported false, or we timed out
        }
    }
}

// `index`/`name` are each used on only some platforms (macOS uses the index,
// Linux the name); suppress the resulting per-platform unused-variable warnings.
#[allow(unused_variables)]
fn probe_target(
    index: u32,
    name: &str,
    local_ip: std::net::Ipv4Addr,
    octets: [u8; 4],
    port: u16,
) -> bool {
    let socket = match Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)) {
        Ok(s) => s,
        Err(_) => return false,
    };

    // Bind to the local IP of the interface so the kernel routes via it.
    let bind_addr = SocketAddr::new(IpAddr::V4(local_ip), 0);
    if socket.bind(&bind_addr.into()).is_err() {
        return false;
    }

    // Platform specific bindings for extra assurance the packets leave this NIC.
    #[cfg(target_os = "macos")]
    {
        if let Some(idx) = std::num::NonZeroU32::new(index) {
            let _ = socket.bind_device_by_index_v4(Some(idx));
        }
    }

    #[cfg(target_os = "linux")]
    {
        let _ = socket.bind_device(Some(name.as_bytes()));
    }

    let target = SocketAddr::new(
        IpAddr::V4(std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3])),
        port,
    );

    socket.connect_timeout(&target.into(), PROBE_TIMEOUT).is_ok()
}
