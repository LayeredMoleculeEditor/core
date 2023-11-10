use std::sync::{Arc, RwLock};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, patch, put},
    Json, Router,
};
use layer::{Layer, LayerConfig, Molecule};
use many_to_many::ManyToMany;
use utils::UniqueValueMap;

mod layer;
pub mod serde;
mod utils;

struct Project {
    stacks: Vec<Arc<Layer>>,
    id_map: UniqueValueMap<usize, String>,
    class_map: ManyToMany<usize, String>,
}

type ServerStore = Arc<RwLock<Project>>;

#[tokio::main]
async fn main() {
    let project = Arc::new(RwLock::new(Project {
        stacks: vec![Arc::new(Layer::default())],
        id_map: UniqueValueMap::new(),
        class_map: ManyToMany::new(),
    }));

    let router = Router::new()
        .route("/", get(|| async { "hello, world" }))
        .route("/", post(new_empty_stack))
        .route("/:base", post(new_stack))
        .route("/:base", patch(write_to_layer))
        .route("/:base/fill_layer", put(add_fill_layer))
        .with_state(project);

    axum::Server::bind(&"127.0.0.1:10810".parse().unwrap())
        .serve(router.into_make_service())
        .await
        .unwrap()
}

async fn new_empty_stack(State(store): State<ServerStore>) -> StatusCode {
    store
        .write()
        .unwrap()
        .stacks
        .push(Arc::new(Layer::default()));
    StatusCode::OK
}

async fn new_stack(
    State(store): State<ServerStore>,
    Path(base): Path<usize>,
) -> (StatusCode, Json<Option<usize>>) {
    if let Some(base) = store.read().unwrap().stacks.get(base) {
        store.write().unwrap().stacks.push(base.clone());
        (
            StatusCode::OK,
            Json(Some(store.read().unwrap().stacks.len())),
        )
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

async fn add_fill_layer(State(store): State<ServerStore>, Path(base): Path<usize>) -> StatusCode {
    if let Some(current) = store.write().unwrap().stacks.get_mut(base) {
        if let Ok(overlayed) = Layer::overlay(current.clone(), LayerConfig::new_fill()) {
            *current = Arc::new(overlayed);
        };
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn write_to_layer(
    State(store): State<ServerStore>,
    Path(base): Path<usize>,
    Json(patch): Json<Molecule>,
) -> StatusCode {
    if let Some(current) = store.write().unwrap().stacks.get_mut(base) {
        let mut updated = current.as_ref().clone();
        if let Ok(_) = updated.write(&patch) {
            *current = Arc::new(updated);
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        }
    } else {
        StatusCode::NOT_FOUND
    }
}
