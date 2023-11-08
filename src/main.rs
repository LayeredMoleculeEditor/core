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
use layer::{
    fill_layer::{FillLayer, BLANK_LAYER},
    filter_layer::{RotationLayer, TranslateLayer},
    AtomTable, BondTable, Layer,
};
use nalgebra::Vector3;

mod layer;
pub mod utils;

struct Session {
    stack: Vec<(Arc<dyn Layer>, bool)>,
}

impl Session {
    fn new() -> Self {
        Self { stack: Vec::new() }
    }

    fn read(&self, index: usize) -> (AtomTable, BondTable) {
        let target = self
            .stack[0..=index]
            .iter()
            .filter_map(|(layer, enabled)| if *enabled { Some(layer.clone()) } else { None })
            .collect::<Vec<_>>();
        if let Some((last, base)) = target.split_last() {
            last.read(base)
        } else {
            BLANK_LAYER.read(&[])
        }
    }

    // fn add_atom(&mut self, layer_index: usize, element: usize, position: Vector3<f64>) -> Result<usize, SessionError> {
    //     if let 
    // }

    fn create_fill_layer(&mut self) -> Result<usize, SessionError> {
        self.stack.push((Arc::new(FillLayer::new()), true));
        Ok(self.stack.len())
    }

    fn create_translate_layer(&mut self, vector: Vector3<f64>) -> usize {
        self.stack
            .push((Arc::new(TranslateLayer::new(vector)), true));
        self.stack.len()
    }

    fn create_rotation_layer(
        &mut self,
        center: Vector3<f64>,
        vector: Vector3<f64>,
        angle: f64,
    ) -> usize {
        self.stack
            .push((Arc::new(RotationLayer::new(center, vector, angle)), true));
        self.stack.len()
    }

    fn disable_layer(&mut self, target: usize) -> Result<(), SessionError> {
        if target >= self.stack.len() {
            Err(SessionError::NoSuchLayer)
        } else {
            self.stack[target].1 = false;
            Ok(())
        }
    }

    fn remove_layer(&mut self, index: usize) -> Result<usize, SessionError> {
        if index >= self.stack.len() {
            Err(SessionError::NoSuchLayer)
        } else {
            self.stack.remove(index);
            Ok(self.stack.len())
        }
    }
}

enum SessionError {
    NoSuchLayer,
    NotFillLayer,
}

type SessionStore = Arc<RwLock<HashMap<String, Arc<RwLock<Session>>>>>;

#[tokio::main]
async fn main() {
    let projects: SessionStore = Arc::new(RwLock::new(HashMap::new()));

    let layer_rt = Router::new();

    let session_rt = Router::new()
        .route("/:session_name", post(create_session))
        .route("/:session_name", delete(close_session))
        .nest("/:session_name/op/:layer_name", layer_rt)
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
