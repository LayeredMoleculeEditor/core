use std::{collections::HashSet, sync::Arc};

use axum::{extract::Path, response::Result, Extension, Json};
use rayon::prelude::*;

use crate::{
    data_manager::{Stack, Workspace},
    error::LMECoreError,
    utils::InsertResult,
};

use super::params::{AtomNamePathParam, AtomPathParam, NamePathParam};

pub async fn list_ids(Extension(workspace): Extension<Workspace>) -> Json<HashSet<String>> {
    Json(workspace.list_ids().await)
}

pub async fn set_id(
    Extension(workspace): Extension<Workspace>,
    Json((idx, id)): Json<(usize, String)>,
) -> Result<(), LMECoreError> {
    match workspace.set_id(idx, id).await {
        InsertResult::Duplicated(_) => Err(LMECoreError::IdMapUniqueError),
        _ => Ok(()),
    }
}

pub async fn index_to_id(
    Extension(workspace): Extension<Workspace>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Result<Json<String>, LMECoreError> {
    if let Some(id) = workspace.index_to_id(atom_idx).await {
        Ok(Json(id))
    } else {
        Err(LMECoreError::NoSuchId)
    }
}

pub async fn remove_id(
    Extension(workspace): Extension<Workspace>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Result<()> {
    Ok(workspace.remove_id(atom_idx).await)
}

pub async fn set_to_class(
    Extension(workspace): Extension<Workspace>,
    Json((idxs, class)): Json<(Vec<usize>, String)>,
) -> Result<()> {
    for idx in idxs {
        workspace.set_to_class(idx, class.clone()).await;
    }
    Ok(())
}

pub async fn remove_from_class(
    Extension(workspace): Extension<Workspace>,
    Path(AtomNamePathParam { atom_idx, name }): Path<AtomNamePathParam>,
) -> Result<()> {
    Ok(workspace.remove_from_class(atom_idx, &name).await)
}

pub async fn remove_from_all_class(
    Extension(workspace): Extension<Workspace>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Result<()> {
    Ok(workspace.remove_from_all_class(atom_idx).await)
}

pub async fn remove_class(
    Extension(workspace): Extension<Workspace>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> Result<()> {
    Ok(workspace.remove_class(&name).await)
}

pub async fn id_to_index(
    Extension(workspace): Extension<Workspace>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> Result<Json<usize>, LMECoreError> {
    if let Some(idx) = workspace.id_to_index(&name).await {
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

pub async fn list_classes(Extension(workspace): Extension<Workspace>) -> Json<HashSet<String>> {
    Json(workspace.list_classes().await)
}

pub async fn get_classes(
    Extension(workspace): Extension<Workspace>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Json<HashSet<String>> {
    Json(workspace.get_classes(atom_idx).await)
}

pub async fn class_indexes(
    Extension(workspace): Extension<Workspace>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(NamePathParam { name }): Path<NamePathParam>,
) -> Json<Vec<usize>> {
    let indexes = workspace.class_indexes(&name).await;
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
