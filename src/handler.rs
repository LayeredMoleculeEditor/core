mod state_handler {
    use std::sync::Arc;

    use axum::{
        extract::{Path, State},
        http::{Request, StatusCode},
        middleware::Next,
        response::{IntoResponse, Response},
        Json,
    };
    use lme_core::{entity::Molecule, Workspace};
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
        Json(base): Json<Molecule>,
    ) -> StatusCode {
        let mut state = state.write().await;
        if state.contains_key(&ws) {
            StatusCode::CONFLICT
        } else {
            state.insert(ws, Arc::new(Mutex::new(Workspace::new(base))));
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

    #[derive(Deserialize)]
    pub struct StacksSelect {
        pub start: usize,
        pub range: usize,
    }

    pub async fn read_stacks(
        Extension(workspace): Extension<WorkspaceAccessor>,
        Query(StacksSelect { start, range }): Query<StacksSelect>,
    ) -> Result<Json<Vec<Molecule>>> {
        let workspace = workspace.lock().await;
        (start..start + range)
            .map(|index| workspace.read(index))
            .collect::<Option<Vec<_>>>()
            .map(|result| Json(result))
            .ok_or(ErrorResponse::from(StatusCode::NOT_FOUND))
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

    pub async fn write_to_stack(
        Extension(workspace): Extension<WorkspaceAccessor>,
        Query(StacksSelect { start, range }): Query<StacksSelect>,
        Json(data): Json<Molecule>
    ) -> Json<bool> {
        Json(
            workspace
                .lock()
                .await
                .write_to_stack(start, range, data),
        )
    }

    pub async fn add_layer_to_stack(
        Extension(workspace): Extension<WorkspaceAccessor>,
        Query(StacksSelect { start, range }): Query<StacksSelect>,
        Json(layer): Json<Layer>
    ) -> Json<bool> {
        Json(
            workspace
                .lock()
                .await
                .add_layer_to_stack(start, range, Arc::new(layer)),
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

mod chemistry_handler {
    use std::collections::HashMap;

    use axum::{extract::Query, Extension, Json};

    use crate::{StacksSelect, WorkspaceAccessor};

    pub fn modify_bonds(Extension(workspace): Extension<WorkspaceAccessor>, Query(StacksSelect {start, range}): Query<StacksSelect>, Json(bonds): Json<HashMap<Pair<usize>, f64>>) -> Json<bool> {}
}

pub use state_handler::*;
pub use workspace_handler::*;
