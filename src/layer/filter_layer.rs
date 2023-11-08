use std::collections::HashMap;

use lazy_static::lazy_static;
use nalgebra::{Matrix3, Rotation3, Unit, Vector3};
use rayon::prelude::*;

use super::{AtomTable, BondTable, FilterCore};

use super::Layer;

#[derive(Clone, Copy, Debug)]
pub struct HideHydrogens;
#[derive(Clone, Copy, Debug)]
pub struct HideBonds;

impl FilterCore for HideBonds {
    fn transformer(&self, (atoms, _): (AtomTable, BondTable)) -> (AtomTable, BondTable) {
        (atoms, HashMap::new())
    }
}

impl FilterCore for HideHydrogens {
    fn transformer(&self, data: (AtomTable, BondTable)) -> (AtomTable, BondTable) {
        let (mut atom_table, mut bond_table) = data;
        atom_table.retain(|_, v| {
            v.and_then(|atom| if atom.element == 1 { Some(()) } else { None })
                .is_some()
        });
        let existed = atom_table.keys().collect::<Vec<_>>();
        bond_table.retain(|pair, bond| {
            let (a, b) = pair.to_tuple();
            existed.contains(&a) && existed.contains(&b) && bond.is_some()
        });
        (atom_table, bond_table)
    }
}

pub static HIDE_HS: HideHydrogens = HideHydrogens;
pub static HIDE_BONDS: HideBonds = HideBonds;

// pub struct TranslateLayer(Vector3<f64>);

// impl TranslateLayer {
//     pub fn new(vector: Vector3<f64>) -> Self {
//         Self(vector)
//     }
// }

// impl FilterCore for TranslateLayer {
//     fn transformer(&self, data: (AtomTable, BondTable)) -> (AtomTable, BondTable) {
//         let vector = self.0;
//         let (mut atom_table, bond_table) = data;
//         let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
//             .par_iter()
//             .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
//             .unzip();
//         let translated = atoms
//             .into_par_iter()
//             .map(|Atom { element, position }| {
//                 Some(Atom {
//                     element,
//                     position: position + vector,
//                 })
//             })
//             .collect::<Vec<_>>();
//         atom_table = idxs.into_iter().zip(translated).collect();
//         (atom_table, bond_table)
//     }
// }

// pub struct RotationLayer {
//     matrix: Matrix3<f64>,
//     center: Vector3<f64>,
// }

// impl RotationLayer {
//     pub fn new(center:Vector3<f64>, axis: Vector3<f64>, angle: f64) -> Self {
//         Self { matrix: *Rotation3::from_axis_angle(&Unit::new_normalize(axis), angle).matrix(), center }
//     }
// }

// impl FilterCore for RotationLayer {
//     fn transformer(&self, data: (AtomTable, BondTable)) -> (AtomTable, BondTable) {
//         let Self { matrix, center } = self;
//         let (mut atom_table, bond_table) = data;
//         let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
//             .par_iter()
//             .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
//             .unzip();
//         let rotated = atoms
//             .into_par_iter()
//             .map(|Atom { element, position }| {
//                 let vector = Vector3::from(position - center).transpose();
//                 let rotated = vector * matrix;
//                 let position = rotated.transpose() + center;
//                 Some(Atom { element, position })
//             })
//             .collect::<Vec<_>>();
//         atom_table = idxs.into_iter().zip(rotated).collect();
//         (atom_table, bond_table)
//     }
// }
