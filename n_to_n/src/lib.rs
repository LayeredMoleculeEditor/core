use std::collections::hash_set::IntoIter;
use std::collections::HashSet;
use std::hash::Hash;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NtoN<L: Eq + Hash, R: Eq + Hash>(HashSet<(L, R)>);

impl<L: Sync + Send + Eq + Hash + Clone, R: Sync + Send + Eq + Hash + Clone> NtoN<L, R> {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn data(&self) -> &HashSet<(L, R)> {
        &self.0
    }

    fn data_mut(&mut self) -> &mut HashSet<(L, R)> {
        &mut self.0
    }

    pub fn get_lefts(&self) -> HashSet<L> {
        self.data().par_iter().map(|(l, _)| l).cloned().collect()
    }

    pub fn get_rights(&self) -> HashSet<R> {
        self.data().par_iter().map(|(_, r)| r).cloned().collect()
    }

    pub fn get_left(&self, left: &L) -> HashSet<R> {
        self.data()
            .iter()
            .filter_map(|(l, r)| if l == left { Some(r) } else { None })
            .cloned()
            .collect()
    }

    pub fn get_right(&self, right: &R) -> HashSet<L> {
        self.data()
            .iter()
            .filter_map(|(l, r)| if r == right { Some(l) } else { None })
            .cloned()
            .collect()
    }

    pub fn insert(&mut self, left: L, right: R) -> bool {
        self.data_mut().insert((left, right))
    }

    pub fn remove(&mut self, left: &L, right: &R) -> bool {
        self.data_mut().remove(&(left.clone(), right.clone()))
    }

    pub fn remove_left(&mut self, left: &L) {
        self.data_mut().retain(|(l, _)| l != left)
    }

    pub fn remove_right(&mut self, right: &R) {
        self.data_mut().retain(|(_, r)| r != right)
    }

    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (L, R)>,
    {
        self.data_mut().extend(iter)
    }

    pub fn overlay_to(&self, other: &Self) -> Self {
        let mut overlayed = other.clone();
        overlayed.extend(self.data().clone());
        overlayed
    }
}

impl<L: Eq + Hash, R: Eq + Hash> From<HashSet<(L, R)>> for NtoN<L, R> {
    fn from(value: HashSet<(L, R)>) -> Self {
        Self(value)
    }
}

impl<L: Eq + Hash, R: Eq + Hash> Into<HashSet<(L, R)>> for NtoN<L, R> {
    fn into(self) -> HashSet<(L, R)> {
        self.0
    }
}

impl<L: Eq + Hash, R: Eq + Hash> IntoIterator for NtoN<L, R> {
    type Item = (L, R);
    type IntoIter = IntoIter<(L, R)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
