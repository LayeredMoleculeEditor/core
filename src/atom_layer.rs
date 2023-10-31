use std::collections::HashMap;

use many_to_many::ManyToMany;
use nalgebra::{Point3, Rotation3, Unit, Vector3};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};

use crate::utils::{InsertResult, UniqueValueMap};

#[derive(Debug, Clone, Copy)]
pub struct AtomData {
    pub element: isize,
    pub position: Point3<f64>,
}

pub trait ReadableAtomLayer {
    fn atoms(&self) -> &HashMap<usize, AtomData>;
}

pub trait WritableAtomLayer {
    fn atoms_mut(&mut self) -> &mut HashMap<usize, AtomData>;
}

pub trait IdLayer {
    fn id_map(&self) -> &UniqueValueMap<usize, String>;
    fn id_map_mut(&self) -> &mut UniqueValueMap<usize, String>;
}

pub trait ClassLayer {
    fn class_map(&self) -> &ManyToMany<usize, String>;
    fn class_map_mut(&mut self) -> &mut ManyToMany<usize, String>;
}

pub trait AtomEntryBase: ReadableAtomLayer + WritableAtomLayer + IdLayer + ClassLayer {}

pub trait AtomEntry<T> {
    fn close(self) -> T;
}

static SELECTED_NOT_FOUND: &str = "selected atoms should always existed";

pub struct SelectOne<T: AtomEntryBase> {
    idx: usize,
    layer: T
}

impl<T: AtomEntryBase> AtomEntry<T> for SelectOne<T> {
    fn close(self) -> T {
        self.layer
    }
}

impl<T: AtomEntryBase> SelectOne<T> {
    pub fn get_data(&self) -> AtomData {
        *self.layer.atoms().get(&self.idx).expect(SELECTED_NOT_FOUND)
    }

    pub fn set_data(&mut self, data: AtomData) -> AtomData {
        self.layer
            .atoms_mut()
            .insert(self.idx, data)
            .expect(SELECTED_NOT_FOUND)
    }

    pub fn get_id(&self) -> Option<&String> {
        self.layer.id_map().data().get(&self.idx)
    }

    pub fn set_id(&mut self, id: String) -> InsertResult<usize, String> {
        self.layer.id_map_mut().insert(self.idx, id)
    }

    pub fn remove_id(&mut self) -> Option<String> {
        self.layer.id_map_mut().remove(&self.idx)
    }

    pub fn get_classes(&self) -> Vec<String> {
        self.layer.class_map().get_left(&self.idx).unwrap_or(vec![])
    }

    pub fn set_class(&mut self, class: String) -> bool {
        self.layer.class_map_mut().insert(self.idx, class)
    }

    pub fn remove_from_class(&mut self, class: &String) -> bool {
        self.layer.class_map_mut().remove(&self.idx, class)
    }

    pub fn remove_atom(mut self) -> T {
        self.layer
            .atoms_mut()
            .remove(&self.idx)
            .expect(SELECTED_NOT_FOUND);
        self.layer.id_map_mut().remove(&self.idx);
        self.layer.class_map_mut().remove_left(&self.idx);
        self.close()
    }
}

pub struct SelectGroup<T: AtomEntryBase + Sync> {
    class: String,
    layer: T,
}

impl<T: AtomEntryBase + Sync> AtomEntry<T> for SelectGroup<T> {
    fn close(self) -> T {
        self.layer
    }
}

impl<T: AtomEntryBase + Sync> SelectGroup<T> {
    pub fn get_idxs(&self) -> Vec<usize> {
        self.layer
            .class_map()
            .get_right(&self.class)
            .unwrap_or(vec![])
    }

    pub fn get_data(&self) -> HashMap<usize, AtomData> {
        self.get_idxs()
            .par_iter()
            .map(|idx| (*idx, *self.layer.atoms().get(idx).expect(SELECTED_NOT_FOUND)))
            .collect::<HashMap<_, _>>()
    }

    pub fn get_ids(&self) -> HashMap<usize, Option<String>> {
        self.get_idxs()
            .par_iter()
            .map(|idx| (*idx, self.layer.id_map().data().get(idx).cloned()))
            .collect::<HashMap<_, _>>()
    }

    pub fn set_element(&mut self, element: isize) {
        for idx in self.get_idxs() {
            self.layer
                .atoms_mut()
                .entry(idx)
                .and_modify(|data| data.element = element);
        }
    }

    /// Move atom in single thread mode.
    ///
    /// Similar to `move_atom` but calculate in single thread. You can use a mutable function which does't impl Clone + Sync
    pub fn move_atoms_single_thread<F>(&mut self, mut f: F)
    where
        F: FnMut(Point3<f64>) -> Point3<f64>,
    {
        let updated_positions = self
            .get_data()
            .into_iter()
            .map(|(idx, data)| (idx, f(data.position)))
            .collect::<Vec<_>>();
        for (idx, position) in updated_positions {
            self.layer
                .atoms_mut()
                .entry(idx)
                .and_modify(|data| data.position = position);
        }
    }

    /// Move atom with given function
    ///
    /// The given function receives origin position as parameter and returns new position
    ///
    /// `move_atoms` caculate new position in multi-thread mode, so the function/closure should impl Copy + Sync and immutable
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
                .atoms_mut()
                .entry(idx)
                .and_modify(|data| data.position = position);
        }
    }

    pub fn translate_atoms(&mut self, delta: Vector3<f64>) {
        self.move_atoms(|origin| origin + delta)
    }

    /// Rotate atoms
    ///
    /// Rotate atoms around given axis with specified angle.
    ///
    /// If target axis doesn't pass the origin, you should translate atoms before rotate and then translate them back.
    pub fn rotate_atoms(&mut self, axis: Vector3<f64>, angle: f64) {
        let unit_axis = Unit::new_normalize(axis);
        let rotation = Rotation3::from_axis_angle(&unit_axis, angle);
        self.move_atoms(|origin| {
            let vector = Vector3::<f64>::from(origin - Point3::new(0., 0., 0.)).transpose();
            let rotated = vector * rotation;
            Point3::from(rotated.transpose())
        })
    }

    pub fn remove_atoms(mut self) -> T {
        let idxs = self.get_idxs();
        self.layer.atoms_mut().retain(|k, _| idxs.contains(k));
        for idx in idxs {
            self.layer.id_map_mut().remove(&idx);
            self.layer.class_map_mut().remove_left(&idx);
        }
        self.close()
    }

    pub fn remove_class(mut self) -> T {
        self.layer.class_map_mut().remove_right(&self.class);
        self.close()
    }
}
