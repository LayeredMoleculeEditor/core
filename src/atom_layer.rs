use rayon::prelude::*;
use std::{collections::HashMap, hash::Hash, sync::Arc};

use nalgebra::Point3;

use crate::utils::Pair;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atom {
    element: usize,
    position: Point3<f64>,
}

pub trait FillLayer<K: Copy + Eq + Hash, V: Copy> {
    fn get_idxs(&self) -> Vec<K>;
    fn get_value(&self, idx: &K) -> Option<&V>;
}

pub trait BondFillLayer<BondType: Copy>: FillLayer<Pair<usize>, BondType> {
    fn get_neighbors(&self, idx: &usize) -> Vec<(Pair<usize>, &BondType)> {
        self.get_idxs()
            .into_iter()
            .filter_map(|pair| {
                pair.get_another(idx)
                    .and_then(|_| Some(pair))
                    .zip(self.get_value(&pair))
            })
            .collect::<Vec<_>>()
    }
}

pub trait WritableFillLayer<K: Copy + Eq + Hash, V: Copy>: FillLayer<K, V> {
    fn set_value(&mut self, idx: K, value: V) -> Option<V>;
    fn shadow_value(&mut self, idx: K);
}

pub trait WritableBondFillLayer<BondType: Copy>:
    WritableFillLayer<Pair<usize>, BondType> + BondFillLayer<BondType>
{
    fn remove_node(&mut self, idx: &usize) {
        let neighbors = self
            .get_neighbors(idx)
            .into_iter()
            .map(|(neighbor, _)| neighbor)
            .collect::<Vec<_>>();
        for neighbor in neighbors {
            self.shadow_value(neighbor);
        }
    }
}

pub trait ABFillLayer<BondType: Copy>: FillLayer<usize, Atom> + BondFillLayer<BondType> {}

pub struct RwFillLayer<BondType> {
    atoms: HashMap<usize, Option<Atom>>,
    bonds: HashMap<Pair<usize>, Option<BondType>>,
}

impl<BondType> FillLayer<usize, Atom> for RwFillLayer<BondType> {
    fn get_idxs(&self) -> Vec<usize> {
        self.atoms.keys().copied().collect::<Vec<_>>()
    }

    fn get_value(&self, idx: &usize) -> Option<&Atom> {
        self.atoms.get(idx).expect("Index out of range").as_ref()
    }
}

impl<BondType: Copy> FillLayer<Pair<usize>, BondType> for RwFillLayer<BondType> {
    fn get_idxs(&self) -> Vec<Pair<usize>> {
        self.bonds.keys().copied().collect::<Vec<_>>()
    }

    fn get_value(&self, idx: &Pair<usize>) -> Option<&BondType> {
        self.bonds.get(idx).expect("Index out of range").as_ref()
    }
}

impl<BondType: Copy> BondFillLayer<BondType> for RwFillLayer<BondType> {}

impl<BondType: Copy> WritableFillLayer<usize, Atom> for RwFillLayer<BondType> {
    fn set_value(&mut self, idx: usize, value: Atom) -> Option<Atom> {
        self.atoms.insert(idx, Some(value)).unwrap_or(None)
    }
    fn shadow_value(&mut self, idx: usize) {
        self.atoms.insert(idx, None);
    }
}

impl<BondType: Copy> WritableFillLayer<Pair<usize>, BondType> for RwFillLayer<BondType> {
    fn set_value(&mut self, idx: Pair<usize>, value: BondType) -> Option<BondType> {
        self.bonds.insert(idx, Some(value)).unwrap_or(None)
    }

    fn shadow_value(&mut self, idx: Pair<usize>) {
        self.bonds.insert(idx, None);
    }
}

impl<BondType: Copy> WritableBondFillLayer<BondType> for RwFillLayer<BondType> {}

impl<BondType: Copy> ABFillLayer<BondType> for RwFillLayer<BondType> {}

pub trait Exportable<BondType, OutputBondType> {
    fn export(
        &self,
        f: fn(BondType) -> Option<OutputBondType>,
    ) -> (
        Vec<Atom>,
        HashMap<(usize, usize), OutputBondType>,
        Vec<usize>,
    );
}

impl<BondType: Copy + Sync + Send, OutputBondType: Copy + Sync + Send>
    Exportable<BondType, OutputBondType> for RwFillLayer<BondType>
{
    fn export(
        &self,
        f: fn(BondType) -> Option<OutputBondType>,
    ) -> (
        Vec<Atom>,
        HashMap<(usize, usize), OutputBondType>,
        Vec<usize>,
    ) {
        let mut cleaned = self
            .atoms
            .par_iter()
            .filter_map(|(idx, atom)| atom.and_then(|atom| Some((*idx, atom))))
            .collect::<Vec<_>>();
        cleaned.sort_by(|(a, _), (b, _)| a.cmp(b));
        let (idxs, atoms): (Vec<usize>, Vec<Atom>) = cleaned.into_par_iter().unzip();
        let bonds = self
            .bonds
            .par_iter()
            .filter_map(|(pair, bond)| {
                let (a, b) = pair.to_tuple();
                idxs.iter()
                    .position(|idx| idx == a)
                    .zip(idxs.iter().position(|idx| idx == b))
                    .zip(bond.and_then(f))
            })
            .collect::<HashMap<_, _>>();
        (atoms, bonds, idxs)
    }
}

pub enum MultiLayerContainer<BondType> {
    Fill(Arc<dyn ABFillLayer<BondType>>),
    Filter(Arc<fn(RwFillLayer<BondType>) -> RwFillLayer<BondType>>),
}

impl<BondType: Copy> MultiLayerContainer<BondType> {
    pub fn compose(layers: &Vec<Self>) -> RwFillLayer<BondType> {
        let mut output = RwFillLayer {
            atoms: HashMap::new(),
            bonds: HashMap::new(),
        };
        for layer in layers {
            match layer {
                Self::Fill(fill_layer) => {
                    let atoms = fill_layer
                        .get_idxs()
                        .into_iter()
                        .map(|idx| (idx, fill_layer.get_value(&idx).copied()));
                    output.atoms.extend(atoms);
                    let bonds = fill_layer
                        .get_idxs()
                        .into_iter()
                        .map(|pair| (pair, fill_layer.get_value(&pair).copied()));
                    output.bonds.extend(bonds);
                }
                Self::Filter(transformer_fn) => {
                    output = transformer_fn(output);
                }
            }
        }
        output
    }
}
