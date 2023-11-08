use std::collections::HashMap;

use lazy_static::lazy_static;
use nalgebra::{Rotation3, Unit, Vector3};
use rayon::prelude::*;

use super::{AtomTable, BondTable};

use super::{Atom, Layer};

lazy_static! {
    pub static ref BLANK_BACKGROUND: Layer =
        Layer::new_filter_layer(Box::new(|_| (HashMap::new(), HashMap::new())));
    pub static ref REMOVE_BOND_LAYER: Layer =
        Layer::new_filter_layer(Box::new(|(atoms, _)| (atoms, HashMap::new())));
    pub static ref REMOVE_HS_LAYER: Layer =
        Layer::new_filter_layer(Box::new(|current: (AtomTable, BondTable)| {
            let (mut atom_table, mut bond_table) = current;
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
        }));
}

pub fn create_translate_layer(vector: Vector3<f64>) -> Layer {
    Layer::new_filter_layer(Box::new(move |current: (AtomTable, BondTable)| {
        let (mut atom_table, bond_table) = current;
        let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
            .par_iter()
            .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
            .unzip();
        let translated = atoms
            .into_par_iter()
            .map(|Atom { element, position }| {
                Some(Atom {
                    element,
                    position: position + vector,
                })
            })
            .collect::<Vec<_>>();
        atom_table = idxs.into_iter().zip(translated).collect();
        (atom_table, bond_table)
    }))
}

pub fn crate_rotation_layer(center: Vector3<f64>, vector: Vector3<f64>, angle: f64) -> Layer {
    let matrix = Rotation3::from_axis_angle(&Unit::new_normalize(vector), angle)
        .matrix()
        .clone();
    Layer::new_filter_layer(Box::new(move |current: (AtomTable, BondTable)| {
        let (mut atom_table, bond_table) = current;
        let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
            .par_iter()
            .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
            .unzip();
        let rotated = atoms
            .into_par_iter()
            .map(|Atom { element, position }| {
                let vector = Vector3::from(position - center).transpose();
                let rotated = vector * matrix;
                let position = rotated.transpose() + center;
                Some(Atom { element, position })
            })
            .collect::<Vec<_>>();
        atom_table = idxs.into_iter().zip(rotated).collect();
        (atom_table, bond_table)
    }))
}
