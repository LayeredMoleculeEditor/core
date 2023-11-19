use std::{collections::HashSet, sync::Arc};

use axum::{extract::Path, response::Result, Extension, Json};
use rayon::prelude::*;

use crate::{
    data_manager::{Stack, WorkspaceStore},
    error::LMECoreError,
    utils::InsertResult,
};

use super::params::{AtomNamePathParam, AtomPathParam, NamePathParam};

pub async fn list_ids(Extension(workspace): Extension<WorkspaceStore>) -> Json<HashSet<String>> {
    Json(
        workspace
            .lock()
            .await
            .list_ids()
            .into_par_iter()
            .cloned()
            .collect(),
    )
}

pub async fn set_id(
    Extension(workspace): Extension<WorkspaceStore>,
    Json((idx, id)): Json<(usize, String)>,
) -> Result<(), LMECoreError> {
    match workspace.lock().await.set_id(idx, id) {
        InsertResult::Duplicated(_) => Err(LMECoreError::IdMapUniqueError),
        _ => Ok(()),
    }
}

pub async fn index_to_id(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Result<Json<String>, LMECoreError> {
    if let Some(id) = workspace.lock().await.index_to_id(atom_idx) {
        Ok(Json(id))
    } else {
        Err(LMECoreError::NoSuchId)
    }
}

pub async fn remove_id(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Result<()> {
    workspace.lock().await.remove_id(atom_idx);
    Ok(())
}

pub async fn set_to_class(
    Extension(workspace): Extension<WorkspaceStore>,
    Json((idxs, class)): Json<(Vec<usize>, String)>,
) -> Result<()> {
    let mut workspace = workspace.lock().await;
    for idx in idxs {
        workspace.set_to_class(idx, class.clone());
    }
    Ok(())
}

pub async fn remove_from_class(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomNamePathParam { atom_idx, name }): Path<AtomNamePathParam>,
) -> Result<()> {
    workspace.lock().await.remove_from_class(atom_idx, &name);
    Ok(())
}

pub async fn remove_from_all_class(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Result<()> {
    workspace.lock().await.remove_from_all_class(atom_idx);
    Ok(())
}

pub async fn remove_class(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> Result<()> {
    workspace.lock().await.remove_class(&name);
    Ok(())
}

pub async fn id_to_index(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> Result<Json<usize>, LMECoreError> {
    if let Some(idx) = workspace.lock().await.id_to_index(&name) {
        if let Some(atom) = stack.read().0.get(&idx) {
            if let Some(_) = atom {
                Ok(Json(idx))
            } else {
                Err(LMECoreError::NoSuchAtom)
            }
        } else {
            Err(LMECoreError::NoSuchAtom)
        }
    } else {
        Err(LMECoreError::NoSuchId)
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
            .into_par_iter()
            .cloned()
            .collect(),
    )
}

pub async fn get_classes(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Json<Vec<String>> {
    let classes = workspace
        .lock()
        .await
        .get_classes(atom_idx)
        .into_par_iter()
        .cloned()
        .collect::<Vec<_>>();
    Json(classes)
}

pub async fn class_indexes(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> Json<Vec<usize>> {
    let workspace = workspace.lock().await;
    let indexes = workspace.class_indexes(&name);
    let indexes = stack
        .read()
        .0
        .par_iter()
        .filter(|(idx, atom)| indexes.contains(idx) && atom.is_some())
        .map(|(idx, _)| idx)
        .cloned()
        .collect::<Vec<_>>();
    Json(indexes)
}
