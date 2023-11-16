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
use nanoid::nanoid;

use crate::{
    data_manager::{Atom, CleanedMolecule, Layer, Molecule, Stack, WorkspaceStore},
    error::LMECoreError,
    utils::{vector_align_rotation, BondGraph, Pair},
};

use super::{
    namespace::{class_indexes, set_to_class},
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
    let atoms = stack
        .read()
        .clone()
        .0
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

pub async fn translation_atoms(
    Extension(workspace): Extension<WorkspaceStore>,
    Extension(stack): Extension<Arc<Stack>>,
    Path(StackNamePathParam { stack_id, name }): Path<StackNamePathParam>,
    Json(vector): Json<[f64; 3]>,
) -> StatusCode {
    let (_, Json(indexes)) = class_indexes(
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
        .into_iter()
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
) -> (StatusCode, Json<Option<Vec<usize>>>) {
    let offset = stack.read().0.keys().max().unwrap_or(&0) + 1;
    let atoms_patch = atoms
        .into_iter()
        .enumerate()
        .map(|(idx, atom)| (idx + offset, Some(atom)))
        .collect::<HashMap<_, _>>();
    let mut bonds_patch = BondGraph::from(bonds);
    bonds_patch.offset(offset);
    let atom_idxs = atoms_patch.keys().cloned().collect::<Vec<_>>();
    let write_result = write_to_layer(
        Extension(workspace.clone()),
        Path(StackPathParam { stack_id }),
        Json((atoms_patch, bonds_patch)),
    )
    .await;
    if write_result.is_success() {
        (
            set_to_class(Extension(workspace), Json((atom_idxs.clone(), name))).await,
            Json(Some(atom_idxs)),
        )
    } else {
        (write_result, Json(None))
    }
}

// #[derive(Deserialize)]
// pub struct AddSubstitute {
//     atoms: Vec<Atom>,
//     bond: HashMap<Pair<usize>, f64>,
//     current: (usize, usize),
//     target: (usize, usize),
//     class_name: Option<String>,
// }

// pub async fn add_substitute(
//     Extension(workspace): Extension<WorkspaceStore>,
//     Extension(stack): Extension<Arc<Stack>>,
//     Path(StackPathParam { stack_id }): Path<StackPathParam>,
//     Json(configuration): Json<AddSubstitute>,
// ) -> (StatusCode, Json<Option<String>>) {
//     let atoms = &stack.read().0;
//     let target_atoms = atoms
//         .get(&configuration.target.0)
//         .map(|item| item.as_ref())
//         .flatten()
//         .zip(
//             atoms
//                 .get(&configuration.target.1)
//                 .map(|item| item.as_ref())
//                 .flatten(),
//         );

//     let current_atoms = configuration
//         .atoms
//         .get(configuration.current.0)
//         .zip(configuration.atoms.get(configuration.current.1));

//     if let Some(((base_entry, base_center), (sub_entry, sub_center))) =
//         target_atoms.zip(current_atoms)
//     {
//         let base_direction = base_center.get_position() - base_entry.get_position();
//         let sub_direction = sub_center.get_position() - sub_entry.get_position();
//         let (axis, angle) = vector_align_rotation(&sub_direction, &base_direction);
//         let matrix = *Rotation3::from_axis_angle(&Unit::new_normalize(axis), angle).matrix();
//         let center = *sub_center.get_position();
//         let translation_vector = base_center.get_position() - sub_center.get_position();
//         let atoms = configuration
//             .atoms
//             .into_iter()
//             .map(|atom| {
//                 atom.update_position(|origin| {
//                     ((origin - center).transpose() * matrix).transpose() + center
//                 })
//             })
//             .map(|atom| atom.update_position(|origin| origin + translation_vector))
//             .collect::<Vec<_>>();
//         let temp_class = configuration.class_name.unwrap_or(nanoid!());
//         let (status, Json(indexes)) = import_structure(
//             Extension(workspace.clone()),
//             Extension(stack.clone()),
//             Path(StackNamePathParam {
//                 stack_id,
//                 name: temp_class.clone(),
//             }),
//             Json((atoms, configuration.bond)),
//         )
//         .await;
//         if let Some(indexes) = indexes {

//             write_to_layer(Extension(workspace.clone()), Path(StackPathParam { stack_id }), json).await;
//         } else {
//             (StatusCode::FORBIDDEN, Json(None))
//         }
//     } else {
//         (StatusCode::NOT_FOUND, Json(None))
//     }
//     // (StatusCode::NOT_FOUND, Json(None))
// }
