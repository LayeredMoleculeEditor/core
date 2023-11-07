use std::sync::Arc;

use nalgebra::{Vector3, Matrix3, Rotation3, Unit};
use rayon::prelude::*;
use uuid::Uuid;

use super::{Atom, AtomTable, BondTable, Layer, LAYER_MERGER};

pub struct RotationLayer {
    center: Vector3<f64>,
    matrix: Matrix3<f64>,
    layer_id: Uuid,
}

impl RotationLayer {
    pub fn new(center: Vector3<f64>, vector: Vector3<f64>, angle: f64) -> Self {
        Self {
            center,
            matrix: Rotation3::from_axis_angle(&Unit::new_normalize(vector), angle)
                .matrix()
                .clone(),
            layer_id: Uuid::new_v4(),
        }
    }
}

impl Layer for RotationLayer {
    fn read(&self, base: &[Arc<dyn Layer>]) -> (AtomTable, BondTable) {
        let (mut atom_table, bond_table) = LAYER_MERGER.merge_base(base);
        let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
            .par_iter()
            .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
            .unzip();
        let rotated = atoms
            .into_par_iter()
            .map(|Atom { element, position }| {
                let vector = Vector3::from(position - self.center).transpose();
                let rotated = vector * self.matrix;
                let position = rotated.transpose() + self.center;
                Some(Atom { element, position })
            })
            .collect::<Vec<_>>();
        atom_table.extend(idxs.into_iter().zip(rotated));
        (atom_table, bond_table)
    }

    fn id(&self) -> &Uuid {
        &self.layer_id
    }
}

pub struct TranslateLayer {
    vector: Vector3<f64>,
    layer_id: Uuid,
}

impl TranslateLayer {
    pub fn new(vector: Vector3<f64>) -> Self {
        Self {
            vector,
            layer_id: Uuid::new_v4(),
        }
    }
}

impl Layer for TranslateLayer {
    fn read(&self, base: &[Arc<dyn Layer>]) -> (AtomTable, BondTable) {
        let (mut atom_table, bond_table) = LAYER_MERGER.merge_base(base);
        let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
            .par_iter()
            .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
            .unzip();
        let translated = atoms
            .into_par_iter()
            .map(|Atom { element, position }| {
                Some(Atom {
                    element,
                    position: position + self.vector,
                })
            })
            .collect::<Vec<_>>();
        atom_table.extend(idxs.into_iter().zip(translated));
        (atom_table, bond_table)
    }

    fn id(&self) -> &Uuid {
        &self.layer_id
    }
}