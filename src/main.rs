use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use layer::{Layer, LayerTree, Molecule, Stack};

use utils::{InsertResult, NtoN, UniqueValueMap};

mod layer;
pub mod serde;
mod utils;

struct Workspace {
    stacks: Vec<Arc<Stack>>,
    id_map: UniqueValueMap<usize, String>,
    class_map: NtoN<usize, String>,
}

type ServerStore = Arc<RwLock<Workspace>>;

#[tokio::main]
async fn main() {
    let project = Arc::new(RwLock::new(Workspace {
        stacks: vec![Arc::new(Stack::default())],
        id_map: UniqueValueMap::new(),
        class_map: NtoN::new(),
    }));

    let router = Router::new()
        .route("/load", put(load_workspace))
        
        .route("/export", get(export_workspace))
        .route(
            "/stacks",
            get(|State(store): State<ServerStore>| async move {
                Json(
                    store
                        .read()
                        .unwrap()
                        .stacks
                        .iter()
                        .map(|stack| stack.len())
                        .collect::<Vec<_>>(),
                )
            }),
        )
        .route("/stacks", post(new_empty_stack))
        .route("/stacks/:idx", get(read_stack))
        .route("/stacks/:idx", patch(write_to_layer))
        .route("/stacks/:idx", put(overlay_to))
        .route("/atoms/:idx/id/:id", post(set_id))
        .route("/atoms/:idx/class/:class", post(set_to_group))
        .route("/atoms/:idx/class/:class", delete(remove_from_group))
        .route("/atoms/:idx/class", delete(remove_from_all_group))
        .route("/ids/:idx", delete(remove_id))
        .route("/classes/:class", delete(remove_group))
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
        .push(Arc::new(Stack::default()));
    StatusCode::OK
}

async fn overlay_to(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Json(config): Json<Layer>,
) -> StatusCode {
    if let Some(current) = store.write().unwrap().stacks.get_mut(idx) {
        if let Ok(overlayed) = Stack::overlay(Some(current.clone()), config) {
            *current = Arc::new(overlayed);
            StatusCode::OK
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn write_to_layer(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Json(patch): Json<Molecule>,
) -> StatusCode {
    if let Some(current) = store.write().unwrap().stacks.get_mut(idx) {
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

async fn set_id(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Option<usize>>) {
    if let InsertResult::Duplicated(duplicated_with) = store.write().unwrap().id_map.insert(idx, id)
    {
        (StatusCode::BAD_REQUEST, Json(Some(duplicated_with)))
    } else {
        (StatusCode::OK, Json(None))
    }
}

async fn set_to_group(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Path(class): Path<String>,
) -> StatusCode {
    store.write().unwrap().class_map.insert(idx, class);
    StatusCode::OK
}

async fn remove_id(State(store): State<ServerStore>, Path(idx): Path<usize>) -> StatusCode {
    store.write().unwrap().id_map.remove(&idx);
    StatusCode::OK
}

async fn remove_from_group(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Path(class): Path<String>,
) -> StatusCode {
    store.write().unwrap().class_map.remove(&idx, &class);
    StatusCode::OK
}

async fn remove_from_all_group(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
) -> StatusCode {
    store.write().unwrap().class_map.remove_left(&idx);
    StatusCode::OK
}

async fn remove_group(State(store): State<ServerStore>, Path(class): Path<String>) -> StatusCode {
    store.write().unwrap().class_map.remove_right(&class);
    StatusCode::OK
}

async fn export_workspace(
    State(store): State<ServerStore>,
) -> Json<(LayerTree, HashMap<usize, String>, HashSet<(usize, String)>)> {
    let store = store.read().unwrap();
    let mut layer_tree = LayerTree::from(store.stacks[0].as_ref().clone());
    for stack in &store.stacks[1..] {
        layer_tree
            .merge(stack.get_layers())
            .expect("Layers in workspace has same white idx");
    }
    let ids = store.id_map.data().clone();
    let classes = store.class_map.data().clone();
    Json((layer_tree, ids, classes))
}

async fn load_workspace(
    State(store): State<ServerStore>,
    Json((layer_tree, ids, classes)): Json<(
        LayerTree,
        HashMap<usize, String>,
        HashSet<(usize, String)>,
    )>,
) -> StatusCode {
    if let Ok((root, mut others)) = layer_tree.to_stack(None) {
        let mut stacks = vec![root];
        stacks.append(&mut others);
        if let Ok(id_map) = UniqueValueMap::from_map(ids) {
            let class_map = NtoN::from(classes);
            let updated = Workspace {
                stacks,
                id_map,
                class_map,
            };
            *store.write().unwrap() = updated;
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        }
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn read_stack(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
) -> (StatusCode, Json<Option<Molecule>>) {
    if let Some(stack) = store.read().unwrap().stacks.get(idx) {
        (StatusCode::OK, Json(Some(stack.read().clone())))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}
