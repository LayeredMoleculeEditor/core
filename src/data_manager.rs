use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::Arc,
};

use async_recursion::async_recursion;
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::RwLock,
};

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
pub type CleanedMolecule = (Vec<Atom>, HashMap<Pair<usize>, f64>);

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
    let bonds = bonds
        .into_iter()
        .par_bridge()
        .filter_map(|(pair, bond)| bond.map(|bond| (pair, bond)))
        .filter_map(|(pair, bond)| {
            let (a, b): (usize, usize) = pair.into();
            let (a, b) = idx_map.get(&a).copied().zip(idx_map.get(&b).copied())?;
            Some((Pair::from((a, b)), bond))
        })
        .collect::<HashMap<_, _>>();
    (atoms, bonds)
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
    enabled: bool,
    children: Vec<Box<LayerTree>>,
}

impl LayerTree {
    #[async_recursion]
    pub async fn to_stack(
        &self,
        base: Option<Arc<Stack>>,
    ) -> Result<Vec<Arc<Stack>>, LMECoreError> {
        let layer = Arc::new(Stack::overlay(base, self.config.clone()).await?);
        let mut stacks = if self.enabled {
            vec![layer.clone()]
        } else {
            vec![]
        };
        for child in &self.children {
            let mut sub_layers = child.to_stack(Some(layer.clone())).await?;
            stacks.append(&mut sub_layers);
        }
        Ok(stacks)
    }

    pub fn merge(&mut self, mut reversed_stack: Vec<Layer>) -> Option<Vec<Layer>> {
        let root = reversed_stack
            .last()
            .expect("Each stack input should always contains at least 1 element");
        if root == &self.config {
            reversed_stack.pop();
            if reversed_stack.len() == 0 {
                self.enabled = true;
            } else {
                let mut reversed_stack = Some(reversed_stack);
                for sub_tree in self.children.iter_mut() {
                    if let Some(to_merge) = reversed_stack {
                        reversed_stack = sub_tree.merge(to_merge);
                    }
                }
                if let Some(mut reversed_stack) = reversed_stack {
                    reversed_stack.reverse();
                    self.children
                        .push(Box::new(LayerTree::from(reversed_stack)));
                }
            }
            None
        } else {
            Some(reversed_stack)
        }
    }
}

impl From<Stack> for LayerTree {
    fn from(value: Stack) -> Self {
        Self::from(value.get_layers())
    }
}

impl From<Vec<Layer>> for LayerTree {
    fn from(value: Vec<Layer>) -> Self {
        let mut stack = value;
        let mut layer_tree = Box::new(Self {
            config: stack
                .pop()
                .expect("Each stack input should always contains at least 1 element"),
            enabled: false,
            children: vec![],
        });
        while let Some(config) = stack.pop() {
            layer_tree = Box::new(Self {
                config,
                enabled: false,
                children: vec![layer_tree],
            })
        }
        layer_tree.enabled = true;
        *layer_tree
    }
}

#[test]
fn merge_layer() {
    let ser = serde_json::to_string(&HashMap::from([((1, 2), 3.), ((1, 0), 1.)]));
    println!("{}", ser.unwrap());
}

pub struct Workspace {
    stacks: Vec<Arc<Stack>>,
    id_map: UniqueValueMap<usize, String>,
    class_map: NtoN<usize, String>,
}

impl Workspace {
    pub fn new() -> Self {
        Workspace {
            stacks: vec![Arc::new(Stack::default())],
            id_map: UniqueValueMap::new(),
            class_map: NtoN::new(),
        }
    }

    pub fn get_stack(&self, idx: usize) -> Result<&Arc<Stack>, LMECoreError> {
        if let Some(stack) = self.stacks.get(idx) {
            Ok(stack)
        } else {
            Err(LMECoreError::NoSuchStack)
        }
    }

    fn get_stack_mut(&mut self, idx: usize) -> Result<&mut Arc<Stack>, LMECoreError> {
        if let Some(stack) = self.stacks.get_mut(idx) {
            Ok(stack)
        } else {
            Err(LMECoreError::NoSuchStack)
        }
    }

    pub fn get_stacks(&self) -> Vec<usize> {
        self.stacks
            .par_iter()
            .map(|stack| stack.len())
            .collect::<Vec<_>>()
    }

    pub fn new_empty_stack(&mut self) {
        self.stacks.push(Arc::new(Stack::default()));
    }

    pub fn remove_stack(&mut self, idx: usize) {
        self.stacks.remove(idx);
    }

    pub fn clone_stack(&mut self, idx: usize) -> Result<usize, LMECoreError> {
        let stack = self.get_stack(idx)?;
        self.stacks.push(stack.clone());
        Ok(self.stacks.len() - 1)
    }

    pub fn clone_base(&mut self, idx: usize) -> Result<usize, LMECoreError> {
        let stack = self.get_stack(idx)?;
        if let Some(base) = stack.clone_base() {
            self.stacks.push(base);
            Ok(self.stacks.len() - 1)
        } else {
            Err(LMECoreError::RootLayerError)
        }
    }

    pub async fn overlay_to(&mut self, idx: usize, config: Layer) -> Result<(), LMECoreError> {
        let stack = self.get_stack_mut(idx)?;
        let overlayed = Stack::overlay(Some(stack.clone()), config).await?;
        *stack = Arc::new(overlayed);
        Ok(())
    }

    pub async fn write_to_layer(
        &mut self,
        idx: usize,
        patch: &Molecule,
    ) -> Result<(), LMECoreError> {
        let stack = self.get_stack_mut(idx)?;
        let mut updated = stack.as_ref().clone();
        updated.write(patch).await?;
        *stack = Arc::new(updated);
        Ok(())
    }

    pub fn list_ids(&self) -> HashSet<&String> {
        self.id_map.data().values().collect()
    }

    pub fn id_to_index(&self, target: &String) -> Option<usize> {
        self.id_map
            .data()
            .par_iter()
            .find_map_first(|(idx, id)| if target == id { Some(*idx) } else { None })
    }

    pub fn set_id(&mut self, idx: usize, id: String) -> InsertResult<usize, String> {
        self.id_map.insert(idx, id)
    }

    pub fn remove_id(&mut self, idx: usize) {
        self.id_map.remove(&idx);
    }

    pub fn index_to_id(&self, idx: usize) -> Option<String> {
        self.id_map.data().get(&idx).cloned()
    }

    pub fn set_to_class(&mut self, idx: usize, class: String) {
        self.class_map.insert(idx, class);
    }

    pub fn remove_from_class(&mut self, idx: usize, class: &String) {
        self.class_map.remove(&idx, class);
    }

    pub fn remove_from_all_class(&mut self, idx: usize) {
        self.class_map.remove_left(&idx);
    }

    pub fn remove_class(&mut self, class: &String) {
        self.class_map.remove_right(class);
    }

    pub fn class_indexes(&self, class: &String) -> HashSet<&usize> {
        self.class_map.get_right(class)
    }

    pub fn get_classes(&self, idx: usize) -> HashSet<&String> {
        self.class_map.get_left(&idx)
    }

    pub fn list_classes(&self) -> HashSet<&String> {
        self.class_map.get_rights()
    }

    pub fn export(&self) -> (LayerTree, HashMap<usize, String>, HashSet<(usize, String)>) {
        let mut layer_tree = LayerTree::from(self.stacks[0].as_ref().clone());
        for stack in &self.stacks[1..] {
            layer_tree
                .merge(stack.get_layers())
                .expect("Layers in workspace has same white idx");
        }
        let ids = self.id_map.data().clone();
        let classes = self.class_map.data().clone();
        (layer_tree, ids, classes)
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
            stacks,
            id_map,
            class_map,
        }
    }
}

pub type WorkspaceStore = Arc<RwLock<Workspace>>;

pub fn create_workspace_store() -> WorkspaceStore {
    Arc::new(RwLock::new(Workspace::new()))
}

pub type ServerStore = Arc<RwLock<HashMap<String, WorkspaceStore>>>;

pub fn create_server_store() -> ServerStore {
    Arc::new(RwLock::new(HashMap::new()))
}
