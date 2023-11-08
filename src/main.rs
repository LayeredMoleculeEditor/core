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
    filter_layer::HIDE_BONDS,
    AtomTable, BondTable, Layer, LayerError,
};
use many_to_many::ManyToMany;
use utils::UniqueValueMap;
use uuid::Uuid;

mod layer;
pub mod utils;

struct Session {
    stack: Vec<(Layer, bool)>,
    id_map: UniqueValueMap<usize, String>,
    class_map: ManyToMany<usize, String>,
    cache: HashMap<Vec<Uuid>, (AtomTable, BondTable)>,
}

impl Session {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            id_map: UniqueValueMap::new(),
            class_map: ManyToMany::new(),
            cache: HashMap::new(),
        }
    }

    fn get_layer_mut(&mut self, idx: usize) -> Result<&mut (Layer, bool), LayerError> {
        if let Some(target) = self.stack.get_mut(idx) {
            Ok(target)
        } else {
            Err(LayerError::NoSuchLayer)
        }
    }

    fn add_layer(&mut self, layer: Layer) -> usize {
        self.stack.push((layer, true));
        self.stack.len()
    }

    fn remove_layer(&mut self, id: usize) -> Result<Layer, LayerError> {
        if self.stack.len() > id {
            Ok(self.stack.remove(id).0)
        } else {
            Err(LayerError::NoSuchLayer)
        }
    }

    fn enable_layer(&mut self, idx: usize) -> Result<(), LayerError> {
        let (_, enabled) = self.get_layer_mut(idx)?;
        *enabled = true;
        Ok(())
    }

    fn disable_layer(&mut self, idx: usize) -> Result<(), LayerError> {
        let (_, enabled) = self.get_layer_mut(idx)?;
        *enabled = false;
        Ok(())
    }

    fn read(&mut self, top: usize, use_cache: bool) -> (AtomTable, BondTable) {
        let visible = self.stack[0..=top]
            .iter()
            .filter_map(|(layer, enabled)| if *enabled { Some(layer) } else { None })
            .collect::<Vec<_>>();
        if let Some((last, base)) = visible.split_last() {
            last.read(
                base,
                if use_cache {
                    Some(&mut self.cache)
                } else {
                    None
                },
            )
        } else {
            (HashMap::new(), HashMap::new())
        }
    }

    fn clear_cache(&mut self) {
        self.cache.clear()
    }

    fn patch(&mut self, idx: usize, patch: (&AtomTable, &BondTable)) -> Result<&Uuid, LayerError> {
        self.get_layer_mut(idx)?.0.patch(patch)
    }
}

type SessionStore = Arc<RwLock<HashMap<String, Arc<RwLock<Session>>>>>;

#[tokio::main]
async fn main() {
    let projects: SessionStore = Arc::new(RwLock::new(HashMap::new()));

    let layers_rt = Router::new()
        .route("/fill", post(create_fill_layer))
        .route("/hide_bonds", post(create_hide_bonds_layer));

    let session_rt = Router::new()
        .route("/", post(create_session))
        .route("/", delete(close_session))
        .nest("/layers", layers_rt);

    let sessions_rt = Router::new()
        .nest("/:session_name", session_rt)
        .route("/", get(list_sessions));

    let app = Router::new()
        .route("/", get(hello_world))
        .nest("/sessions", sessions_rt)
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

async fn create_fill_layer(
    State(store): State<SessionStore>,
    Path(session_name): Path<String>,
) -> (StatusCode, Json<Option<usize>>) {
    let store = store.read().unwrap();
    if let Some(session) = store.get(&session_name) {
        let idx = session.write().unwrap().add_layer(Layer::new_fill_layer());
        (StatusCode::OK, Json(Some(idx)))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

async fn create_hide_bonds_layer(
    State(store): State<SessionStore>,
    Path(session_name): Path<String>,
) -> (StatusCode, Json<Option<usize>>) {
    let store = store.read().unwrap();
    if let Some(session) = store.get(&session_name) {
        let idx = session.write().unwrap().add_layer(Layer::new_filter_layer(
            "remove bonds".to_string(),
            Box::new(HIDE_BONDS),
        ));
        (StatusCode::OK, Json(Some(idx)))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}
