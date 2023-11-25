use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Extension, Json,
};
use serde::Deserialize;

use crate::data_manager::{LayerTree, ServerStore, Workspace};

#[derive(Deserialize)]
pub struct WorkspacePathParam {
    ws: String,
}

pub async fn workspace_middleware<B>(
    State(store): State<ServerStore>,
    Path(WorkspacePathParam { ws }): Path<WorkspacePathParam>,
    mut req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    if let Some(workspace) = store.read().await.get(&ws) {
        req.extensions_mut().insert(workspace.clone());
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn export_workspace(
    Extension(workspace): Extension<Workspace>,
) -> Json<(LayerTree, HashMap<usize, String>, HashSet<(usize, String)>)> {
    Json(workspace.export().await)
}

pub async fn read_stacks(Extension(workspace): Extension<Workspace>) -> Json<Vec<usize>> {
    Json(workspace.get_all_stack().await)
}

pub async fn new_stack(Extension(workspace): Extension<Workspace>) -> StatusCode {
    workspace.new_empty_stack().await;
    StatusCode::OK
}
