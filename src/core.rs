use std::collections::{HashMap, HashSet};

use many_to_many::ManyToMany;
use nalgebra::Point3;
use rayon::prelude::*;

use crate::utils::{InsertResult, UniqueValueMap};

pub trait ReadableAtomLayer: Sync {
    fn get_idxs(&self) -> HashSet<usize>;
    fn get_atom(&self, idx: usize) -> Option<(isize, Point3<f64>)>;
    fn get_ids(&self) -> &HashMap<String, usize>;
    fn get_classes(&self) -> &ManyToMany<String, usize>;
    fn get_atom_with_id(&self, target_id: &str) -> Option<(isize, Point3<f64>)> {
        self.get_ids()
            .par_iter()
            .find_map_first(|(id, idx)| if id == target_id { Some(idx) } else { None })
            .and_then(|idx| self.get_atom(*idx))
    }
    fn get_atoms_with_classes(&self, class_name: &String) -> Option<Vec<(isize, Point3<f64>)>> {
        self.get_classes().get_left(class_name).and_then(|idxs| {
            Some(
                idxs.par_iter()
                    .map(|idx| self.get_atom(*idx).unwrap())
                    .collect::<Vec<_>>(),
            )
        })
    }
    fn id_of(&self, target_idx: usize) -> Option<&String> {
        self.get_ids()
            .par_iter()
            .find_map_first(|(id, idx)| if *idx == target_idx { Some(id) } else { None })
    }
    fn classes_of(&self, target_idx: usize) -> Option<Vec<String>> {
        self.get_classes().get_right(&target_idx)
    }
}

pub trait WritableAtomLayer: ReadableAtomLayer {
    fn id_map_mut(&mut self) -> &mut UniqueValueMap<String, usize>;
    fn set_element(&mut self, idx: usize, element: isize) -> Option<isize>;
    fn set_position(&mut self, idx: usize, position: Point3<f64>) -> Option<Point3<f64>>;
    fn set_id(&mut self, idx: usize, id: String) -> InsertResult<String, usize> {
        self.id_map_mut().insert(id, idx)
    }
    fn remove_id(&mut self, id: &str) -> Option<usize> {
        self.id_map_mut().remove(id)
    }
    fn set_class(&mut self, idx: usize, class: String);
    fn remove_class(&mut self, idx: usize, class: &str);
}

pub struct AtomFillLayer {
    next: usize,
    basic: HashMap<usize, (isize, Point3<f64>)>,
    id_map: UniqueValueMap<String, usize>,
    class_map: ManyToMany<String, usize>,
}

impl ReadableAtomLayer for AtomFillLayer {
    fn get_idxs(&self) -> HashSet<usize> {
        self.basic.keys().copied().collect::<HashSet<_>>()
    }

    fn get_atom(&self, idx: usize) -> Option<(isize, Point3<f64>)> {
        self.basic.get(&idx).copied()
    }

    fn get_ids(&self) -> &HashMap<String, usize> {
        self.id_map.data()
    }

    fn get_classes(&self) -> &ManyToMany<String, usize> {
        &self.class_map
    }

    fn id_of(&self, target_idx: usize) -> Option<&String> {
        self.get_ids()
            .par_iter()
            .find_map_first(|(k, v)| if v == &target_idx { Some(k) } else { None })
    }

    fn classes_of(&self, target_idx: usize) -> Option<Vec<String>> {
        self.get_classes().get_right(&target_idx)
    }

    fn get_atom_with_id(&self, target_id: &str) -> Option<(isize, Point3<f64>)> {
        self.get_ids().get(target_id).and_then(|idx| self.get_atom(*idx))
    }
}
