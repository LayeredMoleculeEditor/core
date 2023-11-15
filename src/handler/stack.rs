use std::sync::Arc;

use axum::{Extension, http::{Request, StatusCode}, middleware::Next, response::Response, extract::Path, Json};

use crate::data_manager::{Stack, Molecule, WorkspaceStore};

pub async fn stack_middleware<B>(Extension(workspace): Extension<WorkspaceStore>, Path((_, stack_id)): Path<(String, usize)>, mut req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    if let Some(stack) = workspace.read().await.get_stack(stack_id) {
        req.extensions_mut().insert(stack.clone());
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn read_stack(Extension(stack): Extension<Arc<Stack>>) -> Json<Molecule> {
    Json(
        stack.read().clone()
    )
}


