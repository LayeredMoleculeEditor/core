use std::{
    collections::{HashMap, HashSet},
    io::Write,
    process::{Command, Stdio},
    sync::{Arc, RwLock},
};

use lazy_static::lazy_static;
use nalgebra::{Matrix3, Vector3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    serde::{de_arc_layer, de_m3_64, de_v3_64, ser_arc_layer, ser_m3_64, ser_v3_64},
    utils::{BondGraph, InsertResult, NtoN, UniqueValueMap},
};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Atom {
    element: usize,
    #[serde(serialize_with = "ser_v3_64", deserialize_with = "de_v3_64")]
    position: Vector3<f64>,
}

type AtomTable = HashMap<usize, Option<Atom>>;
pub type Molecule = (AtomTable, BondGraph);

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
    pub fn read(&self, base: &Molecule) -> Result<Molecule, &'static str> {
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
                    .map_err(|_| "Failed to start target program")?;
                let data_to_send = serde_json::to_string(&(&atom_table, &bond_table))
                    .map_err(|_| "Failed to stringify base data")?;
                if let Some(ref mut stdin) = child.stdin {
                    stdin
                        .write_all(&data_to_send.as_bytes())
                        .map_err(|_| "Failed to write to child stdin")?;
                    let output = child
                        .wait_with_output()
                        .map_err(|_| "Failed to get data from child stdout.")?;
                    let data = String::from_utf8_lossy(&output.stdout);
                    let (atoms, bonds): Molecule = serde_json::from_str(&data)
                        .map_err(|_| "Failed to parse data returned from child process")?;
                    atom_table = atoms;
                    bond_table = bonds;
                } else {
                    Err("unable to write to child stdin")?;
                }
            }
        };
        Ok((atom_table, bond_table))
    }

    pub fn write(&mut self, patch: &Molecule) -> Result<(), &'static str> {
        if let Self::Fill { atoms, bonds } = self {
            let (patch_atoms, patch_bonds) = patch;
            atoms.extend(patch_atoms);
            bonds.extend(patch_bonds);
            Ok(())
        } else {
            Err("Not a fill layer.")
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
    pub fn overlay(base: Option<Arc<Self>>, config: Layer) -> Result<Self, &'static str> {
        let cached = if let Some(base) = base.clone() {
            config.read(&base.cached)?
        } else {
            Ok::<Molecule, &'static str>(empty_tables())?
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

    pub fn write(&mut self, patch: &Molecule) -> Result<(), &'static str> {
        self.config.write(patch)?;
        let base = self
            .base
            .as_ref()
            .map(|layer| &layer.cached)
            .unwrap_or(&EMPTY_TABLES);
        self.cached = self.config.read(base)?;
        Ok(())
    }

    pub fn clone_base(&self) -> Option<Self> {
        self.base.as_ref().map(|value| value.as_ref().clone())
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
    children: Vec<(Box<LayerTree>, bool)>,
}

impl LayerTree {
    pub fn to_stack(
        &self,
        base: Option<Arc<Stack>>,
    ) -> Result<(Arc<Stack>, Vec<Arc<Stack>>), &'static str> {
        let layer = Arc::new(Stack::overlay(base, self.config.clone())?);
        let mut children = vec![];
        for (child, enabled) in &self.children {
            let (current, mut sub_layers) = child.to_stack(Some(layer.clone()))?;
            children.append(&mut sub_layers);
            if *enabled {
                children.push(current);
            }
        }
        Ok((layer, children))
    }

    pub fn merge(&mut self, mut stack: Vec<Layer>) -> Result<bool, Vec<Layer>> {
        stack.reverse();
        let current = stack
            .last()
            .expect("should never put empty vec in to this function");
        if current == &self.config {
            stack.pop();
            if stack.len() == 0 {
                Ok(true)
            } else {
                let mut stack_in_loop = Some(stack);
                for (sub_tree, enabled) in self.children.iter_mut() {
                    if let Some(stack) = stack_in_loop {
                        match sub_tree.merge(stack) {
                            Ok(result) => {
                                *enabled = *enabled || result;
                                stack_in_loop = None;
                            }
                            Err(give_back) => {
                                stack_in_loop = Some(give_back);
                            }
                        }
                    }
                }
                if let Some(stack) = stack_in_loop {
                    let layer_tree = Self::from(stack);
                    let enabled = layer_tree.children.len() == 0;
                    self.children.push((Box::new(layer_tree), enabled))
                };
                Ok(false)
            }
        } else {
            Err(stack)
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
        let mut layer = (
            Box::new(Self {
                config: stack
                    .pop()
                    .expect("Each stack created should always contains at least 1 element"),
                children: vec![],
            }),
            true,
        );
        while let Some(config) = stack.pop() {
            layer = (
                Box::new(LayerTree {
                    config,
                    children: vec![layer],
                }),
                false,
            );
        }
        *layer.0
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

    pub fn get_stack(&self, idx: usize) -> Option<&Arc<Stack>> {
        self.stacks.get(idx)
    }

    fn get_stack_mut(&mut self, idx: usize) -> Option<&mut Arc<Stack>> {
        self.stacks.get_mut(idx)
    }

    pub fn get_stacks(&self) -> Vec<usize> {
        self.stacks.par_iter().map(|stack| stack.len()).collect::<Vec<_>>()
    }

    pub fn new_empty_stack(&mut self) {
        self.stacks.push(Arc::new(Stack::default()));
    }

    pub fn overlay_to(&mut self, idx: usize, config: Layer) -> Result<(), WorkspaceError> {
        if let Some(current) = self.get_stack_mut(idx) {
            match Stack::overlay(Some(current.clone()), config) {
                Ok(overlayed) => {
                    *current = Arc::new(overlayed);
                    Ok(())
                }
                Err(err) => Err(WorkspaceError::PluginError(err.to_string())),
            }
        } else {
            Err(WorkspaceError::NoSuchStack)
        }
    }

    pub fn write_to_layer(&mut self, idx: usize, patch: &Molecule) -> Result<(), WorkspaceError> {
        if let Some(current) = self.get_stack_mut(idx) {
            let mut updated = current.as_ref().clone();
            match updated.write(patch) {
                Ok(_) => {
                    *current = Arc::new(updated);
                    Ok(())
                }
                Err(err) => Err(WorkspaceError::NotFillLayer),
            }
        } else {
            Err(WorkspaceError::NoSuchStack)
        }
    }

    pub fn id_of(&self, target: &String) -> Option<usize> {
        self.id_map
            .data()
            .iter()
            .find_map(|(idx, id)| if target == id { Some(*idx) } else { None })
    }

    pub fn set_id(&mut self, idx: usize, id: String) -> InsertResult<usize, String> {
        self.id_map.insert(idx, id)
    }

    pub fn remove_id(&mut self, idx: usize) {
        self.id_map.remove(&idx);
    }

    pub fn get_id(&self, idx: usize) -> Option<String> {
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

    pub fn get_class(&self, class: &String) -> Vec<&usize> {
        self.class_map.get_right(class)
    }

    pub fn classes_of(&self, idx: usize) -> Vec<&String> {
        self.class_map.get_left(&idx)
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

impl From<(Vec<Arc<Stack>>, UniqueValueMap<usize, String>, NtoN<usize, String>)> for Workspace {
    fn from(value: (Vec<Arc<Stack>>, UniqueValueMap<usize, String>, NtoN<usize, String>)) -> Self {
        let (stacks, id_map, class_map) = value;
        Self {
            stacks, id_map, class_map
        }
    }
}

pub enum WorkspaceError {
    InvalidDataToLoad,
    NoSuchStack,
    NotFillLayer,
    PluginError(String),
}

pub type ServerStore = Arc<RwLock<Workspace>>;

pub fn create_server_store() -> ServerStore {
    Arc::new(RwLock::new(Workspace::new()))
}
