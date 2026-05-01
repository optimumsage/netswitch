use default_net::Interface;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

pub fn get_interfaces() -> Vec<Interface> {
    default_net::get_interfaces()
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
