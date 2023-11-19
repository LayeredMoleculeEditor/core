use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
    iter::Zip,
    ops::Add,
    slice::Iter,
    vec::IntoIter,
};

use nalgebra::{Unit, Vector3};
use serde::{Deserialize, Serialize};
use rayon::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueValueMap<K: Hash + Eq + Clone, V: Hash + Eq + Clone> {
    map: HashMap<K, V>,
    validate: HashSet<V>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertResult<K, V> {
    Updated(V),
    Duplicated(K),
    Created,
}

impl<K: Clone + Hash + Eq, V: Clone + Hash + Eq> UniqueValueMap<K, V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            validate: HashSet::new(),
        }
    }

    pub fn from_map(map: HashMap<K, V>) -> Result<Self, HashMap<K, V>> {
        let validate = map.values().cloned().collect::<HashSet<_>>();
        if validate.len() == map.len() {
            Ok(Self { map, validate })
        } else {
            Err(map)
        }
    }

    pub fn data(&self) -> &HashMap<K, V> {
        &self.map
    }

    pub fn insert(&mut self, k: K, v: V) -> InsertResult<K, V> {
        if self.validate.contains(&v) {
            self.map
                .iter()
                .find_map(|(key, value)| if value == &v { Some(key) } else { None })
                .and_then(|key| Some(InsertResult::Duplicated(key.clone())))
                .unwrap()
        } else {
            self.validate.insert(v.clone());
            let result = self
                .map
                .insert(k, v)
                .and_then(|v| Some(InsertResult::Updated(v)))
                .unwrap_or(InsertResult::Created);
            if let InsertResult::Updated(v) = &result {
                self.validate.remove(v);
            }
            result
        }
    }

    pub fn remove<Q: Hash + Eq + ?Sized>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        let removed = self.map.remove(k);
        if let Some(value) = &removed {
            self.validate.remove(value);
        }
        removed
    }
}

#[test]
fn uniq_val_map() {
    let mut map: UniqueValueMap<String, usize> = UniqueValueMap::new();
    let x = map.insert("center".to_string(), 1);
    let y = map.insert("center".to_string(), 2);
    let z = map.insert("entry".to_string(), 2);
    assert_eq!(x, InsertResult::Created);
    assert_eq!(y, InsertResult::Updated(1));
    assert_eq!(z, InsertResult::Duplicated("center".to_string()));
    assert_eq!(
        map,
        UniqueValueMap {
            map: HashMap::from([("center".to_string(), 2)]),
            validate: HashSet::from([2])
        }
    )
}

pub struct NtoN<L, R>(HashSet<(L, R)>);

impl<L: Eq + Hash + Clone, R: Eq + Hash + Clone> NtoN<L, R> {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn data(&self) -> &HashSet<(L, R)> {
        &self.0
    }

    pub fn get_lefts(&self) -> HashSet<&L> {
        self.0.iter().map(|(l, _)| l).collect()
    }

    pub fn get_rights(&self) -> HashSet<&R> {
        self.0.iter().map(|(_, r)| r).collect()
    }

    pub fn get_left(&self, left: &L) -> HashSet<&R> {
        self.0
            .iter()
            .filter_map(|(l, r)| if l == left { Some(r) } else { None })
            .collect()
    }

    pub fn get_right(&self, right: &R) -> HashSet<&L> {
        self.0
            .iter()
            .filter_map(|(l, r)| if r == right { Some(l) } else { None })
            .collect()
    }

    pub fn insert(&mut self, left: L, right: R) -> bool {
        self.0.insert((left, right))
    }

    pub fn remove(&mut self, left: &L, right: &R) -> bool {
        self.0.remove(&(left.clone(), right.clone()))
    }

    pub fn remove_left(&mut self, left: &L) {
        self.0.retain(|(l, _)| l != left)
    }

    pub fn remove_right(&mut self, right: &R) {
        self.0.retain(|(_, r)| r != right)
    }
}

impl<K, V> From<HashSet<(K, V)>> for NtoN<K, V> {
    fn from(value: HashSet<(K, V)>) -> Self {
        Self(value)
    }
}

impl<K, V> Into<HashSet<(K, V)>> for NtoN<K, V> {
    fn into(self) -> HashSet<(K, V)> {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair<T>(T, T);

impl<T: Eq> Pair<T> {
    pub fn get_another(&self, current: &T) -> Option<&T> {
        let Self(a, b) = self;
        if a == current {
            Some(b)
        } else if b == current {
            Some(a)
        } else {
            None
        }
    }

    pub fn contains(&self, current: &T) -> bool {
        self.get_another(current).is_some()
    }
}

impl<T: Copy + Ord + Add<Output = T>> Add<T> for Pair<T> {
    type Output = Pair<T>;
    fn add(self, rhs: T) -> Self::Output {
        Pair::from((self.0 + rhs, self.1 + rhs))
    }
}

impl<T: Ord> From<(T, T)> for Pair<T> {
    fn from((a, b): (T, T)) -> Self {
        Self::from([a, b])
    }
}

impl<T: Ord> From<[T; 2]> for Pair<T> {
    fn from(mut value: [T; 2]) -> Self {
        value.sort();
        let [a, b] = value;
        Self(a, b)
    }
}

impl<T: Hash> Hash for Pair<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self(a, b) = self;
        a.hash(state);
        b.hash(state);
    }
}

impl<T> Into<(T, T)> for Pair<T> {
    fn into(self) -> (T, T) {
        let Pair(a, b) = self;
        (a, b)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BondGraph {
    indexes: Vec<Pair<usize>>,
    values: Vec<Option<f64>>,
}

impl<'a> BondGraph {
    pub fn new() -> Self {
        Self {
            indexes: vec![],
            values: vec![],
        }
    }

    pub fn offset(&mut self, offset: usize) {
        for index in self.indexes.iter_mut() {
            *index = *index + offset;
        }
    }

    fn position(&self, key: &Pair<usize>) -> Option<usize> {
        self.indexes.par_iter().position_any(|k| k == key)
    }

    pub fn insert(&mut self, key: Pair<usize>, value: Option<f64>) -> Option<Option<f64>> {
        if let Some(position) = self.position(&key) {
            let origin = self.values[position];
            self.values[position] = value;
            Some(origin)
        } else {
            self.indexes.push(key);
            self.values.push(value);
            None
        }
    }

    pub fn remove(&mut self, key: &Pair<usize>) -> Option<Option<f64>> {
        if let Some(position) = self.position(key) {
            self.indexes.remove(position);
            Some(self.values.remove(position))
        } else {
            None
        }
    }

    pub fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = (&'a Pair<usize>, &'a Option<f64>)>,
    {
        for (key, value) in iter {
            self.insert(key.clone(), value.clone());
        }
    }

    pub fn clear(&mut self) {
        self.indexes.clear();
        self.values.clear();
    }
}

impl Default for BondGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> IntoIterator for &'a BondGraph {
    type Item = (&'a Pair<usize>, &'a Option<f64>);
    type IntoIter = Zip<Iter<'a, Pair<usize>>, Iter<'a, Option<f64>>>;
    fn into_iter(self) -> Self::IntoIter {
        self.indexes.iter().zip(self.values.iter())
    }
}

impl IntoIterator for BondGraph {
    type Item = (Pair<usize>, Option<f64>);
    type IntoIter = Zip<IntoIter<Pair<usize>>, IntoIter<Option<f64>>>;
    fn into_iter(self) -> Self::IntoIter {
        self.indexes.into_iter().zip(self.values.into_iter())
    }
}

impl From<HashMap<Pair<usize>, f64>> for BondGraph {
    fn from(value: HashMap<Pair<usize>, f64>) -> Self {
        let (indexes, values): (Vec<Pair<usize>>, Vec<f64>) = value.into_par_iter().unzip();
        Self {
            indexes,
            values: values.into_par_iter().map(|bond| Some(bond)).collect(),
        }
    }
}

impl From<HashMap<Pair<usize>, Option<f64>>> for BondGraph {
    fn from(value: HashMap<Pair<usize>, Option<f64>>) -> Self {
        let (indexes, values): (Vec<Pair<usize>>, Vec<Option<f64>>) = value.into_iter().unzip();
        Self { indexes, values }
    }
}

#[test]
fn bond_graph_serde() {
    let mut bg = BondGraph::new();
    bg.insert(Pair(1, 2), Some(1.5));
    bg.insert(Pair(3, 4), Some(1.5));
    println!("{:#?}", serde_json::to_string(&bg));
}

/// Returns a rotation axis and angle for rotate `a` to `b`
pub fn vector_align_rotation(a: &Vector3<f64>, b: &Vector3<f64>) -> (Vector3<f64>, f64) {
    let a = Unit::new_normalize(*a);
    let b = Unit::new_normalize(*b);
    let axis = a.cross(&b);
    let angle = a.dot(&b).acos();
    (axis, angle)
}
