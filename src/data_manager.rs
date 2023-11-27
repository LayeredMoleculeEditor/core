use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::Arc,
};

use async_recursion::async_recursion;
use tokio::{io::AsyncWriteExt, join, process::Command, sync::RwLock};

use futures::future::join_all;
use lazy_static::lazy_static;
use nalgebra::{Matrix3, Vector3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    error::LMECoreError,
    serde::{de_arc_layer, de_m3_64, de_v3_64, ser_arc_layer, ser_m3_64, ser_v3_64},
    utils::{BondGraph, InsertResult, NtoN, Pair, UniqueValueMap},
};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Atom {
    element: usize,
    #[serde(serialize_with = "ser_v3_64", deserialize_with = "de_v3_64")]
    position: Vector3<f64>,
}

impl Atom {
    pub fn new(element: usize, position: Vector3<f64>) -> Self {
        Self { element, position }
    }

    pub fn update_position<F>(self, f: F) -> Self
    where
        F: Fn(Vector3<f64>) -> Vector3<f64>,
    {
        Self::new(self.element, f(self.position))
    }

    pub fn get_element(&self) -> &usize {
        &self.element
    }

    pub fn get_position(&self) -> &Vector3<f64> {
        &self.position
    }
}

type AtomTable = HashMap<usize, Option<Atom>>;
pub type Molecule = (AtomTable, BondGraph);
pub type CleanedMolecule = (Vec<Atom>, Vec<Pair<usize>>, Vec<f64>);

pub fn clean_molecule(input: Molecule) -> CleanedMolecule {
    let (atoms, bonds) = input;
    let mut atoms = atoms
        .into_par_iter()
        .filter_map(|(idx, atom)| atom.map(|atom| (idx, atom)))
        .collect::<Vec<_>>();
    atoms.sort_by(|(a, _), (b, _)| a.cmp(b));
    let idx_map = atoms
        .par_iter()
        .enumerate()
        .map(|(new_idx, (old_idx, _))| (*old_idx, new_idx))
        .collect::<HashMap<_, _>>();
    let atoms = atoms.into_iter().map(|(_, atom)| atom).collect::<Vec<_>>();
    let mut bonds = bonds.into_iter()
        .collect::<Vec<_>>();
    bonds.sort_by(|(a, _), (b, _)| a.cmp(b));
    let (bonds_idxs, bonds_values): (Vec<Pair<usize>>, Vec<f64>) = bonds
        .into_iter()
        .par_bridge()
        .filter_map(|(pair, bond)| bond.map(|bond| (pair, bond)))
        .filter_map(|(pair, bond)| {
            let (a, b): (usize, usize) = pair.into();
            let (a, b) = idx_map.get(&a).copied().zip(idx_map.get(&b).copied())?;
            Some((Pair::from((a, b)), bond))
        })
        .unzip();
    (atoms, bonds_idxs, bonds_values)
}

pub fn empty_tables() -> Molecule {
    (HashMap::new(), BondGraph::new())
}

lazy_static! {
    static ref EMPTY_TABLES: Molecule = empty_tables();
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum Layer {
    Transparent,
    Fill {
        #[serde(default)]
        atoms: AtomTable,
        #[serde(default)]
        bonds: BondGraph,
    },
    HideBonds,
    HideHydrogens {
        valence_table: HashMap<usize, usize>,
    },
    Rotation {
        #[serde(serialize_with = "ser_m3_64", deserialize_with = "de_m3_64")]
        matrix: Matrix3<f64>,
        #[serde(serialize_with = "ser_v3_64", deserialize_with = "de_v3_64")]
        center: Vector3<f64>,
    },
    Translate {
        #[serde(serialize_with = "ser_v3_64", deserialize_with = "de_v3_64")]
        vector: Vector3<f64>,
    },
    Plugin {
        command: String,
        args: Vec<String>,
    },
}

impl Layer {
    pub async fn read(&self, base: &Molecule) -> Result<Molecule, LMECoreError> {
        let (mut atom_table, mut bond_table) = base.clone();
        match self {
            Self::Transparent => {}
            Self::Fill { atoms, bonds } => {
                atom_table.extend(atoms);
                bond_table.extend(bonds);
            }
            Self::HideBonds => {
                bond_table.clear();
            }
            Self::HideHydrogens { valence_table } => {
                todo!()
            }
            Self::Rotation { matrix, center } => {
                let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
                    .into_par_iter()
                    .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
                    .unzip();
                let atoms = atoms.into_par_iter().map(|Atom { element, position }| {
                    Some(Atom {
                        element,
                        position: ((position - center).transpose() * matrix).transpose() - center,
                    })
                });
                atom_table = idxs
                    .into_par_iter()
                    .zip(atoms.into_par_iter())
                    .collect::<HashMap<_, _>>();
            }
            Self::Translate { vector } => {
                let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
                    .into_par_iter()
                    .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
                    .unzip();
                let atoms = atoms.into_par_iter().map(|Atom { element, position }| {
                    Some(Atom {
                        element,
                        position: position + vector,
                    })
                });
                atom_table = idxs
                    .into_par_iter()
                    .zip(atoms.into_par_iter())
                    .collect::<HashMap<_, _>>();
            }
            Self::Plugin { command, args } => {
                let mut child = Command::new(command)
                    .args(args)
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|err| LMECoreError::PluginLayerError(-1, err.to_string()))?;
                let data_to_send = serde_json::to_string(&(&atom_table, &bond_table))
                    .map_err(|err| LMECoreError::PluginLayerError(-2, err.to_string()))?;
                if let Some(ref mut stdin) = child.stdin {
                    stdin
                        .write_all(&data_to_send.as_bytes())
                        .await
                        .map_err(|err| LMECoreError::PluginLayerError(-3, err.to_string()))?;
                    let output = child
                        .wait_with_output()
                        .await
                        .map_err(|err| LMECoreError::PluginLayerError(-4, err.to_string()))?;
                    let data = String::from_utf8_lossy(&output.stdout);
                    let (atoms, bonds): Molecule = serde_json::from_str(&data)
                        .map_err(|err| LMECoreError::PluginLayerError(-5, err.to_string()))?;
                    atom_table = atoms;
                    bond_table = bonds;
                } else {
                    Err(LMECoreError::PluginLayerError(
                        -6,
                        "Unable to get stdin of child process".to_string(),
                    ))?;
                }
            }
        };
        Ok((atom_table, bond_table))
    }

    pub fn write(&mut self, patch: &Molecule) -> Result<(), LMECoreError> {
        if let Self::Fill { atoms, bonds } = self {
            let (patch_atoms, patch_bonds) = patch;
            atoms.extend(patch_atoms);
            bonds.extend(patch_bonds);
            Ok(())
        } else {
            Err(LMECoreError::NotFillLayer)
        }
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self::Fill {
            atoms: HashMap::new(),
            bonds: BondGraph::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Stack {
    config: Layer,
    #[serde(serialize_with = "ser_arc_layer", deserialize_with = "de_arc_layer")]
    base: Option<Arc<Stack>>,
    cached: Molecule,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            config: Layer::Transparent,
            base: None,
            cached: empty_tables(),
        }
    }
}

impl Stack {
    pub fn top(&self) -> &Layer {
        &self.config
    }

    pub async fn overlay(base: Option<Arc<Self>>, config: Layer) -> Result<Self, LMECoreError> {
        let cached = if let Some(base) = base.clone() {
            config.read(&base.cached).await?
        } else {
            Ok(empty_tables())?
        };
        Ok(Self {
            config,
            base,
            cached,
        })
    }

    pub fn read(&self) -> &Molecule {
        &self.cached
    }

    pub async fn write(&mut self, patch: &Molecule) -> Result<(), LMECoreError> {
        self.config.write(patch)?;
        let base = self
            .base
            .as_ref()
            .map(|layer| &layer.cached)
            .unwrap_or(&EMPTY_TABLES);
        self.cached = self.config.read(base).await?;
        Ok(())
    }

    pub fn clone_base(&self) -> Option<Arc<Self>> {
        self.base.as_ref().map(|value| value.clone())
    }

    pub fn len(&self) -> usize {
        if let Some(base) = &self.base {
            base.len() + 1
        } else {
            1
        }
    }

    pub fn get_deep_layer(&self, layer: usize) -> Result<Layer, &'static str> {
        if layer >= self.len() {
            Err("Layer number out of layers")
        } else if layer == self.len() - 1 {
            Ok(self.config.clone())
        } else {
            self.base
                .as_ref()
                .expect("should never found None base in condition")
                .get_deep_layer(layer)
        }
    }

    pub fn get_layers(&self) -> Vec<Layer> {
        (0..self.len())
            .map(|layer| self.get_deep_layer(layer))
            .collect::<Result<Vec<_>, _>>()
            .expect("should never hint this condition")
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LayerTree {
    config: Layer,
    indexes: Vec<usize>,
    children: Vec<Box<LayerTree>>,
}

impl LayerTree {
    #[async_recursion]
    pub async fn to_stack(
        &self,
        base: Option<Arc<Stack>>,
    ) -> Result<HashMap<usize, Arc<Stack>>, LMECoreError> {
        let layer = Arc::new(Stack::overlay(base, self.config.clone()).await?);
        let mut stacks = self
            .indexes
            .iter()
            .map(|idx| (*idx, layer.clone()))
            .collect::<HashMap<_, _>>();
        for child in &self.children {
            let sub_layers = child.to_stack(Some(layer.clone())).await?;
            stacks.extend(sub_layers);
        }
        Ok(stacks)
    }

    pub fn merge(&mut self, mut reversed_stack: Vec<Layer>, index: usize) -> Option<Vec<Layer>> {
        let root = reversed_stack
            .last()
            .expect("Each stack input should always contains at least 1 element");
        if root == &self.config {
            reversed_stack.pop();
            if reversed_stack.len() == 0 {
                self.indexes.push(index);
            } else {
                let mut reversed_stack = Some(reversed_stack);
                for sub_tree in self.children.iter_mut() {
                    if let Some(to_merge) = reversed_stack {
                        reversed_stack = sub_tree.merge(to_merge, index);
                    }
                }
                if let Some(mut reversed_stack) = reversed_stack {
                    reversed_stack.reverse();
                    self.children
                        .push(Box::new(LayerTree::from((reversed_stack, index))));
                }
            }
            None
        } else {
            Some(reversed_stack)
        }
    }
}

impl From<(Stack, usize)> for LayerTree {
    fn from((stack, index): (Stack, usize)) -> Self {
        Self::from((stack.get_layers(), index))
    }
}

impl From<(Vec<Layer>, usize)> for LayerTree {
    fn from((layers, index): (Vec<Layer>, usize)) -> Self {
        let mut stack = layers;
        let mut layer_tree = Box::new(Self {
            config: stack
                .pop()
                .expect("Each stack input should always contains at least 1 element"),
            indexes: vec![index],
            children: vec![],
        });
        while let Some(config) = stack.pop() {
            layer_tree = Box::new(Self {
                config,
                indexes: vec![],
                children: vec![layer_tree],
            })
        }
        *layer_tree
    }
}

#[test]
fn merge_layer() {
    let ser = serde_json::to_string(&HashMap::from([((1, 2), 3.), ((1, 0), 1.)]));
    println!("{}", ser.unwrap());
}

pub fn arc_rwlock<T>(value: T) -> Arc<RwLock<T>> {
    Arc::new(RwLock::new(value))
}

#[derive(Clone)]
pub struct Workspace {
    stacks: Arc<RwLock<Vec<Arc<Stack>>>>,
    id_map: Arc<RwLock<UniqueValueMap<usize, String>>>,
    class_map: Arc<RwLock<NtoN<usize, String>>>,
}

impl Workspace {
    pub fn new() -> Self {
        Workspace {
            stacks: arc_rwlock(vec![Arc::new(Stack::default())]),
            id_map: arc_rwlock(UniqueValueMap::new()),
            class_map: arc_rwlock(NtoN::new()),
        }
    }

    pub async fn get_stack(&self, idx: usize) -> Result<Arc<Stack>, LMECoreError> {
        if let Some(stack) = self.stacks.read().await.get(idx) {
            Ok(stack.clone())
        } else {
            Err(LMECoreError::NoSuchStack)
        }
    }

    async fn update_stacks(
        &self,
        patches: &HashMap<usize, Arc<Stack>>,
    ) -> Result<(), LMECoreError> {
        let existed_stacks_amount = self.stacks.read().await.len();
        if let Some(_) = patches.keys().position(|idx| existed_stacks_amount <= *idx) {
            Err(LMECoreError::NoSuchStack)
        } else {
            let mut stacks = self.stacks.write().await;
            for (idx, stack) in patches {
                *stacks.get_mut(*idx).unwrap() = stack.clone();
            }
            Ok(())
        }
    }

    async fn get_stacks(&self, indexes: &Vec<usize>) -> Result<Vec<Arc<Stack>>, LMECoreError> {
        let existed_stacks = self.stacks.read().await;
        if let Some(_) = indexes.iter().position(|idx| existed_stacks.len() <= *idx) {
            Err(LMECoreError::NoSuchStack)
        } else {
            Ok(indexes
                .iter()
                .map(|idx| existed_stacks.get(*idx).unwrap().clone())
                .collect())
        }
    }

    async fn update_stack(&self, idx: usize, stack: Arc<Stack>) -> Result<(), LMECoreError> {
        if let Some(current) = self.stacks.write().await.get_mut(idx) {
            *current = stack;
            Ok(())
        } else {
            Err(LMECoreError::NoSuchStack)
        }
    }

    pub async fn get_all_stack(&self) -> Vec<usize> {
        self.stacks
            .read()
            .await
            .par_iter()
            .map(|stack| stack.len())
            .collect::<Vec<_>>()
    }

    pub async fn new_empty_stack(&self) {
        self.stacks.write().await.push(Arc::new(Stack::default()));
    }

    pub async fn remove_stack(&self, idx: usize) {
        self.stacks.write().await.remove(idx);
    }

    pub async fn clone_stack(&self, idx: usize, amount: usize) -> Result<usize, LMECoreError> {
        let stack = self.get_stack(idx).await?;
        let mut stacks = self.stacks.write().await;
        for _ in 0..amount {
            stacks.push(stack.clone());
        }
        Ok(stacks.len() - 1)
    }

    pub async fn clone_base(&self, idx: usize, amount: usize) -> Result<usize, LMECoreError> {
        let stack = self.get_stack(idx).await?;
        if let Some(base) = stack.clone_base() {
            let mut stacks = self.stacks.write().await;
            for _ in 0..amount {
                stacks.push(base.clone());
            }
            Ok(stacks.len() - 1)
        } else {
            Err(LMECoreError::RootLayerError)
        }
    }

    pub async fn overlay_to(
        &self,
        indexes: &Vec<usize>,
        config: Layer,
    ) -> Result<(), LMECoreError> {
        let stacks = self.get_stacks(indexes).await?;
        let overlays = stacks
            .into_iter()
            .map(|stack| Stack::overlay(Some(stack), config.clone()))
            .collect::<Vec<_>>();
        let overlayeds = join_all(overlays)
            .await
            .into_iter()
            .map(|value| value.map(|value| Arc::new(value)))
            .collect::<Result<Vec<_>, _>>()?;
        let patches = indexes
            .iter()
            .cloned()
            .enumerate()
            .map(|(idx, stack_idx)| (stack_idx, overlayeds.get(idx).unwrap().clone()))
            .collect::<HashMap<_, _>>();
        self.update_stacks(&patches).await
    }

    pub async fn write_to_layer(&self, idx: usize, patch: &Molecule) -> Result<(), LMECoreError> {
        let stack = self.get_stack(idx).await?;
        let mut updated = stack.as_ref().clone();
        updated.write(patch).await?;
        self.update_stack(idx, Arc::new(updated)).await
    }

    pub async fn list_ids(&self) -> HashSet<String> {
        self.id_map.read().await.data().values().cloned().collect()
    }

    pub async fn id_to_index(&self, target: &String) -> Option<usize> {
        self.id_map
            .read()
            .await
            .data()
            .par_iter()
            .find_map_first(|(idx, id)| if target == id { Some(*idx) } else { None })
    }

    pub async fn set_id(&self, idx: usize, id: String) -> InsertResult<usize, String> {
        self.id_map.write().await.insert(idx, id)
    }

    pub async fn remove_id(&self, idx: usize) {
        self.id_map.write().await.remove(&idx);
    }

    pub async fn index_to_id(&self, idx: usize) -> Option<String> {
        self.id_map.read().await.data().get(&idx).cloned()
    }

    pub async fn set_to_class(&self, idx: usize, class: String) {
        self.class_map.write().await.insert(idx, class);
    }

    pub async fn remove_from_class(&self, idx: usize, class: &String) {
        self.class_map.write().await.remove(&idx, class);
    }

    pub async fn remove_from_all_class(&self, idx: usize) {
        self.class_map.write().await.remove_left(&idx);
    }

    pub async fn remove_class(&self, class: &String) {
        self.class_map.write().await.remove_right(class);
    }

    pub async fn class_indexes(&self, class: &String) -> HashSet<usize> {
        self.class_map.read().await.get_right(class)
    }

    pub async fn get_classes(&self, idx: usize) -> HashSet<String> {
        self.class_map.read().await.get_left(&idx)
    }

    pub async fn list_classes(&self) -> HashSet<String> {
        self.class_map.read().await.get_rights()
    }

    pub async fn export(&self) -> (LayerTree, HashMap<usize, String>, HashSet<(usize, String)>) {
        let (stacks, ids, classes) = join!(
            self.stacks.read(),
            self.id_map.read(),
            self.class_map.read()
        );
        let mut layer_tree = LayerTree::from((stacks[0].as_ref().clone(), 0));
        for (idx, stack) in stacks[1..].to_vec().iter().enumerate() {
            let mut reversed_stack = stack.get_layers();
            reversed_stack.reverse();
            if layer_tree.merge(reversed_stack, idx + 1).is_some() {
                panic!("All stacks should based on same Transparent Layer")
            }
        }
        (layer_tree, ids.data().clone(), classes.data().clone())
    }
}

impl
    From<(
        Vec<Arc<Stack>>,
        UniqueValueMap<usize, String>,
        NtoN<usize, String>,
    )> for Workspace
{
    fn from(
        value: (
            Vec<Arc<Stack>>,
            UniqueValueMap<usize, String>,
            NtoN<usize, String>,
        ),
    ) -> Self {
        let (stacks, id_map, class_map) = value;
        Self {
            stacks: arc_rwlock(stacks),
            id_map: arc_rwlock(id_map),
            class_map: arc_rwlock(class_map),
        }
    }
}

pub type ServerStore = Arc<RwLock<HashMap<String, Workspace>>>;

pub fn create_server_store() -> ServerStore {
    Arc::new(RwLock::new(HashMap::new()))
}
