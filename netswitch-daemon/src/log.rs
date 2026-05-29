//! Minimal, dependency-free timestamped logging for the daemon.
//!
//! The daemon runs under launchd / systemd / the Windows SCM, whose log files do
//! not always prepend timestamps, so we add our own UTC timestamp to every line.

use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current UTC time formatted as `YYYY-MM-DD HH:MM:SSZ`.
///
/// Uses Howard Hinnant's civil-from-days algorithm so we avoid pulling in a
/// date/time crate just for log prefixes.
pub fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, min, sec) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    if m <= 2 {
        y += 1;
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z",
        y, m, d, hour, min, sec
    )
}

/// Log an informational line to stdout with a timestamp prefix.
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        println!("[{}] {}", $crate::log::timestamp(), format!($($arg)*))
    };
}

/// Log a warning/error line to stderr with a timestamp prefix.
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        eprintln!("[{}] WARN {}", $crate::log::timestamp(), format!($($arg)*))
    };
}
