use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use data_manager::{Layer, LayerTree, Molecule, Workspace, create_server_store, ServerStore, WorkspaceError};

use utils::{InsertResult, NtoN, UniqueValueMap};

mod data_manager;
pub mod serde;
mod utils;

#[tokio::main]
async fn main() {
    let project = create_server_store();

    let router = Router::new()
        .route("/load", put(load_workspace))
        .route("/export", get(export_workspace))
        .route("/stacks", get(get_stacks))
        .route("/stacks", post(new_empty_stack))
        .route("/stacks/:idx", get(read_stack))
        .route("/stacks/:idx", patch(write_to_layer))
        .route("/stacks/:idx", put(overlay_to))
        .route("/atoms/:idx/id/:id", post(set_id))
        .route("/atoms/:idx/class/:class", post(set_to_class))
        .route("/atoms/:idx/class/:class", delete(remove_from_class))
        .route("/atoms/:idx/class", delete(remove_from_all_class))
        .route("/ids/:idx", delete(remove_id))
        .route("/classes/:class", delete(remove_class))
        .with_state(project);

    axum::Server::bind(&"127.0.0.1:10810".parse().unwrap())
        .serve(router.into_make_service())
        .await
        .unwrap()
}

async fn get_stacks(State(store): State<ServerStore>) -> Json<Vec<usize>> {
    Json(
        store.read().unwrap().get_stacks()
    )
}

async fn new_empty_stack(State(store): State<ServerStore>) -> StatusCode {
    store.write().unwrap().new_empty_stack();
    StatusCode::OK
}

async fn overlay_to(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Json(config): Json<Layer>,
) -> StatusCode {
    match store.write().unwrap().overlay_to(idx, config) {
        Ok(_) => StatusCode::OK,
        Err(err) => match err {
            WorkspaceError::NoSuchStack => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn write_to_layer(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Json(patch): Json<Molecule>,
) -> StatusCode {
    match store.write().unwrap().write_to_layer(idx, &patch) {
        Ok(_) => StatusCode::OK,
        Err(err) => match err {
            WorkspaceError::NotFillLayer => StatusCode::BAD_REQUEST,
            WorkspaceError::NoSuchStack => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR
        }
        
    }
}

async fn set_id(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Option<usize>>) {
    if let InsertResult::Duplicated(duplicated_with) = store.write().unwrap().set_id(idx, id)
    {
        (StatusCode::BAD_REQUEST, Json(Some(duplicated_with)))
    } else {
        (StatusCode::OK, Json(None))
    }
}

async fn set_to_class(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Path(class): Path<String>,
) -> StatusCode {
    store.write().unwrap().set_to_class(idx, class);
    StatusCode::OK
}

async fn remove_id(State(store): State<ServerStore>, Path(idx): Path<usize>) -> StatusCode {
    store.write().unwrap().remove_id(idx);
    StatusCode::OK
}

async fn remove_from_class(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
    Path(class): Path<String>,
) -> StatusCode {
    store.write().unwrap().remove_from_class(idx, &class);
    StatusCode::OK
}

async fn remove_from_all_class(
    State(store): State<ServerStore>,
    Path(idx): Path<usize>,
) -> StatusCode {
    store.write().unwrap().remove_from_all_class(idx);
    StatusCode::OK
}

async fn remove_class(State(store): State<ServerStore>, Path(class): Path<String>) -> StatusCode {
    store.write().unwrap().remove_class(&class);
    StatusCode::OK
}

async fn export_workspace(
    State(store): State<ServerStore>,
) -> Json<(LayerTree, HashMap<usize, String>, HashSet<(usize, String)>)> {
    Json(store.read().unwrap().export())
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
            *store.write().unwrap() = Workspace::from((stacks, id_map, class_map));
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
    if let Some(stack) = store.read().unwrap().get_stack(idx) {
        (StatusCode::OK, Json(Some(stack.read().clone())))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}
