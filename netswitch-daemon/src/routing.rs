use anyhow::{anyhow, Context, Result};
use default_net::Interface;
use std::process::Command;

#[cfg(any(target_os = "linux", target_os = "windows"))]
use crate::{log_info, log_warn};

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
        let _ = (iface, all_interfaces);
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
        if line.starts_with('(') && !line.starts_with("(Hardware Port:") {
            // (1) Wi-Fi   — or "(*) Wi-Fi" for a disabled service
            if let Some(idx) = line.find(')') {
                current_service = line[idx + 1..].trim().to_string();
                // A leading '*' inside the parens marks a disabled service.
                // `-ordernetworkservices` rejects disabled services, so skip them.
                let disabled = line[..=idx].contains('*');
                if !disabled && !current_service.is_empty() {
                    services.push(current_service.clone());
                }
            }
        } else if line.starts_with("(Hardware Port:") {
            // (Hardware Port: Wi-Fi, Device: en0)
            if line.contains(&format!("Device: {}", primary.name)) {
                primary_service_name = current_service.clone();
            }
        }
    }

    if primary_service_name.is_empty() {
        return Err(anyhow!(
            "Could not find macOS network service for interface {}",
            primary.name
        ));
    }

    // Reorder: put primary at the top, followed by the rest (enabled services only).
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

    let status = cmd
        .status()
        .context("Failed to run networksetup -ordernetworkservices")?;
    if !status.success() {
        return Err(anyhow!("Failed to reorder network services"));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn set_primary_interface_windows(primary: &Interface, all_interfaces: &[Interface]) -> Result<()> {
    // Lower metric = higher priority.
    let mut any_failed = false;
    for iface in all_interfaces {
        if iface.is_loopback() || iface.ipv4.is_empty() {
            continue;
        }
        let metric = if iface.index == primary.index { 1 } else { 50 };

        let result = Command::new("netsh")
            .args([
                "interface",
                "ipv4",
                "set",
                "interface",
                &iface.index.to_string(),
                &format!("metric={}", metric),
            ])
            .output();

        match result {
            Ok(out) if !out.status.success() => {
                any_failed = true;
                log_warn!(
                    "netsh metric set failed for {} (idx {}): {}",
                    iface.name,
                    iface.index,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Err(e) => {
                any_failed = true;
                log_warn!("Failed to spawn netsh for {}: {}", iface.name, e);
            }
            _ => {}
        }
    }

    if any_failed {
        log_warn!("One or more interface metrics could not be set");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_primary_interface_linux(primary: &Interface, all_interfaces: &[Interface]) -> Result<()> {
    // Adjust default-route metrics via iproute2: the primary interface gets the
    // lowest metric (highest priority) and every other interface a higher one.
    // This is best-effort — we never tear the network down, and we log instead
    // of erroring so a single failing command does not abort failover.
    const PRIMARY_METRIC: u32 = 100;
    const FALLBACK_METRIC: u32 = 600;

    let mut applied = false;

    for iface in all_interfaces {
        if iface.is_loopback() || iface.ipv4.is_empty() {
            continue;
        }

        let gateway = match &iface.gateway {
            Some(gw) => gw.ip_addr,
            None => {
                // No known gateway — can't set a default route for it.
                continue;
            }
        };

        let metric = if iface.index == primary.index {
            PRIMARY_METRIC
        } else {
            FALLBACK_METRIC
        };

        let result = Command::new("ip")
            .args([
                "route",
                "replace",
                "default",
                "via",
                &gateway.to_string(),
                "dev",
                &iface.name,
                "metric",
                &metric.to_string(),
            ])
            .output();

        match result {
            Ok(out) if out.status.success() => {
                applied = true;
            }
            Ok(out) => {
                log_warn!(
                    "`ip route replace` failed for {} via {}: {}",
                    iface.name,
                    gateway,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Err(e) => {
                log_warn!(
                    "Failed to spawn `ip` for {} (is iproute2 installed?): {}",
                    iface.name,
                    e
                );
            }
        }
    }

    if !applied {
        log_info!(
            "Linux failover: no default routes were adjusted (no usable gateway for {})",
            primary.name
        );
    }
    Ok(())
}
