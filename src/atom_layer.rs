use std::{collections::HashMap, hash::Hash, usize};

use nalgebra::Point3;

use crate::utils::Pair;

pub struct Atom {
    element: usize,
    position: Point3<f64>,
}

pub trait ReadableFillLayer<K: Copy + Eq + Hash, V> {
    fn get_idxs(&self) -> Vec<K>;
    fn get_value(&self, idx: &K) -> Option<&V>;
}

pub trait ReadableBondFillLayer<BondType>: ReadableFillLayer<Pair<usize>, BondType> {
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

pub trait WritableFillLayer<K: Copy + Eq + Hash, V>: ReadableFillLayer<K, V> {
    fn set_value(&mut self, idx: K, value: V) -> Option<V>;
    fn shadow_value(&mut self, idx: K);
}

pub trait WritableBondFillLayer<BondType>:
    WritableFillLayer<Pair<usize>, BondType> + ReadableBondFillLayer<BondType>
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

pub trait ABFillLayer<BondType>:
    ReadableFillLayer<usize, Atom> + ReadableBondFillLayer<BondType>
{
}

pub struct RwFillLayer<BondType> {
    atoms: HashMap<usize, Option<Atom>>,
    bonds: HashMap<Pair<usize>, Option<BondType>>,
}

impl<BondType> ReadableFillLayer<usize, Atom> for RwFillLayer<BondType> {
    fn get_idxs(&self) -> Vec<usize> {
        self.atoms.keys().copied().collect::<Vec<_>>()
    }

    fn get_value(&self, idx: &usize) -> Option<&Atom> {
        self.atoms.get(idx).expect("Index out of range").as_ref()
    }
}

impl<BondType> ReadableFillLayer<Pair<usize>, BondType> for RwFillLayer<BondType> {
    fn get_idxs(&self) -> Vec<Pair<usize>> {
        self.bonds.keys().copied().collect::<Vec<_>>()
    }

    fn get_value(&self, idx: &Pair<usize>) -> Option<&BondType> {
        self.bonds.get(idx).expect("Index out of range").as_ref()
    }
}

impl<BondType> ReadableBondFillLayer<BondType> for RwFillLayer<BondType> {}

impl<BondType> WritableFillLayer<usize, Atom> for RwFillLayer<BondType> {
    fn set_value(&mut self, idx: usize, value: Atom) -> Option<Atom> {
        self.atoms.insert(idx, Some(value)).unwrap_or(None)
    }
    fn shadow_value(&mut self, idx: usize) {
        self.atoms.insert(idx, None);
    }
}

impl<BondType> WritableFillLayer<Pair<usize>, BondType> for RwFillLayer<BondType> {
    fn set_value(&mut self, idx: Pair<usize>, value: BondType) -> Option<BondType> {
        self.bonds.insert(idx, Some(value)).unwrap_or(None)
    }

    fn shadow_value(&mut self, idx: Pair<usize>) {
        self.bonds.insert(idx, None);
    }
}

impl<BondType> WritableBondFillLayer<BondType> for RwFillLayer<BondType> {}
