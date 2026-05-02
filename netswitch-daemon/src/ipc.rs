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

pub async fn start_server(state: SharedState, token: tokio_util::sync::CancellationToken) {
    let app = Router::new()
        .route("/status", get(get_status))
        .route("/order", post(update_order))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 51337));
    println!("IPC Server listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            token.cancelled().await;
            println!("IPC server received shutdown signal");
        })
        .await
        .unwrap();
}

async fn get_status(State(state): State<SharedState>) -> Json<DaemonState> {
    let state = state.lock().await;
    Json(state.clone())
}

async fn update_order(
    State(state): State<SharedState>,
    Json(payload): Json<UpdateOrderRequest>,
) -> Json<bool> {
    println!("IPC: Updating custom order to {:?}", payload.order);
    let mut state = state.lock().await;
    state.custom_order = payload.order;
    Json(true)
}
