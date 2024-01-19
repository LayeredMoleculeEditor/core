mod state_handler {
    use std::sync::Arc;

    use axum::{
        extract::{Path, State},
        http::{Request, StatusCode},
        middleware::Next,
        response::{IntoResponse, Response},
    };
    use lme_core::Workspace;
    use serde::Deserialize;
    use tokio::sync::Mutex;

    use crate::ServerState;

    #[derive(Deserialize)]
    pub struct WorkspaceParam {
        ws: String,
    }

    pub async fn create_workspace(
        State(state): State<ServerState>,
        Path(WorkspaceParam { ws }): Path<WorkspaceParam>,
    ) -> StatusCode {
        let mut state = state.write().await;
        if state.contains_key(&ws) {
            StatusCode::CONFLICT
        } else {
            state.insert(ws, Arc::new(Mutex::new(Workspace::new())));
            StatusCode::OK
        }
    }

    pub async fn remove_workspace(
        State(state): State<ServerState>,
        Path(WorkspaceParam { ws }): Path<WorkspaceParam>,
    ) -> StatusCode {
        let mut state = state.write().await;
        if state.remove(&ws).is_some() {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        }
    }

    pub async fn workspace_middleware<B>(
        State(state): State<ServerState>,
        Path(WorkspaceParam { ws }): Path<WorkspaceParam>,
        mut req: Request<B>,
        next: Next<B>,
    ) -> Response {
        let workspace = state.read().await.get(&ws).cloned();
        if let Some(workspace) = workspace {
            req.extensions_mut().insert(workspace);
            next.run(req).await
        } else {
            (StatusCode::NOT_FOUND, "No such workspace").into_response()
        }
    }
}

mod workspace_handler {
    use axum::{
        http::StatusCode,
        response::{ErrorResponse, Result},
    };
    use std::{ops::Deref, sync::Arc};

    use axum::{extract::Query, Extension, Json};
    use lme_core::{
        entity::{Layer, Molecule, Stack},
        WorkspaceExport,
    };
    use serde::Deserialize;

    use crate::WorkspaceAccessor;

    pub async fn stacks(Extension(workspace): Extension<WorkspaceAccessor>) -> Json<usize> {
        Json(workspace.lock().await.stacks())
    }

    #[derive(Deserialize)]
    pub struct StackCreationParam {
        copies: usize,
    }

    impl Default for StackCreationParam {
        fn default() -> Self {
            Self { copies: 1 }
        }
    }

    pub async fn create_stack(
        Extension(workspace): Extension<WorkspaceAccessor>,
        Query(StackCreationParam { copies }): Query<StackCreationParam>,
    ) -> Json<usize> {
        let mut workspace = workspace.lock().await;
        Json(workspace.create_stack(Arc::new(Stack::new(vec![])), copies))
    }

    #[derive(Deserialize)]
    pub struct WriteToStack {
        start_idx: usize,
        copies: usize,
        data: Molecule,
    }

    pub async fn write_to_stack(
        Extension(workspace): Extension<WorkspaceAccessor>,
        Json(WriteToStack {
            start_idx,
            copies,
            data,
        }): Json<WriteToStack>,
    ) -> Json<bool> {
        Json(
            workspace
                .lock()
                .await
                .write_to_stack(start_idx, copies, data),
        )
    }

    #[derive(Deserialize)]
    pub struct AddLayerToStack {
        start_idx: usize,
        copies: usize,
        layer: Layer,
    }

    pub async fn add_layer_to_stack(
        Extension(workspace): Extension<WorkspaceAccessor>,
        Json(AddLayerToStack {
            start_idx,
            copies,
            layer,
        }): Json<AddLayerToStack>,
    ) -> Json<bool> {
        Json(
            workspace
                .lock()
                .await
                .add_layer_to_stack(start_idx, copies, Arc::new(layer)),
        )
    }

    #[derive(Deserialize)]
    pub struct CloneStack {
        stack_idx: usize,
        copies: usize,
    }

    pub async fn clone_stack(
        Extension(workspace): Extension<WorkspaceAccessor>,
        Json(CloneStack { stack_idx, copies }): Json<CloneStack>,
    ) -> Result<Json<usize>> {
        workspace
            .lock()
            .await
            .clone_stack(stack_idx, copies)
            .map(|start| Json(start))
            .ok_or(ErrorResponse::from(StatusCode::NOT_FOUND))
    }

    pub async fn clone_base(
        Extension(workspace): Extension<WorkspaceAccessor>,
        Json(CloneStack { stack_idx, copies }): Json<CloneStack>,
    ) -> Result<Json<usize>> {
        workspace
            .lock()
            .await
            .clone_base(stack_idx, copies)
            .map(|start| Json(start))
            .ok_or(ErrorResponse::from(StatusCode::NOT_FOUND))
    }

    pub async fn workspace_export(
        Extension(workspace): Extension<WorkspaceAccessor>,
    ) -> Json<WorkspaceExport> {
        Json(WorkspaceExport::from(workspace.lock().await.deref()))
    }
}

pub use state_handler::*;
pub use workspace_handler::*;
