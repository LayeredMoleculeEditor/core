use std::{collections::HashSet, sync::Arc};

use axum::{extract::Path, http::StatusCode, Extension, Json};

use crate::{
    data_manager::{Stack, WorkspaceStore},
    utils::InsertResult,
};

use super::params::{AtomNamePathParam, AtomPathParam, NamePathParam};

pub async fn list_ids(Extension(workspace): Extension<WorkspaceStore>) -> Json<HashSet<String>> {
    Json(
        workspace
            .lock()
            .await
            .list_ids()
            .into_iter()
            .cloned()
            .collect(),
    )
}

pub async fn set_id(
    Extension(workspace): Extension<WorkspaceStore>,
    Json((idx, id)): Json<(usize, String)>,
) -> (StatusCode, Json<Option<usize>>) {
    match workspace.lock().await.set_id(idx, id) {
        InsertResult::Duplicated(dup_with) => (StatusCode::FORBIDDEN, Json(Some(dup_with))),
        _ => (StatusCode::OK, Json(None)),
    }
}

pub async fn index_to_id(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> (StatusCode, Json<Option<String>>) {
    if let Some(id) = workspace.lock().await.index_to_id(atom_idx) {
        (StatusCode::OK, Json(Some(id)))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

pub async fn remove_id(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> StatusCode {
    workspace.lock().await.remove_id(atom_idx);
    StatusCode::OK
}

pub async fn set_to_class(
    Extension(workspace): Extension<WorkspaceStore>,
    Json((idxs, class)): Json<(Vec<usize>, String)>,
) -> StatusCode {
    let mut workspace = workspace.lock().await;
    for idx in idxs {
        workspace.set_to_class(idx, class.clone());
    }
    StatusCode::OK
}

pub async fn remove_from_class(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomNamePathParam { atom_idx, name }): Path<AtomNamePathParam>,
) -> StatusCode {
    workspace.lock().await.remove_from_class(atom_idx, &name);
    StatusCode::OK
}

pub async fn remove_from_all_class(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> StatusCode {
    workspace.lock().await.remove_from_all_class(atom_idx);
    StatusCode::OK
}

pub async fn remove_class(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> StatusCode {
    workspace.lock().await.remove_class(&name);
    StatusCode::OK
}

pub async fn id_to_index(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> (StatusCode, Json<Option<usize>>) {
    if let Some(idx) = workspace.lock().await.id_to_index(&name) {
        if let Some(Some(_)) = stack.read().0.get(&idx) {
            (StatusCode::OK, Json(Some(idx)))
        } else {
            (StatusCode::NOT_FOUND, Json(None))
        }
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

pub async fn list_classes(
    Extension(workspace): Extension<WorkspaceStore>,
) -> Json<HashSet<String>> {
    Json(
        workspace
            .lock()
            .await
            .list_classes()
            .into_iter()
            .cloned()
            .collect(),
    )
}

pub async fn get_classes(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> (StatusCode, Json<Vec<String>>) {
    let classes = workspace
        .lock()
        .await
        .get_classes(atom_idx)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(classes))
}

pub async fn class_indexes(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> (StatusCode, Json<Vec<usize>>) {
    let workspace = workspace.lock().await;
    let indexes = workspace.class_indexes(&name);
    let indexes = stack
        .read()
        .0
        .iter()
        .filter(|(idx, atom)| indexes.contains(idx) && atom.is_some())
        .map(|(idx, _)| idx)
        .cloned()
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(indexes))
}
