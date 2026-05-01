use anyhow::{Result, Context, anyhow};
use default_net::Interface;
use std::process::Command;

pub fn set_primary_interface(iface: &Interface, all_interfaces: &[Interface]) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        set_primary_interface_macos(iface, all_interfaces)
    }

    #[cfg(target_os = "windows")]
    {
        set_primary_interface_windows(iface, all_interfaces)
    }

    #[cfg(target_os = "linux")]
    {
        set_primary_interface_linux(iface, all_interfaces)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(anyhow!("Unsupported OS"))
    }
}

#[cfg(target_os = "macos")]
fn set_primary_interface_macos(primary: &Interface, _all: &[Interface]) -> Result<()> {
    // 1. Get service order mapping
    let output = Command::new("networksetup")
        .arg("-listnetworkserviceorder")
        .output()
        .context("Failed to run networksetup")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse output to find service name for the given interface device
    // Example format:
    // (1) Wi-Fi
    // (Hardware Port: Wi-Fi, Device: en0)
    
    let mut services = Vec::new();
    let mut current_service = String::new();
    let mut primary_service_name = String::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("(") && !line.starts_with("(Hardware Port:") {
            // (1) Wi-Fi
            if let Some(idx) = line.find(')') {
                current_service = line[idx+1..].trim().to_string();
                if current_service.contains('*') {
                    // Disabled service, usually has an asterisk
                    current_service = current_service.replace("*", "");
                }
                services.push(current_service.clone());
            }
        } else if line.starts_with("(Hardware Port:") {
            // (Hardware Port: Wi-Fi, Device: en0)
            if line.contains(&format!("Device: {}", primary.name)) {
                primary_service_name = current_service.clone();
            }
        }
    }

    if primary_service_name.is_empty() {
        return Err(anyhow!("Could not find macOS network service for interface {}", primary.name));
    }

    // Reorder: put primary at the top, followed by the rest
    let mut new_order = vec![primary_service_name.clone()];
    for s in services {
        if s != primary_service_name {
            new_order.push(s);
        }
    }

    // Run networksetup -ordernetworkservices
    let mut cmd = Command::new("networksetup");
    cmd.arg("-ordernetworkservices");
    for s in new_order {
        cmd.arg(s);
    }

    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow!("Failed to reorder network services"));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn set_primary_interface_windows(primary: &Interface, all_interfaces: &[Interface]) -> Result<()> {
    // Lower metric = higher priority
    for iface in all_interfaces {
        if iface.is_loopback() || iface.ipv4.is_empty() {
            continue;
        }
        let metric = if iface.index == primary.index { 1 } else { 50 };
        
        let _ = Command::new("netsh")
            .args(&[
                "interface",
                "ipv4",
                "set",
                "interface",
                &iface.index.to_string(),
                &format!("metric={}", metric),
            ])
            .output();
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_primary_interface_linux(primary: &Interface, all_interfaces: &[Interface]) -> Result<()> {
    // Use `ip route change` or `iproute2` to update default gateways
    // A simpler approach for just managing metrics is ifmetric, but ip route is standard.
    // For now we just print a placeholder to avoid breaking network without careful gateway info.
    println!("Linux routing metric change for {} not fully implemented", primary.name);
    Ok(())
}
