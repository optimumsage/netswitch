mod network;
mod routing;
mod ipc;
#[macro_use]
mod log;
mod config;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;

use tokio_util::sync::CancellationToken;

#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, service_main);

// --- Failover tuning -------------------------------------------------------
// How often we poll interface health.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
// Consecutive successful probes before an interface is considered UP.
const UP_THRESHOLD: u32 = 1;
// Consecutive failed probes before an interface is considered DOWN. Higher than
// UP so a transient blip on the active link does NOT trigger a failover.
const DOWN_THRESHOLD: u32 = 3;
// A higher-priority interface must be stably UP for this many probes before it
// is allowed to preempt a currently-working active interface. Prevents a flaky
// higher-priority link from flapping the route back and forth.
const PREEMPT_STABLE: u32 = 3;
// Cap on the counters to avoid unbounded growth.
const COUNTER_CAP: u32 = 1000;

/// Debounced health for a single interface, carried across poll iterations.
#[derive(Default)]
struct IfaceHealth {
    up: bool,
    consecutive_ok: u32,
    consecutive_fail: u32,
}

impl IfaceHealth {
    fn record(&mut self, ok: bool) {
        if ok {
            self.consecutive_ok = (self.consecutive_ok + 1).min(COUNTER_CAP);
            self.consecutive_fail = 0;
            if self.consecutive_ok >= UP_THRESHOLD {
                self.up = true;
            }
        } else {
            self.consecutive_fail = (self.consecutive_fail + 1).min(COUNTER_CAP);
            self.consecutive_ok = 0;
            if self.consecutive_fail >= DOWN_THRESHOLD {
                self.up = false;
            }
        }
    }
}

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
                    log_info!("Netswitch Daemon v{} started (CLI mode)", version);
                    run_daemon(token).await;
                }
                _ => return Err(e.into()),
            }
        }
    }

    #[cfg(not(windows))]
    {
        log_info!("Netswitch Daemon v{} started", version);

        // Handle Ctrl+C (SIGINT) and SIGTERM, which is what launchd/systemd send
        // to stop the service. Either triggers a graceful shutdown that releases
        // the IPC port.
        let signal_token = token.clone();
        tokio::spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm = match signal(SignalKind::terminate()) {
                    Ok(s) => s,
                    Err(e) => {
                        log_warn!("Failed to install SIGTERM handler: {}", e);
                        return;
                    }
                };
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = sigterm.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
            log_info!("Shutting down...");
            signal_token.cancel();
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

    let status_handle = match service_control_handler::register("NetswitchDaemon", move |event| {
        match event {
            ServiceControl::Stop => {
                service_token.cancel();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    }) {
        Ok(h) => h,
        Err(e) => {
            log_warn!("Failed to register service control handler: {}", e);
            return;
        }
    };

    let running_status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    };
    if let Err(e) = status_handle.set_service_status(running_status) {
        log_warn!("Failed to report Running status: {}", e);
        return;
    }

    // Start the tokio runtime for the daemon.
    match tokio::runtime::Runtime::new() {
        Ok(rt) => rt.block_on(async {
            run_daemon(token).await;
        }),
        Err(e) => {
            log_warn!("Failed to start tokio runtime: {}", e);
        }
    }

    // Report stopped (best-effort).
    let stopped_status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    };
    if let Err(e) = status_handle.set_service_status(stopped_status) {
        log_warn!("Failed to report Stopped status: {}", e);
    }
}

fn resolve_port() -> u16 {
    std::env::var("NETSWITCH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| {
            if std::env::var("NETSWITCH_DEV").is_ok() {
                51338
            } else {
                51337
            }
        })
}

/// Chooses which interface should be active given the priority-sorted list and
/// the debounced health map. Implements stickiness: a working active interface
/// is only preempted by a higher-priority one that has been *stably* up.
fn select_active(
    sorted: &[ipc::InterfaceInfo],
    health: &HashMap<String, IfaceHealth>,
    current: Option<&str>,
) -> Option<String> {
    let is_up = |name: &str| health.get(name).map(|h| h.up).unwrap_or(false);

    // Best = highest-priority interface that is debounced-UP.
    let best = sorted.iter().find(|i| is_up(&i.name)).map(|i| i.name.clone());

    let best = match best {
        Some(b) => b,
        None => return None, // nothing has internet
    };

    // If the current active is still up and the best candidate is a *different*
    // (necessarily higher-priority) interface, only switch if that candidate has
    // been stably up — otherwise stick with the working connection.
    if let Some(cur) = current {
        if is_up(cur) && best != cur {
            let stable = health
                .get(&best)
                .map(|h| h.consecutive_ok >= PREEMPT_STABLE)
                .unwrap_or(false);
            if !stable {
                return Some(cur.to_string());
            }
        }
    }

    Some(best)
}

async fn run_daemon(token: CancellationToken) {
    let version = env!("CARGO_PKG_VERSION");
    let port = resolve_port();

    // Restore the user's saved priority order so a reboot doesn't reset it.
    let saved_order = config::load_order();

    let state = Arc::new(Mutex::new(ipc::DaemonState {
        version: version.to_string(),
        interfaces: vec![],
        current_active: None,
        custom_order: saved_order,
    }));

    let server_state = state.clone();
    let server_token = token.clone();
    tokio::spawn(async move {
        ipc::start_server(server_state, server_token, port).await;
    });

    // `current_active` reflects the route we have actually applied (only updated
    // on a successful routing change), so the UI never claims a primary we
    // failed to set.
    let mut current_active: Option<String> = None;
    let mut last_order: Vec<String> = vec![];
    let mut health: HashMap<String, IfaceHealth> = HashMap::new();

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                log_info!("Daemon loop stopping...");
                break;
            }
            _ = time::sleep(POLL_INTERVAL) => {
                run_cycle(&state, &mut current_active, &mut last_order, &mut health).await;
            }
        }
    }
}

async fn run_cycle(
    state: &ipc::SharedState,
    current_active: &mut Option<String>,
    last_order: &mut Vec<String>,
    health: &mut HashMap<String, IfaceHealth>,
) {
    let mut interfaces = network::get_interfaces();

    let custom_order = {
        let s = state.lock().await;
        s.custom_order.clone()
    };

    let order_changed = custom_order != *last_order;

    // Sort by the user's custom priority order, then by name for stability.
    interfaces.sort_by(|a, b| {
        let pos_a = custom_order
            .iter()
            .position(|name| name == &a.name)
            .unwrap_or(usize::MAX);
        let pos_b = custom_order
            .iter()
            .position(|name| name == &b.name)
            .unwrap_or(usize::MAX);
        if pos_a != pos_b {
            pos_a.cmp(&pos_b)
        } else {
            a.name.cmp(&b.name)
        }
    });

    // Probe every usable interface concurrently. The probe does blocking socket
    // I/O, so it runs on the blocking pool rather than starving async workers.
    let mut check_tasks = Vec::new();
    for iface in interfaces.clone() {
        if iface.is_loopback() || iface.ipv4.is_empty() {
            continue;
        }
        check_tasks.push(tokio::task::spawn_blocking(move || {
            let has_internet = network::check_internet_on_interface(&iface);
            (iface.name.clone(), has_internet)
        }));
    }

    let results = futures::future::join_all(check_tasks).await;
    let probe_results: HashMap<String, bool> = results.into_iter().filter_map(|r| r.ok()).collect();

    // Update debounced health and prune interfaces that disappeared.
    for (name, ok) in &probe_results {
        health.entry(name.clone()).or_default().record(*ok);
    }
    health.retain(|name, _| probe_results.contains_key(name));

    // Build the per-interface view shown in the UI using the *debounced* state.
    let mut interface_infos: Vec<ipc::InterfaceInfo> = Vec::new();
    for iface in &interfaces {
        if iface.is_loopback() || iface.ipv4.is_empty() {
            continue;
        }
        if !probe_results.contains_key(&iface.name) {
            continue;
        }
        let up = health.get(&iface.name).map(|h| h.up).unwrap_or(false);
        interface_infos.push(ipc::InterfaceInfo {
            name: iface.name.clone(),
            friendly_name: iface
                .friendly_name
                .clone()
                .unwrap_or_else(|| iface.name.clone()),
            has_internet: up,
            is_primary: false,
        });
    }

    // In debug builds only, surface a mock interface when nothing real is found
    // so the UI can be exercised. Never shown to real users.
    #[cfg(debug_assertions)]
    if interface_infos.is_empty() {
        interface_infos.push(ipc::InterfaceInfo {
            name: "mock0".to_string(),
            friendly_name: "Mock Interface (Debug)".to_string(),
            has_internet: true,
            is_primary: true,
        });
        health.entry("mock0".to_string()).or_default().up = true;
    }

    // Decide the desired active interface (with hysteresis + stickiness).
    let desired = select_active(&interface_infos, health, current_active.as_deref());

    // Apply routing only when the desired interface changed or the user's
    // priority order changed (so the OS ordering reflects the new preference).
    if let Some(desired_name) = &desired {
        let needs_apply = current_active.as_deref() != Some(desired_name.as_str()) || order_changed;
        if needs_apply {
            if let Some(active_iface) = interfaces.iter().find(|i| &i.name == desired_name) {
                log_info!(
                    "Switching active route -> {} (order_changed={})",
                    desired_name,
                    order_changed
                );
                match routing::set_primary_interface(active_iface, &interfaces) {
                    Ok(()) => {
                        *current_active = Some(desired_name.clone());
                        *last_order = custom_order.clone();
                    }
                    Err(e) => {
                        log_warn!("Failed to switch to {}: {}", desired_name, e);
                    }
                }
            } else if desired_name == "mock0" {
                // Debug mock has no real interface to route to.
                *current_active = Some("mock0".to_string());
                *last_order = custom_order.clone();
            }
        }
    } else {
        // Nothing is up. Keep last_order in sync so a later recovery re-applies.
        if order_changed {
            *last_order = custom_order.clone();
        }
    }

    // Mark the primary in the UI view based on the route we actually applied.
    for info in &mut interface_infos {
        info.is_primary = Some(&info.name) == current_active.as_ref();
    }

    // Publish state if anything changed.
    let mut changed = false;
    {
        let mut s = state.lock().await;
        if s.interfaces != interface_infos
            || s.current_active != *current_active
            || s.custom_order != custom_order
        {
            s.interfaces = interface_infos;
            s.current_active = current_active.clone();
            changed = true;
        }
    }
    if changed {
        let s = state.lock().await;
        log_info!(
            "State updated: active={:?}, interfaces={}",
            s.current_active,
            s.interfaces.len()
        );
    }
}
