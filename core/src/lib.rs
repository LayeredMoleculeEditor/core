use std::{collections::HashMap, sync::Arc};

use entity::{Layer, Molecule, Stack};
use n_to_n::NtoN;
use rayon::prelude::*;
use serde::{de::value, Deserialize, Serialize};

mod entity {
    use std::{collections::HashMap, sync::Arc};

    use n_to_n::NtoN;
    use nalgebra::{Point3, Transform3};
    use pair::Pair;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)]
    pub struct Atom {
        element: usize,
        position: Point3<f64>,
    }

    impl Atom {
        pub fn transform_position(self, transform: &Transform3<f64>) -> Self {
            Self {
                position: transform * self.position,
                ..self
            }
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

    #[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
    pub enum Layer {
        Fill(Molecule),
        Transform(Transform3<f64>),
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
    caches: Vec<Molecule>,
    atom_names: HashMap<String, usize>,
    groups: NtoN<String, usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct WorkspaceExport {
    base: Molecule,
    stacks: Vec<StackTree>,
    atom_names: HashMap<String, usize>,
    groups: NtoN<String, usize>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_stack(&mut self, stack: Arc<Stack>, copies: usize) -> usize {
        let index = self.stacks.len();
        let cache = stack.read(self.base.clone());
        for _ in 0..=copies {
            self.stacks.push(stack.clone());
            self.caches.push(cache.clone());
        }
        index
    }

    pub fn create_stack_from_layer(&mut self, layer: Arc<Layer>, copies: usize) -> usize {
        let stack = Stack::new(vec![layer]);
        self.create_stack(Arc::new(stack), copies)
    }

    pub fn clone_stack(
        &mut self,
        stack_idx: usize,
        copies: usize,
    ) -> Result<usize, WorkspaceError> {
        let stack = self
            .stacks
            .get(stack_idx)
            .cloned()
            .ok_or(WorkspaceError::StacksOutOfIndex)?;

        Ok(self.create_stack(stack, copies))
    }

    pub fn clone_base(&mut self, stack_idx: usize, copies: usize) -> Result<usize, WorkspaceError> {
        let stack = self
            .stacks
            .get(stack_idx)
            .ok_or(WorkspaceError::StacksOutOfIndex)?;
        let base = stack.get_base();
        Ok(self.create_stack(Arc::new(base), copies))
    }

    pub fn write_to_stack(
        &mut self,
        start_idx: usize,
        copies: usize,
        data: Molecule,
    ) -> Result<(), WorkspaceError> {
        let max_idx = start_idx + copies - 1;
        if max_idx >= self.stacks.len() {
            Err(WorkspaceError::StacksOutOfIndex)
        } else {
            let stacks = (start_idx..start_idx + copies)
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
            Ok(())
        }
    }

    pub fn add_layer_to_stack(
        &mut self,
        start_idx: usize,
        copies: usize,
        layer: Arc<Layer>,
    ) -> Result<(), WorkspaceError> {
        let max_idx = start_idx + copies - 1;
        if max_idx >= self.stacks.len() {
            Err(WorkspaceError::StacksOutOfIndex)
        } else {
            let stacks = (start_idx..start_idx + copies)
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
            Ok(())
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
        let caches = stacks
            .iter()
            .map(|stack| stack.read(self.base.clone()))
            .collect();
        Workspace {
            base: self.base.clone(),
            stacks,
            caches,
            atom_names: self.atom_names.clone(),
            groups: self.groups.clone(),
        }
    }
}

pub enum WorkspaceError {
    LayerStoreOutOfIndex,
    StacksOutOfIndex,
    NoBase,
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
