use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::{
    extract::{Path, State},
    Json,
};
use tokio::sync::RwLock;

use crate::{
    data_manager::{create_workspace_store, LayerTree, ServerStore, Workspace},
    error::LMECoreError,
    utils::{NtoN, UniqueValueMap},
};

pub async fn create_workspace(
    State(store): State<ServerStore>,
    Path(ws): Path<String>,
    Json(load): Json<Option<(LayerTree, HashMap<usize, String>, HashSet<(usize, String)>)>>,
) -> Result<(), LMECoreError> {
    if store.read().await.contains_key(&ws) {
        Err(LMECoreError::WorkspaceNameConflict)
    } else if let Some((layer_tree, id_map, class_map)) = load {
        let stacks = layer_tree.to_stack(None).await?;
        let id_map =
            UniqueValueMap::from_map(id_map).map_err(|_| LMECoreError::IdMapUniqueError)?;
        let class_map = NtoN::from(class_map);
        store.write().await.insert(
            ws,
            Arc::new(RwLock::new(Workspace::from((stacks, id_map, class_map)))),
        );
        Ok(())
    } else {
        store.write().await.insert(ws, create_workspace_store());
        Ok(())
    }
}

pub async fn remove_workspace(
    State(store): State<ServerStore>,
    Path(ws): Path<String>,
) -> Result<(), LMECoreError> {
    if store.write().await.remove(&ws).is_some() {
        Ok(())
    } else {
        Err(LMECoreError::WorkspaceNotFound)
    }
}
