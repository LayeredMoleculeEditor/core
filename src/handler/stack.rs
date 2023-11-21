use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::Path,
    http::{Request, StatusCode},
    middleware::Next,
    response::{ErrorResponse, Response, Result},
    Extension, Json,
};
use nalgebra::{Rotation3, Unit, Vector3};
use nanoid::nanoid;
use rayon::prelude::*;
use serde::Deserialize;

use crate::{
    data_manager::{clean_molecule, Atom, CleanedMolecule, Layer, Molecule, Stack, WorkspaceStore},
    error::LMECoreError,
    utils::{vector_align_rotation, BondGraph, Pair},
};

use super::{
    namespace::{class_indexes, set_to_class},
    params::{AtomPathParam, NamePathParam, StackNamePathParam},
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
) -> Result<Response, LMECoreError> {
    // unlock the workspace immediately after insert stack to extensions
    {
        let workspace = workspace.lock().await;
        let stack = workspace.get_stack(stack_id)?;
        req.extensions_mut().insert(stack.clone());
    }
    Ok(next.run(req).await)
}

pub async fn read_stack(Extension(stack): Extension<Arc<Stack>>) -> Json<Molecule> {
    Json(stack.read().clone())
}

pub async fn read_cleaned(Extension(stack): Extension<Arc<Stack>>) -> Json<CleanedMolecule> {
    Json(clean_molecule(stack.read().clone()))
}

pub async fn get_neighbors(
    Extension(stack): Extension<Arc<Stack>>,
    Path(AtomPathParam { atom_idx }): Path<AtomPathParam>,
) -> Result<Json<Vec<(usize, f64)>>, LMECoreError> {
    let (atoms, bonds) = stack.read();
    if let Some(Some(_)) = atoms.get(&atom_idx) {
        let neighbors = bonds
            .into_iter()
            .par_bridge()
            .filter_map(|(pair, bond)| pair.get_another(&atom_idx).zip(bond.as_ref()))
            .filter(|(another, _)| atoms.get(another).and_then(|atom| atom.as_ref()).is_some())
            .map(|(another, bond)| (*another, *bond))
            .collect::<Vec<_>>();
        Ok(Json(neighbors))
    } else {
        Err(LMECoreError::NoSuchAtom)
    }
}

pub async fn is_writable(Extension(stack): Extension<Arc<Stack>>) -> Json<bool> {
    if let Layer::Fill { .. } = stack.top() {
        Json(true)
    } else {
        Json(false)
    }
}

pub async fn write_to_layer(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(patch): Json<Molecule>,
) -> Result<(), LMECoreError> {
    workspace
        .lock()
        .await
        .write_to_layer(stack_id, &patch)
        .await
}

pub async fn overlay_to(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(config): Json<Layer>,
) -> Result<(), LMECoreError> {
    workspace.lock().await.overlay_to(stack_id, config).await
}

pub async fn remove_stack(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
) -> Result<()> {
    workspace.lock().await.remove_stack(stack_id);
    Ok(())
}

pub async fn clone_stack(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
) -> Result<Json<usize>, LMECoreError> {
    Ok(Json(workspace.lock().await.clone_stack(stack_id)?))
}

pub async fn clone_base(
    Extension(workspace): Extension<WorkspaceStore>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
) -> Result<Json<usize>, LMECoreError> {
    Ok(Json(workspace.lock().await.clone_base(stack_id)?))
}

// Complex level APIs
pub async fn rotation_atoms(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(StackNamePathParam { stack_id, name }): Path<StackNamePathParam>,
    Json((center, axis, angle)): Json<([f64; 3], [f64; 3], f64)>,
) -> Result<(), LMECoreError> {
    let Json(indexes) = class_indexes(
        Extension(workspace.clone()),
        Extension(stack.clone()),
        Path(NamePathParam { name }),
    )
    .await;
    let center = Vector3::from(center);
    let axis = Vector3::from(axis);
    let rotation = Rotation3::from_axis_angle(&Unit::new_normalize(axis), angle);
    let rotation_matrix = rotation.matrix();
    let atoms = stack
        .read()
        .clone()
        .0
        .into_par_iter()
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

pub async fn translation_atoms(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(StackNamePathParam { stack_id, name }): Path<StackNamePathParam>,
    Json(vector): Json<[f64; 3]>,
) -> Result<(), LMECoreError> {
    let Json(indexes) = class_indexes(
        Extension(workspace.clone()),
        Extension(stack.clone()),
        Path(NamePathParam { name }),
    )
    .await;
    let vector = Vector3::from(vector);
    let atoms = stack
        .read()
        .0
        .clone()
        .into_par_iter()
        .filter_map(|(idx, atom)| {
            atom.and_then(|atom| {
                if indexes.contains(&idx) {
                    Some((idx, Some(atom.update_position(|origin| origin + vector))))
                } else {
                    None
                }
            })
        })
        .collect::<HashMap<_, _>>();
    write_to_layer(
        Extension(workspace),
        Path(StackPathParam { stack_id }),
        Json((atoms, BondGraph::new())),
    )
    .await
}

pub async fn import_structure(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(StackNamePathParam { stack_id, name }): Path<StackNamePathParam>,
    Json((atoms, bonds)): Json<CleanedMolecule>,
) -> Result<Json<Vec<usize>>> {
    let offset = stack.read().0.keys().max().unwrap_or(&0) + 1;
    let atoms_patch = atoms
        .into_par_iter()
        .enumerate()
        .map(|(idx, atom)| (idx + offset, Some(atom)))
        .collect::<HashMap<_, _>>();
    let mut bonds_patch = BondGraph::from(bonds);
    bonds_patch.offset(offset);
    let atom_idxs = atoms_patch.keys().cloned().collect::<Vec<_>>();
    write_to_layer(
        Extension(workspace.clone()),
        Path(StackPathParam { stack_id }),
        Json((atoms_patch, bonds_patch)),
    )
    .await?;
    set_to_class(Extension(workspace), Json((atom_idxs.clone(), name))).await?;
    Ok(Json(atom_idxs))
}

#[derive(Deserialize)]
pub struct AddSubstitute {
    atoms: Vec<Atom>,
    bond: HashMap<Pair<usize>, f64>,
    current: (usize, usize),
    target: (usize, usize),
    class_name: Option<String>,
}

pub async fn add_substitute(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(configuration): Json<AddSubstitute>,
) -> Result<Json<String>> {
    let atoms = &stack.read().0;
    let target_atoms = atoms
        .get(&configuration.target.0)
        .map(|item| item.as_ref())
        .flatten()
        .zip(
            atoms
                .get(&configuration.target.1)
                .map(|item| item.as_ref())
                .flatten(),
        );

    let current_atoms = configuration
        .atoms
        .get(configuration.current.0)
        .zip(configuration.atoms.get(configuration.current.1));

    if let Some(((base_entry, base_center), (sub_entry, sub_center))) =
        target_atoms.zip(current_atoms)
    {
        let base_direction = base_center.get_position() - base_entry.get_position();
        let sub_direction = sub_center.get_position() - sub_entry.get_position();
        let (axis, angle) = vector_align_rotation(&sub_direction, &base_direction);
        let matrix = *Rotation3::from_axis_angle(&Unit::new_normalize(axis), angle).matrix();
        let center = *sub_center.get_position();
        let translation_vector = base_center.get_position() - sub_center.get_position();
        let atoms = configuration
            .atoms
            .into_par_iter()
            .map(|atom| {
                atom.update_position(|origin| {
                    ((origin - center).transpose() * matrix).transpose() + center
                })
            })
            .map(|atom| atom.update_position(|origin| origin + translation_vector))
            .collect::<Vec<_>>();
        let temp_class = configuration.class_name.unwrap_or(nanoid!());
        let Json(indexes) = import_structure(
            Extension(workspace.clone()),
            Extension(stack.clone()),
            Path(StackNamePathParam {
                stack_id,
                name: temp_class.clone(),
            }),
            Json((atoms, configuration.bond)),
        )
        .await?;
        let to_remove = indexes
            .get(configuration.current.0)
            .zip(indexes.get(configuration.current.0));
        if let Some((entry_idx, center_idx)) = to_remove {
            let center_atom = stack.read().0.get(center_idx).unwrap().unwrap();
            let atoms_patch = HashMap::from([
                (*entry_idx, None),
                (*center_idx, None),
                (configuration.target.1, Some(center_atom)),
            ]);
            let bonds_to_modify = (&stack.read().1)
                .into_iter()
                .par_bridge()
                .filter_map(|(pair, bond)| pair.get_another(center_idx).cloned().zip(bond.clone()))
                .collect::<Vec<_>>();
            let mut bonds_patch = bonds_to_modify
                .iter()
                .par_bridge()
                .map(|(neighbor, _)| (Pair::from((*center_idx, *neighbor)), None))
                .collect::<HashMap<_, _>>();
            let bonds_to_create = bonds_to_modify
                .into_par_iter()
                .map(|(neighbor, bond)| {
                    (Pair::from((configuration.target.1, neighbor)), Some(bond))
                })
                .collect::<HashMap<_, _>>();
            bonds_patch.extend(bonds_to_create);
            let bonds_patch = BondGraph::from(bonds_patch);
            write_to_layer(
                Extension(workspace.clone()),
                Path(StackPathParam { stack_id }),
                Json((atoms_patch, bonds_patch)),
            )
            .await?;
            set_to_class(
                Extension(workspace),
                Json((vec![configuration.target.1], temp_class.clone())),
            )
            .await?;

            Ok(Json(temp_class))
        } else {
            Err(ErrorResponse::from((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to found atoms imported to stack",
            )))
        }
    } else {
        Err(ErrorResponse::from((
            StatusCode::NOT_ACCEPTABLE,
            "Failed to found current or target atoms to replace",
        )))
    }
}
