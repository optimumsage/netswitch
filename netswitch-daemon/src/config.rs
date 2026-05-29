//! Persistence for the user's custom interface priority order.
//!
//! The order is the only piece of user intent the daemon holds, and losing it
//! on every reboot (it previously lived only in memory) made the app feel
//! unreliable. We persist it to a system-wide path the privileged daemon can
//! write. All operations are best-effort and never panic.

use std::path::PathBuf;

use crate::{log_info, log_warn};

/// Returns the file where the custom order is stored.
///
/// * Unix (macOS/Linux): `/usr/local/etc/netswitch/order.json`
/// * Windows: `%ProgramData%\Netswitch\order.json` (falls back to a temp dir)
fn order_path() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var("ProgramData").unwrap_or_else(|_| {
            std::env::temp_dir().to_string_lossy().into_owned()
        });
        PathBuf::from(base).join("Netswitch").join("order.json")
    }

    #[cfg(not(windows))]
    {
        PathBuf::from("/usr/local/etc/netswitch/order.json")
    }
}

/// Loads the persisted custom order, or an empty list if none/unreadable.
pub fn load_order() -> Vec<String> {
    let path = order_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<Vec<String>>(&contents) {
            Ok(order) => {
                log_info!("Loaded persisted interface order ({} entries)", order.len());
                order
            }
            Err(e) => {
                log_warn!("Failed to parse {}: {}", path.display(), e);
                Vec::new()
            }
        },
        Err(_) => Vec::new(), // No file yet — first run.
    }
}

/// Persists the custom order. Best-effort: failures are logged, not propagated.
pub fn save_order(order: &[String]) {
    let path = order_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log_warn!("Could not create config dir {}: {}", parent.display(), e);
            return;
        }
    }

    match serde_json::to_string_pretty(order) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log_warn!("Could not write order to {}: {}", path.display(), e);
            }
        }
        Err(e) => log_warn!("Could not serialize order: {}", e),
    }
}
