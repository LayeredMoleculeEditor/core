use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use layer::Layer;

mod layer;
pub mod utils;

struct Session {
    layers: HashMap<String, Box<dyn Layer>>,
}

impl Session {
    fn new() -> Self {
        Self { layers: HashMap::new() }
    }
}

type SessionStore = Arc<RwLock<HashMap<String, Arc<RwLock<Session>>>>>;

#[tokio::main]
async fn main() {
    let projects: SessionStore = Arc::new(RwLock::new(HashMap::new()));

    let operation_rt = Router::new();

    let session_rt = Router::new()
        .route("/:session_name", post(create_session))
        .route("/:session_name", delete(close_session))
        .nest("/:session_name/op", operation_rt)
        .route("/", get(list_sessions));

    let app = Router::new()
        .route("/", get(hello_world))
        .nest("/sessions", session_rt)
        .with_state(projects);
    axum::Server::bind(&"127.0.0.1:35182".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn hello_world() -> &'static str {
    "hello, world"
}

async fn create_session(
    State(store): State<SessionStore>,
    Path(session_name): Path<String>,
) -> (StatusCode, &'static str) {
    if store.read().unwrap().contains_key(&session_name) {
        (
            StatusCode::CONFLICT,
            "Session with given name already existed",
        )
    } else {
        store
            .write()
            .unwrap()
            .insert(session_name, Arc::new(RwLock::new(Session::new())));
        (StatusCode::OK, "Created")
    }
}

async fn close_session(
    State(store): State<SessionStore>,
    Path(session_name): Path<String>,
) -> (StatusCode, &'static str) {
    if store.read().unwrap().contains_key(&session_name) {
        store.write().unwrap().remove(&session_name);
        (StatusCode::OK, "Session closed")
    } else {
        (StatusCode::NOT_FOUND, "No such created session")
    }
}

async fn list_sessions(State(store): State<SessionStore>) -> (StatusCode, Json<Vec<String>>) {
    (
        StatusCode::OK,
        Json::from(store.read().unwrap().keys().cloned().collect::<Vec<_>>()),
    )
}
