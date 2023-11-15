use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::Path,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Extension, Json,
};
use nalgebra::{Rotation3, Unit, Vector3};
use serde::Deserialize;

use crate::{
    data_manager::{Layer, Molecule, Stack, WorkspaceStore},
    error::LMECoreError,
    utils::BondGraph,
};

use super::{
    namespace::class_indexes,
    params::{NamePathParam, StackNamePathParam},
};

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
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(patch): Json<Molecule>,
) -> StatusCode {
    match workspace
        .lock()
        .await
        .write_to_layer(stack_id, &patch)
        .await
    {
        Ok(_) => StatusCode::OK,
        Err(err) => match err {
            LMECoreError::NotFillLayer => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
    }
}

pub async fn overlay_to(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(config): Json<Layer>,
) -> (StatusCode, Json<Option<LMECoreError>>) {
    match workspace.lock().await.overlay_to(stack_id, config).await {
        Ok(_) => (StatusCode::OK, Json(None)),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(Some(err))),
    }
}

pub async fn remove_stack(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
) -> StatusCode {
    workspace.lock().await.remove_stack(stack_id);
    StatusCode::OK
}

pub async fn clone_base(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
) -> StatusCode {
    if workspace.lock().await.clone_base(stack_id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

// Complex level APIs
pub async fn rotation_atoms(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(StackNamePathParam { stack_id, name }): Path<StackNamePathParam>,
    Json((center, axis, angle)): Json<([f64; 3], [f64; 3], f64)>,
) -> StatusCode {
    let (_, Json(indexes)) = class_indexes(
        Extension(workspace.clone()),
        Extension(stack.clone()),
        Path(NamePathParam { name }),
    )
    .await;
    let center = Vector3::from(center);
    let axis = Vector3::from(axis);
    let rotation = Rotation3::from_axis_angle(&Unit::new_normalize(axis), angle);
    let rotation_matrix = rotation.matrix();
    let (atoms, _) = stack.read().clone();
    let atoms = atoms
        .into_iter()
        .filter_map(|(idx, atom)| {
            atom.and_then(|atom| {
                if indexes.contains(&idx) {
                    Some((idx, atom))
                } else {
                    None
                }
            })
        })
        .map(|(idx, atom)| {
            (
                idx,
                Some(atom.update_position(|origin| {
                    ((origin - center).transpose() * rotation_matrix).transpose() + center
                })),
            )
        })
        .collect::<HashMap<_, _>>();
    write_to_layer(
        Extension(workspace),
        Path(StackPathParam { stack_id }),
        Json((atoms, BondGraph::new())),
    )
    .await
}
