use std::sync::Arc;

use axum::{
    extract::Path,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Extension, Json,
};
use serde::Deserialize;

use crate::data_manager::{Layer, Molecule, Stack, WorkspaceError, WorkspaceStore};

#[derive(Deserialize)]
pub struct StackPathParam {
    stack_id: usize,
}

pub async fn stack_middleware<B>(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    mut req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    if let Some(stack) = workspace.lock().await.get_stack(stack_id) {
        req.extensions_mut().insert(stack.clone());
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn read_stack(Extension(stack): Extension<Arc<Stack>>) -> Json<Molecule> {
    Json(stack.read().clone())
}

pub async fn write_to_layer(
    Extension(workspace): Extension<WorkspaceStore>,
    // Extension(stack): Extension<Arc<Stack>>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(patch): Json<Molecule>,
) -> StatusCode {
    match workspace.lock().await.write_to_layer(stack_id, &patch) {
        Ok(_) => StatusCode::OK,
        Err(err) => match err {
            WorkspaceError::NotFillLayer => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
    }
}

pub async fn overlay_to(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(config): Json<Layer>,
) -> (StatusCode, Json<Option<String>>) {
    match workspace.lock().await.overlay_to(stack_id, config) {
        Ok(_) => (StatusCode::OK, Json(None)),
        Err(err) => match err {
            WorkspaceError::PluginError(reason) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(Some(reason)))
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(None)),
        },
    }
}

pub async fn remove_stack(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>
) -> StatusCode {
    workspace.lock().await.remove_stack(stack_id);
    StatusCode::OK
}

pub async fn clone_base(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>
) -> StatusCode {
    if workspace.lock().await.clone_base(stack_id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
