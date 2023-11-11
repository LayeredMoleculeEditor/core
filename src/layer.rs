use std::{
    collections::HashMap,
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
};

use lazy_static::lazy_static;
use nalgebra::{Matrix3, Vector3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::serde::{de_arc_layer, de_m3_64, de_v3_64, ser_arc_layer, ser_m3_64, ser_v3_64};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Atom {
    element: usize,
    #[serde(serialize_with = "ser_v3_64", deserialize_with = "de_v3_64")]
    position: Vector3<f64>,
}

type AtomTable = HashMap<usize, Option<Atom>>;
type BondTable = HashMap<(usize, usize), Option<f64>>;
pub type Molecule = (AtomTable, BondTable);

pub fn empty_tables() -> Molecule {
    (HashMap::new(), HashMap::new())
}

lazy_static! {
    static ref EMPTY_TABLES: Molecule = empty_tables();
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum LayerConfig {
    Transparent,
    Fill {
        #[serde(default)]
        atoms: AtomTable,
        #[serde(default)]
        bonds: BondTable,
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

impl LayerConfig {
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
pub struct Layer {
    config: LayerConfig,
    #[serde(serialize_with = "ser_arc_layer", deserialize_with = "de_arc_layer")]
    base: Option<Arc<Layer>>,
    cached: Molecule,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            config: LayerConfig::Transparent,
            base: None,
            cached: empty_tables(),
        }
    }
}

impl Layer {
    pub fn overlay(base: Option<Arc<Self>>, config: LayerConfig) -> Result<Self, &'static str> {
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

    pub fn get_deep_config(&self, layer: usize) -> Result<LayerConfig, &'static str> {
        if layer >= self.len() {
            Err("Layer number out of layers")
        } else if layer == self.len() - 1 {
            Ok(self.config.clone())
        } else {
            self.base
                .as_ref()
                .expect("should never found None base in condition")
                .get_deep_config(layer - 1)
        }
    }

    pub fn get_config_stack(&self) -> Vec<LayerConfig> {
        (0..self.len())
            .rev()
            .map(|layer| self.get_deep_config(layer))
            .collect::<Result<Vec<_>, _>>()
            .expect("should never hint this condition")
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LayerTree {
    config: LayerConfig,
    children: Vec<(Box<LayerTree>, bool)>,
}

impl LayerTree {
    pub fn to_layers(&self, base: Option<Arc<Layer>>) -> Result<(Arc<Layer>, Vec<Arc<Layer>>), &'static str> {
        let layer = Arc::new(Layer::overlay(base, self.config.clone())?);
        let mut children = vec![];
        for (child, enabled) in &self.children {
            let (current, mut sub_layers) = child.to_layers(Some(layer.clone()))?;
            children.append(&mut sub_layers);
            if *enabled {
                children.push(current);
            }
        };
        Ok((layer, children))
    }

    pub fn merge(&mut self, stack: Vec<LayerConfig>) -> Result<bool, Vec<LayerConfig>> {
        let current = stack
            .last()
            .expect("should never put empty vec in to this function");
        if current == &self.config {
            let mut stack = stack;
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

impl From<Layer> for LayerTree {
    fn from(value: Layer) -> Self {
        Self::from(value.get_config_stack())
    }
}

impl From<Vec<LayerConfig>> for LayerTree {
    fn from(value: Vec<LayerConfig>) -> Self {
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
