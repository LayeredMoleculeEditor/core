use std::{collections::HashMap, sync::Arc};

use entity::{Layer, Molecule, Stack};
use n_to_n::NtoN;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

pub mod entity {
    use std::{
        collections::{HashMap, HashSet},
        sync::Arc,
    };

    use n_to_n::NtoN;
    use nalgebra::{Point3, Transform3};
    use pair::Pair;
    use rayon::iter::{
        IndexedParallelIterator, IntoParallelIterator, ParallelBridge, ParallelIterator,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)]
    pub struct Atom {
        element: usize,
        position: Point3<f64>,
    }

    impl Atom {
        pub fn set_element(self, element: usize) -> Self {
            Self { element, ..self }
        }

        pub fn set_position(self, position: Point3<f64>) -> Self {
            Self { position, ..self }
        }

        pub fn transform_position(self, transform: &Transform3<f64>) -> Self {
            self.set_position(transform * self.position)
        }
    }

    #[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
    pub struct Molecule {
        atoms: HashMap<usize, Option<Atom>>,
        bonds: HashMap<Pair<usize>, f64>,
        groups: NtoN<usize, String>,
    }

    impl Molecule {
        pub fn merge(mut low: Self, high: Self) -> Self {
            low.atoms.extend(high.atoms);
            low.bonds.extend(high.bonds);
            low.groups.extend(high.groups);
            low
        }
    }

    pub struct CompactedMolecule {
        atoms: Vec<Atom>,
        bonds: HashMap<Pair<usize>, f64>,
        groups: NtoN<usize, String>,
    }

    impl CompactedMolecule {
        pub fn unzip(self, offset: usize) -> Molecule {
            let atoms = self
                .atoms
                .into_par_iter()
                .enumerate()
                .map(|(idx, atom)| (idx + offset, Some(atom)))
                .collect::<HashMap<_, _>>();
            let bonds = self
                .bonds
                .into_par_iter()
                .map(|(pair, bond_order)| (pair.offset(offset), bond_order))
                .collect::<HashMap<_, _>>();
            let groups = self
                .groups
                .into_iter()
                .par_bridge()
                .map(|(idx, group_name)| (idx + offset, group_name))
                .collect::<HashSet<_>>();
            Molecule {
                atoms,
                bonds,
                groups: NtoN::from(groups),
            }
        }
    }

    #[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
    pub enum Layer {
        Fill(Molecule),
        Transform(Transform3<f64>),
        IgnoreBonds,
        ReplaceElement(usize, usize),
        RemoveElement(usize),
        PluginFilter(String, Vec<String>)
    }

    impl Layer {
        pub fn filter(&self, mut low: Molecule) -> Molecule {
            match self {
                Self::Fill(high) => Molecule::merge(low, high.clone()),
                Self::Transform(transform) => {
                    low.atoms.iter_mut().for_each(|(_, atom)| {
                        *atom = atom.map(|atom| atom.transform_position(transform))
                    });
                    low
                }
                Self::IgnoreBonds => {
                    low.bonds = HashMap::new();
                    low
                }
                Self::ReplaceElement(origin, target) => {
                    low.atoms.iter_mut().for_each(|(_, atom)| {
                        *atom = atom.map(|atom| {
                            if &atom.element == origin {
                                atom.set_element(*target)
                            } else {
                                atom
                            }
                        })
                    });
                    low
                }
                Self::RemoveElement(element) => {
                    low.atoms.iter_mut().for_each(|(_, atom)| {
                        *atom = atom.and_then(|atom| {
                            if &atom.element == element {
                                None
                            } else {
                                Some(atom)
                            }
                        })
                    });
                    low
                }
                _ => todo!(),
            }
        }
    }

    #[derive(Debug, Default, Clone, PartialEq)]
    pub struct Stack(Vec<Arc<Layer>>);

    impl Stack {
        pub fn new(layer: Vec<Arc<Layer>>) -> Self {
            Self(layer)
        }

        pub fn get_layers(&self) -> &Vec<Arc<Layer>> {
            &self.0
        }

        pub fn get_base(&self) -> Self {
            if let Some((_, layers)) = self.0.split_last() {
                Self(layers.to_vec())
            } else {
                Self(vec![])
            }
        }

        pub fn add_layer(&mut self, layer: Arc<Layer>) {
            self.0.push(layer)
        }

        pub fn write(&mut self, w: Molecule) {
            let top = self.0.last().map(|top| top.as_ref());
            if let Some(Layer::Fill(current)) = top {
                let updated = Molecule::merge(current.clone(), w);
                *self.0.last_mut().expect("Should never hint this condition") =
                    Arc::new(Layer::Fill(updated))
            } else {
                self.add_layer(Arc::new(Layer::Fill(w)))
            }
        }

        pub fn read(&self, mut container: Molecule) -> Molecule {
            for layer in &self.0 {
                container = layer.filter(container)
            }
            container
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Workspace {
    base: Molecule,
    stacks: Vec<Arc<Stack>>,
    pub atom_names: HashMap<String, usize>,
    pub groups: NtoN<String, usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct WorkspaceExport {
    base: Molecule,
    stacks: Vec<StackTree>,
    atom_names: HashMap<String, usize>,
    groups: NtoN<String, usize>,
}

impl Workspace {
    pub fn new(base: Molecule) -> Self {
        Self {
            base,
            stacks: vec![],
            atom_names: HashMap::new(),
            groups: NtoN::new(),
        }
    }

    pub fn read(&self, index: usize) -> Option<Molecule> {
        self.stacks
            .get(index)
            .map(|stack| stack.read(self.base.clone()))
    }

    pub fn stacks(&self) -> usize {
        self.stacks.len()
    }

    pub fn create_stack(&mut self, stack: Arc<Stack>, copies: usize) -> usize {
        let index = self.stacks.len();
        for _ in 0..=copies {
            self.stacks.push(stack.clone());
        }
        index
    }

    pub fn create_stack_from_layer(&mut self, layer: Arc<Layer>, copies: usize) -> usize {
        let stack = Stack::new(vec![layer]);
        self.create_stack(Arc::new(stack), copies)
    }

    pub fn clone_stack(&mut self, stack_idx: usize, copies: usize) -> Option<usize> {
        let stack = self.stacks.get(stack_idx).cloned()?;

        Some(self.create_stack(stack, copies))
    }

    pub fn clone_base(&mut self, stack_idx: usize, copies: usize) -> Option<usize> {
        let stack = self.stacks.get(stack_idx)?;
        let base = stack.get_base();
        Some(self.create_stack(Arc::new(base), copies))
    }

    pub fn write_to_stack(&mut self, start_idx: usize, range: usize, data: Molecule) -> bool {
        let max_idx = start_idx + range - 1;
        if max_idx >= self.stacks.len() {
            false
        } else {
            let stacks = (start_idx..start_idx + range)
                .par_bridge()
                .map(|i| {
                    let mut stack = self.stacks[i].as_ref().clone();
                    stack.write(data.clone());
                    stack
                })
                .collect::<Vec<_>>();
            for (i, stack) in stacks.into_iter().enumerate() {
                self.stacks[i + start_idx] = Arc::new(stack)
            }
            true
        }
    }

    pub fn add_layer_to_stack(
        &mut self,
        start_idx: usize,
        range: usize,
        layer: Arc<Layer>,
    ) -> bool {
        let max_idx = start_idx + range - 1;
        if max_idx >= self.stacks.len() {
            false
        } else {
            let stacks = (start_idx..start_idx + range)
                .par_bridge()
                .map(|i| {
                    let mut stack = self.stacks[i].as_ref().clone();
                    stack.add_layer(layer.clone());
                    stack
                })
                .collect::<Vec<_>>();
            for (i, stack) in stacks.into_iter().enumerate() {
                self.stacks[i + start_idx] = Arc::new(stack);
            }
            true
        }
    }
}

impl From<&Workspace> for WorkspaceExport {
    fn from(value: &Workspace) -> Self {
        Self {
            base: value.base.clone(),
            stacks: StackTree::dehydration(&value.stacks),
            atom_names: value.atom_names.clone(),
            groups: value.groups.clone(),
        }
    }
}

impl Into<Workspace> for &WorkspaceExport {
    fn into(self) -> Workspace {
        let stacks = StackTree::hydration(&self.stacks);
        Workspace {
            base: self.base.clone(),
            stacks,
            atom_names: self.atom_names.clone(),
            groups: self.groups.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct StackTree {
    layer: Layer,
    indexes: Vec<usize>,
    children: Vec<StackTree>,
}

impl StackTree {
    pub fn dehydration<'a, I>(stacks: I) -> Vec<StackTree>
    where
        I: IntoIterator<Item = &'a Arc<Stack>>,
    {
        let mut trees = vec![];
        for (idx, stack) in stacks.into_iter().enumerate() {
            let matched = trees
                .iter_mut()
                .map(|tree: &mut StackTree| tree.merge(idx, stack.get_layers()))
                .any(|result| result);
            if !matched {
                trees.push(StackTree::from((stack.get_layers().as_slice(), idx)))
            }
        }
        trees
    }

    pub fn hydration<'a, I>(trees: I) -> Vec<Arc<Stack>>
    where
        I: IntoIterator<Item = &'a StackTree>,
    {
        let mut stacks: HashMap<usize, Arc<Stack>> = HashMap::new();

        for tree in trees.into_iter() {
            stacks.extend(tree.to_stacks(&vec![]));
        }

        let mut stacks = stacks.into_iter().collect::<Vec<_>>();
        stacks.sort_by(|(a, _), (b, _)| a.cmp(b));
        stacks.into_iter().map(|(_, stack)| stack).collect()
    }

    fn to_stacks(&self, base: &Vec<Arc<Layer>>) -> HashMap<usize, Arc<Stack>> {
        let mut map = HashMap::new();
        let mut base = base.clone();
        base.push(Arc::new(self.layer.clone()));
        for index in &self.indexes {
            map.insert(*index, Arc::new(Stack::new(base.clone())));
        }
        for child in &self.children {
            map.extend(child.to_stacks(&base));
        }
        map
    }

    fn merge(&mut self, idx: usize, layers: &[Arc<Layer>]) -> bool {
        let (current, elements) = layers
            .split_first()
            .expect("Should never hint this condition");
        if current.as_ref() == &self.layer {
            if elements.len() == 0 {
                self.indexes.push(idx);
            } else {
                let matched = self
                    .children
                    .iter_mut()
                    .map(|item| item.merge(idx, elements))
                    .any(|result| result);
                if !matched {
                    self.children.push(StackTree::from((elements, idx)))
                }
            }
            true
        } else {
            false
        }
    }
}

impl From<(&[Arc<Layer>], usize)> for StackTree {
    fn from((stack, idx): (&[Arc<Layer>], usize)) -> Self {
        let (bottom, highers) = stack.split_first().expect("Don't create with empty stack");
        if highers.len() == 0 {
            Self {
                layer: bottom.as_ref().clone(),
                indexes: vec![idx],
                children: vec![],
            }
        } else {
            Self {
                layer: bottom.as_ref().clone(),
                indexes: vec![],
                children: vec![StackTree::from((highers, idx))],
            }
        }
    }
}
