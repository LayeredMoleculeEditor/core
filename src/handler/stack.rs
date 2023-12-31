use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::Path,
    http::{Request, StatusCode},
    middleware::Next,
    response::{ErrorResponse, Response, Result},
    Extension, Json,
};
use nalgebra::{Rotation3, Transform3, Unit, Vector3, Matrix4, Point3};
use nanoid::nanoid;
use rayon::prelude::*;
use serde::Deserialize;

use crate::{
    data_manager::{clean_molecule, CompactedMolecule, Layer, Molecule, Stack, Workspace},
    error::LMECoreError,
    utils::{vector_align_rotation, BondGraph, Pair},
};

use super::{
    namespace::{class_indexes, set_to_class},
    params::{AtomPathParam, NamePathParam, StackNamePathParam}, workspace,
};

#[derive(Deserialize)]
pub struct StackPathParam {
    stack_id: usize,
}

pub async fn stack_middleware<B>(
    Extension(workspace): Extension<Workspace>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    mut req: Request<B>,
    next: Next<B>,
) -> Result<Response, LMECoreError> {
    // unlock the workspace immediately after insert stack to extensions
    {
        let stack = workspace.get_stack(stack_id).await?;
        req.extensions_mut().insert(stack.clone());
    }
    Ok(next.run(req).await)
}

pub async fn read_stack(Extension(stack): Extension<Arc<Stack>>) -> Json<Molecule> {
    Json(stack.read().clone())
}

pub async fn read_cleaned(Extension(stack): Extension<Arc<Stack>>) -> Json<CompactedMolecule> {
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
    Extension(workspace): Extension<Workspace>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(patch): Json<Molecule>,
) -> Result<(), LMECoreError> {
    workspace.write_to_layer(stack_id, &patch).await
}

pub async fn overlay_to(
    Extension(workspace): Extension<Workspace>,
    Json((config, stacks)): Json<(Layer, Vec<usize>)>,
) -> Result<(), LMECoreError> {
    workspace.overlay_to(&stacks, config).await
}

pub async fn remove_stack(
    Extension(workspace): Extension<Workspace>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
) -> Result<()> {
    Ok(workspace.remove_stack(stack_id).await)
}

#[derive(Deserialize)]
pub struct CloneStackOptions {
    amount: usize,
}

pub async fn clone_stack(
    Extension(workspace): Extension<Workspace>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(CloneStackOptions { amount }): Json<CloneStackOptions>,
) -> Result<Json<(usize, usize)>, LMECoreError> {
    Ok(Json(workspace.clone_stack(stack_id, amount).await?))
}

pub async fn clone_base(
    Extension(workspace): Extension<Workspace>,
    Path(StackPathParam { stack_id }): Path<StackPathParam>,
    Json(CloneStackOptions { amount }): Json<CloneStackOptions>,
) -> Result<Json<(usize, usize)>, LMECoreError> {
    Ok(Json(workspace.clone_base(stack_id, amount).await?))
}

pub async fn transform_atoms(
    Extension(workspace): Extension<Workspace>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(StackNamePathParam { stack_id, name }): Path<StackNamePathParam>,
    Json(transform): Json<Transform3<f64>>,
) -> Result<(), LMECoreError> {
    let Json(indexes) = class_indexes(
        Extension(workspace.clone()),
        Extension(stack.clone()),
        Path(NamePathParam { name }),
    )
    .await;
    let atoms = stack
        .read()
        .0
        .clone()
        .into_par_iter()
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
        .map(|(idx, atom)| (idx, Some(atom.transform_position(&transform))))
        .collect::<HashMap<_, _>>();
    write_to_layer(
        Extension(workspace),
        Path(StackPathParam { stack_id }),
        Json((atoms, BondGraph::new())),
    )
    .await
}

// Complex level APIs
pub async fn rotation_atoms(
    workspace: Extension<Workspace>,
    stack: Extension<Arc<Stack>>,
    params: Path<StackNamePathParam>,
    Json((center, axis, angle)): Json<(Point3<f64>, Vector3<f64>, f64)>,
) -> Result<(), LMECoreError> {
    let transform = Transform3::from_matrix_unchecked(
        Matrix4::new_rotation_wrt_point(axis * angle, center)
    );
    transform_atoms(workspace, stack, params, Json(transform)).await
}

pub async fn translation_atoms(
    workspace: Extension<Workspace>,
    stack: Extension<Arc<Stack>>,
    params: Path<StackNamePathParam>,
    Json(vector): Json<Vector3<f64>>,
) -> Result<(), LMECoreError> {
    let transform = Transform3::from_matrix_unchecked(
        Matrix4::new_translation(&vector)
    );
    transform_atoms(workspace, stack, params, Json(transform)).await
}

pub async fn import_structure(
    Extension(workspace): Extension<Workspace>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(StackNamePathParam { stack_id, name }): Path<StackNamePathParam>,
    Json(CompactedMolecule {
        atoms,
        bonds_idxs,
        bonds_values,
    }): Json<CompactedMolecule>,
) -> Result<Json<Vec<usize>>> {
    let bonds = bonds_idxs
        .into_iter()
        .zip(bonds_values.into_iter())
        .collect::<HashMap<_, _>>();
    let offset = stack.read().0.keys().max().unwrap_or(&0) + 1;
    let atoms_patch = atoms
        .into_par_iter()
        .enumerate()
        .map(|(idx, atom)| (idx + offset, Some(atom)))
        .collect::<HashMap<_, _>>();
    let mut bonds_patch = BondGraph::from(bonds);
    bonds_patch.offset(offset);
    let mut atom_idxs = atoms_patch.keys().cloned().collect::<Vec<_>>();
    atom_idxs.sort();
    write_to_layer(
        Extension(workspace.clone()),
        Path(StackPathParam { stack_id }),
        Json((atoms_patch, bonds_patch)),
    )
    .await?;
    set_to_class(Extension(workspace), Json((atom_idxs.clone(), name))).await?;
    Ok(Json(atom_idxs))
}

#[derive(Deserialize, Debug)]
pub struct AddSubstitute {
    structure: CompactedMolecule,
    current: (usize, usize),
    target: (usize, usize),
    class_name: Option<String>,
}

pub async fn add_substitute(
    Extension(workspace): Extension<Workspace>,
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
        .structure
        .atoms
        .get(configuration.current.0)
        .zip(configuration.structure.atoms.get(configuration.current.1));

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
            .structure
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
            Json(CompactedMolecule {
                atoms,
                bonds_idxs: configuration
                    .structure
                    .bonds_idxs
                    .into_iter()
                    .map(|pair| Pair::from(pair))
                    .collect(),
                bonds_values: configuration.structure.bonds_values,
            }),
        )
        .await?;
        let to_remove = indexes
            .get(configuration.current.0)
            .zip(indexes.get(configuration.current.1));
        // stack updated, use new stack.
        let stack = workspace.get_stack(stack_id).await?;
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
