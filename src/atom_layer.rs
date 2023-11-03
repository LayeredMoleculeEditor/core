use std::collections::HashMap;

use many_to_many::ManyToMany;
use nalgebra::Point3;

use crate::utils::{InsertResult, LayerInserResult, LayerRemoveResult, Pair, UniqueValueMap};

pub enum Atom {
    Real(usize, Point3<f64>),
    Pesudo(usize, Point3<f64>),
}

pub trait ReadableAtomContainer {
    fn get_idxs(&self) -> Vec<usize>;
    fn get_atom(&self, idx: &usize) -> Option<&Atom>;
    fn get_ids(&self) -> &HashMap<usize, String>;
    fn get_classes(&self) -> &ManyToMany<usize, String>;
}

pub trait WritableAtomContainer<'a> {
    fn add_atom(&mut self, atom: Atom) -> usize;
    fn update_atom(&mut self, idx: &usize, atom: Atom) -> LayerInserResult<'a, Atom>;
    fn remove_atom(&mut self, idx: &usize) -> LayerRemoveResult<Atom>;
    fn set_id(&mut self, idx: &usize, id: String) -> InsertResult<usize, String>;
    fn unset_id(&mut self, idx: &usize) -> Option<String>;
    fn set_class(&mut self, idx: &usize, class: String) -> bool;
    fn unset_class(&mut self, idx: &usize, class: &String) -> bool;
}

pub trait ReadableBondContainer<BondType> {
    fn get_idxs(&self) -> Vec<Pair<usize>>;
    fn get_bond(&self, a: &usize, b: &usize) -> Option<&BondType>;
}

pub trait WritableBondContainer<BondType> {
    fn set_bond(&mut self, a: &usize, b: &usize, bond: BondType) -> Option<BondType>;
    fn remove_bond(&mut self, a: &usize, b: &usize) -> LayerRemoveResult<BondType>;
}

pub struct EmptyBase {
    ids: HashMap<usize, String>,
    classes: ManyToMany<usize, String>,
}

impl ReadableAtomContainer for EmptyBase {
    fn get_idxs(&self) -> Vec<usize> {
        vec![]
    }
    fn get_atom(&self, _idx: &usize) -> Option<&Atom> {
        None
    }
    fn get_ids(&self) -> &HashMap<usize, String> {
        &self.ids
    }
    fn get_classes(&self) -> &ManyToMany<usize, String> {
        &self.classes
    }
}

impl<BondType> ReadableBondContainer<BondType> for EmptyBase {
    fn get_idxs(&self) -> Vec<Pair<usize>> {
        vec![]
    }

    fn get_bond(&self, _a: &usize, _b: &usize) -> Option<&BondType> {
        None
    }
}

pub struct AtomLayer {
    atoms: HashMap<usize, Atom>,
    ids: UniqueValueMap<usize, String>,
    classes: ManyToMany<usize, String>,
}

impl AtomLayer {
    fn next_idx(&self) -> &usize {
        self.atoms.keys().max().unwrap_or(&0)
    }
}

impl ReadableAtomContainer for AtomLayer {
    fn get_idxs(&self) -> Vec<usize> {
        self.atoms.keys().copied().collect::<Vec<_>>()
    }

    fn get_atom(&self, idx: &usize) -> Option<&Atom> {
        self.atoms.get(idx)
    }

    fn get_ids(&self) -> &HashMap<usize, String> {
        self.ids.data()
    }

    fn get_classes(&self) -> &ManyToMany<usize, String> {
        &self.classes
    }
}

impl<'a> WritableAtomContainer<'a> for AtomLayer {
    fn add_atom(&mut self, atom: Atom) -> usize {
        let idx = *self.next_idx();
        self.atoms.insert(idx, atom);
        idx
    }

    fn remove_atom(&mut self, idx: &usize) -> LayerRemoveResult<Atom> {
        if let Some(origin) = self.atoms.remove(idx) {
            self.ids.remove(idx).unwrap();
            self.classes.remove_left(idx);
            LayerRemoveResult::Removed(origin)
        } else {
            LayerRemoveResult::None
        }
    }

    fn update_atom(&mut self, idx: &usize, atom: Atom) -> LayerInserResult<'a, Atom> {
        if let Some(origin) = self.atoms.insert(*idx, atom) {
            LayerInserResult::Updated(origin)
        } else {
            LayerInserResult::Created
        }
    }

    fn set_id(&mut self, idx: &usize, id: String) -> InsertResult<usize, String> {
        self.ids.insert(*idx, id)
    }

    fn unset_id(&mut self, idx: &usize) -> Option<String> {
        self.ids.remove(idx)
    }

    fn set_class(&mut self, idx: &usize, class: String) -> bool {
        self.classes.insert(*idx, class)
    }

    fn unset_class(&mut self, idx: &usize, class: &String) -> bool {
        self.classes.remove(idx, class)
    }
}

pub struct BondLayer<T>(HashMap<Pair<usize>, T>);

impl<T> BondLayer<T> {
    fn data(&self) -> &HashMap<Pair<usize>, T> {
        &self.0
    }

    fn data_mut(&mut self) -> &mut HashMap<Pair<usize>, T> {
        &mut self.0
    }
}

impl<T> ReadableBondContainer<T> for BondLayer<T> {
    fn get_idxs(&self) -> Vec<Pair<usize>> {
        self.data().keys().copied().collect()
    }

    fn get_bond(&self, a: &usize, b: &usize) -> Option<&T> {
        self.data().get(&Pair::new(*a, *b))
    }
}

impl<T> WritableBondContainer<T> for BondLayer<T> {
    fn set_bond(&mut self, a: &usize, b: &usize, bond: T) -> Option<T> {
        let pair = Pair::new(*a, *b);
        self.data_mut().insert(pair, bond)
    }

    fn remove_bond(&mut self, a: &usize, b: &usize) -> LayerRemoveResult<T> {
        let pair = Pair::new(*a, *b);
        if let Some(origin) = self.data_mut().remove(&pair) {
            LayerRemoveResult::Removed(origin)
        } else {
            LayerRemoveResult::None
        }
    }
}

pub trait ReadableFillLayer<T>: ReadableAtomContainer + ReadableBondContainer<T> {}

pub enum Layer<'a, T> {
    FillLayer {
        atom_layer: AtomLayer,
        bond_layer: BondLayer<T>,
    },
    FilterLayer(&'a dyn FnOnce(&dyn ReadableFillLayer<T>) -> dyn ReadableFillLayer<T>),
}
