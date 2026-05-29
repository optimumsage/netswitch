use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::State,
};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::net::SocketAddr;
use std::time::Duration;

use crate::{config, log_info, log_warn};

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonState {
    pub version: String,
    pub interfaces: Vec<InterfaceInfo>,
    pub current_active: Option<String>,
    pub custom_order: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct InterfaceInfo {
    pub name: String,
    pub friendly_name: String,
    pub has_internet: bool,
    pub is_primary: bool,
}

#[derive(Deserialize)]
pub struct UpdateOrderRequest {
    pub order: Vec<String>,
}

pub type SharedState = Arc<Mutex<DaemonState>>;

pub async fn start_server(state: SharedState, token: tokio_util::sync::CancellationToken, port: u16) {
    let app = Router::new()
        .route("/status", get(get_status))
        .route("/order", post(update_order))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    // Bind with exponential backoff instead of panicking. If the port is held
    // (e.g. a stale/duplicate daemon, or a race on restart) we keep retrying so
    // the IPC endpoint eventually comes up rather than silently dying and
    // leaving the GUI unable to ever connect.
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(30);

    let listener = loop {
        tokio::select! {
            _ = token.cancelled() => {
                log_info!("IPC server cancelled before bind");
                return;
            }
            bind = tokio::net::TcpListener::bind(addr) => {
                match bind {
                    Ok(l) => {
                        log_info!("IPC server listening on {}", addr);
                        break l;
                    }
                    Err(e) => {
                        log_warn!(
                            "Could not bind IPC port {} ({}); retrying in {:?}",
                            port, e, backoff
                        );
                        tokio::select! {
                            _ = token.cancelled() => return,
                            _ = tokio::time::sleep(backoff) => {}
                        }
                        backoff = (backoff * 2).min(max_backoff);
                    }
                }
            }
        }
    };

    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            token.cancelled().await;
            log_info!("IPC server received shutdown signal");
        })
        .await;

    if let Err(e) = serve_result {
        log_warn!("IPC server stopped with error: {}", e);
    }
}

async fn get_status(State(state): State<SharedState>) -> Json<DaemonState> {
    let state = state.lock().await;
    Json(state.clone())
}

async fn update_order(
    State(state): State<SharedState>,
    Json(payload): Json<UpdateOrderRequest>,
) -> Json<bool> {
    log_info!("IPC: updating custom order to {:?}", payload.order);
    {
        let mut state = state.lock().await;
        state.custom_order = payload.order.clone();
    }
    // Persist outside the lock so disk I/O never blocks status readers.
    config::save_order(&payload.order);
    Json(true)
}
