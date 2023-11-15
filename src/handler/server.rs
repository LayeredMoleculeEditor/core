use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tokio::sync::Mutex;

use crate::{
    data_manager::{create_workspace_store, LayerTree, ServerStore, Workspace},
    utils::{NtoN, UniqueValueMap},
};

pub async fn create_workspace(
    State(store): State<ServerStore>,
    Path(ws): Path<String>,
    Json(load): Json<Option<(LayerTree, HashMap<usize, String>, HashSet<(usize, String)>)>>,
) -> StatusCode {
    if store.read().await.contains_key(&ws) {
        StatusCode::FORBIDDEN
    } else if let Some((layer_tree, id_map, class_map)) = load {
        if let Ok(stacks) = layer_tree.to_stack(None).await {
            if let Ok(id_map) = UniqueValueMap::from_map(id_map) {
                let class_map = NtoN::from(class_map);
                store.write().await.insert(
                    ws,
                    Arc::new(Mutex::new(Workspace::from((stacks, id_map, class_map)))),
                );
                StatusCode::OK
            } else {
                StatusCode::BAD_REQUEST
            }
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    } else {
        store.write().await.insert(ws, create_workspace_store());
        StatusCode::OK
    }
}

pub async fn remove_workspace(
    State(store): State<ServerStore>,
    Path(ws): Path<String>,
) -> StatusCode {
    if store.write().await.remove(&ws).is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
