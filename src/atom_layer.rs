use std::collections::HashMap;

use many_to_many::ManyToMany;
use nalgebra::{Point3, Vector3};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};

use crate::utils::{UniqueValueMap, InsertResult};

#[derive(Debug, Clone, Copy)]
pub struct AtomData {
    pub element: isize,
    pub position: Point3<f64>,
}

pub struct AtomLayer {
    atoms: HashMap<usize, AtomData>,
    id_map: UniqueValueMap<usize, String>,
    class_map: ManyToMany<usize, String>,
}

pub trait SelectEntry {
    fn close(self) -> AtomLayer;
}

static SELECTED_NOT_FOUND: &str = "selected atoms should always existed";

pub struct SelectOne {
    idx: usize,
    layer: AtomLayer,
}

impl SelectEntry for SelectOne {
    fn close(self) -> AtomLayer {
        self.layer
    }
}

impl SelectOne {
    pub fn get_data(&self) -> AtomData {
        *self.layer.atoms.get(&self.idx).expect(SELECTED_NOT_FOUND)
    }

    pub fn set_data(&mut self, data: AtomData) -> AtomData {
        self.layer
            .atoms
            .insert(self.idx, data)
            .expect(SELECTED_NOT_FOUND)
    }

    pub fn get_id(&self) -> Option<&String> {
        self.layer.id_map.data().get(&self.idx)
    }

    pub fn set_id(&mut self, id: String) -> InsertResult<usize, String> {
        self.layer.id_map.insert(self.idx, id)
    }

    pub fn remove_id(&mut self) -> Option<String> {
        self.layer.id_map.remove(&self.idx)
    }

    pub fn get_classes(&self) -> Vec<String> {
        self.layer.class_map.get_left(&self.idx).unwrap_or(vec![])
    }

    pub fn set_class(&mut self, class: String) -> bool {
        self.layer.class_map.insert(self.idx, class)
    }

    pub fn remove_from_class(&mut self, class: &String) -> bool {
        self.layer.class_map.remove(&self.idx, class)
    }

    pub fn remove_atom(mut self) -> AtomLayer {
        self.layer
            .atoms
            .remove(&self.idx)
            .expect(SELECTED_NOT_FOUND);
        self.layer.id_map.remove(&self.idx);
        self.layer.class_map.remove_left(&self.idx);
        self.close()
    }
}

pub struct SelectGroup {
    class: String,
    layer: AtomLayer,
}

impl SelectEntry for SelectGroup {
    fn close(self) -> AtomLayer {
        self.layer
    }
}

impl SelectGroup {
    pub fn get_idxs(&self) -> Vec<usize> {
        self.layer
            .class_map
            .get_right(&self.class)
            .unwrap_or(vec![])
    }
    pub fn get_data(&self) -> HashMap<usize, AtomData> {
        self.get_idxs()
            .par_iter()
            .map(|idx| (*idx, *self.layer.atoms.get(idx).expect(SELECTED_NOT_FOUND)))
            .collect::<HashMap<_, _>>()
    }

    pub fn get_ids(&self) -> HashMap<usize, Option<String>> {
        self.get_idxs()
            .par_iter()
            .map(|idx| (*idx, self.layer.id_map.data().get(idx).cloned()))
            .collect::<HashMap<_, _>>()
    }

    pub fn set_element(&mut self, element: isize) {
        for idx in self.get_idxs() {
            self.layer
                .atoms
                .entry(idx)
                .and_modify(|data| data.element = element);
        }
    }

    pub fn move_atoms<F>(&mut self, f: F)
    where
        F: Copy + Sync + FnOnce(Point3<f64>) -> Point3<f64>,
    {
        let updated_positions = self
            .get_data()
            .into_par_iter()
            .map(|(idx, data)| (idx, f(data.position)))
            .collect::<Vec<_>>();
        for (idx, position) in updated_positions {
            self.layer
                .atoms
                .entry(idx)
                .and_modify(|data| data.position = position);
        }
    }

    pub fn translate_atoms(&mut self, delta: Vector3<f64>) {
        self.move_atoms(|origin| origin + delta)
    }
}
