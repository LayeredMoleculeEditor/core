pub fn add(left: usize, right: usize) -> usize {
    left + right
}

pub mod layer {
    use rayon::prelude::*;
    use std::{collections::{HashMap, HashSet}, hash::Hash};

    use nalgebra::{Point3, Vector3};

    pub struct AtomBuilder(isize, Point3<f64>, Option<String>, HashSet<String>);

    impl AtomBuilder {
        pub fn new() -> Self {
            AtomBuilder(0, Point3::new(0., 0., 0.), None, HashSet::new())
        }

        pub fn set_element(mut self, element: isize) -> Self {
            self.0 = element;
            self
        }

        pub fn set_position(mut self, position: Point3<f64>) -> Self {
            self.1 = position;
            self
        }

        pub fn move_position(mut self, vector: Vector3<f64>) -> Self {
            self.1 = self.1 + vector;
            self
        }

        pub fn set_id(mut self, id: String) -> Self {
            self.2 = Some(id);
            self
        }

        pub fn remove_id(mut self) -> Self {
            self.2 = None;
            self
        }

        pub fn add_class(mut self, class_name: String) -> Self {
            self.3.insert(class_name);
            self
        }

        pub fn remove_class(mut self, class_name: &str) -> Self {
            self.3.remove(class_name);
            self
        }

        pub fn build(&self) -> Atom {
            Atom(self.0, self.1, self.2.clone(), self.3.clone())
        }
    }

    pub struct Atom(isize, Point3<f64>, Option<String>, HashSet<String>);

    impl Atom {
        pub fn modify(&self) -> AtomBuilder {
            AtomBuilder(self.0, self.1, self.2.clone(), self.3.clone())
        }
    }

    impl Default for AtomBuilder {
        fn default() -> Self {
            AtomBuilder::new()
        }
    }

    impl Default for Atom {
        fn default() -> Self {
            AtomBuilder::default().build()
        }
    }

    #[derive(Clone, Copy, Debug, Eq)]
    pub struct Pair<T>(T, T);
    impl<T> Pair<T> {
        pub fn new(a: T, b: T) -> Self {
            Self(a, b)
        }
    }

    impl<T: PartialEq + Eq> PartialEq for Pair<T> {
        fn eq(&self, other: &Self) -> bool {
            (self.0 == other.0 && self.1 == other.1) || (self.1 == other.0 && self.0 == other.1)
        }
    }

    impl<T: Hash + Ord> Hash for Pair<T> {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            let Self(a, b) = self;
            let mut values = [a, b];
            values.sort();
            for value in values {
                value.hash(state)
            }
        }
    }

    pub trait ReadableAtomLayer {
        fn get_idxs(&self) -> HashSet<usize>;
        fn get_ids(&self) -> HashSet<String>;
        fn get_classes(&self) -> HashSet<String>;
        fn find_with_id(&self, target_id: &str) -> Option<usize>;
        fn find_with_classes(&self, target_classes: &HashSet<String>) -> HashSet<usize>;
        fn get_atom(&self, idx: usize) -> Option<&Atom>;
    }

    pub trait WritableAtomLayer {
        fn set_atom(&mut self, idx: usize, atom: Atom) -> Option<Atom>;
    }

    pub trait ReadableAtomLayerHelper: ReadableAtomLayer {
        fn max_idx(&self) -> Option<usize> {
            self.get_idxs().into_iter().max()
        }

        fn next_avaliable_idx(&self) -> usize {
            self.max_idx().map(|v| v + 1).unwrap_or(0)
        }
    }

    pub trait WritableAtomLayerHelper: ReadableAtomLayerHelper + WritableAtomLayer {
        fn add_atom(&mut self, atom: Atom) -> usize {
            let idx = self.next_avaliable_idx();
            self.set_atom(idx, atom).unwrap();
            idx
        }
    }

    pub struct AtomFillLayer(HashMap<usize, Atom>);

    impl AtomFillLayer {
        fn data(&self) -> &HashMap<usize, Atom> {
            &self.0
        }

        fn data_mut(&mut self) -> &mut HashMap<usize, Atom> {
            &mut self.0
        }
    }

    impl ReadableAtomLayer for AtomFillLayer {
        fn get_idxs(&self) -> HashSet<usize> {
            self.data()
                .keys()
                .par_bridge()
                .cloned()
                .collect::<HashSet<_>>()
        }

        fn get_ids(&self) -> HashSet<String> {
            self.data()
                .values()
                .par_bridge()
                .filter_map(|atom| atom.2.clone())
                .collect::<HashSet<_>>()
        }

        fn get_classes(&self) -> HashSet<String> {
            self.data()
                .values()
                .par_bridge()
                .map(|atom| &atom.3)
                .cloned()
                .reduce(
                    || HashSet::new(),
                    |acc, next| acc.intersection(&next).cloned().collect::<HashSet<_>>(),
                )
        }

        fn find_with_id(&self, target_id: &str) -> Option<usize> {
            self.data().par_iter().find_map_first(|(idx, atom)| {
                if let Some(id) = &atom.2 {
                    if target_id == id {
                        Some(*idx)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }

        fn find_with_classes(&self, target_classes: &HashSet<String>) -> HashSet<usize> {
            self.data()
                .par_iter()
                .filter_map(|(idx, atom)| {
                    if target_classes.is_subset(&atom.3) {
                        Some(*idx)
                    } else {
                        None
                    }
                })
                .collect::<HashSet<_>>()
        }

        fn get_atom(&self, idx: usize) -> Option<&Atom> {
            self.data().get(&idx)
        }
    }

    impl WritableAtomLayer for AtomFillLayer {
        fn set_atom(&mut self, idx: usize, atom: Atom) -> Option<Atom> {
            self.data_mut().insert(idx, atom)
        }
    }

    impl ReadableAtomLayerHelper for AtomFillLayer {}

    impl WritableAtomLayerHelper for AtomFillLayer {}

    pub trait ReadableBondLayer<BondT> {
        fn get_idxs(&self) -> HashSet<Pair<usize>>;
        fn get_value(&self, idx: &Pair<usize>) -> Option<&BondT>;
    }

    pub trait WritableBondLayer<BondT> {
        fn set_bond(&mut self, idx: Pair<usize>, bond: BondT) -> Option<BondT>;

    }

    pub struct BondFillLayer<BondT = f64>(HashMap<Pair<usize>, BondT>);

    impl<BondT> BondFillLayer<BondT> {
        fn data(&self) -> &HashMap<Pair<usize>, BondT> {
            &self.0
        }

        fn data_mut(&mut self) -> &mut HashMap<Pair<usize>, BondT> {
            &mut self.0
        }
    }

    impl<BondT> ReadableBondLayer<BondT> for BondFillLayer<BondT> {
        fn get_idxs(&self) -> HashSet<Pair<usize>> {
            self.data().keys().copied().collect::<HashSet<_>>()
        }

        fn get_value(&self, idx: &Pair<usize>) -> Option<&BondT> {
            self.data().get(idx)
        }
    }

    impl<BondT> WritableBondLayer<BondT> for BondFillLayer<BondT> {
        fn set_bond(&mut self, idx: Pair<usize>, bond: BondT) -> Option<BondT> {
            self.data_mut().insert(idx, bond)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
