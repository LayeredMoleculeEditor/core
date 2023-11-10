use std::{collections::HashMap, sync::Arc};

use lazy_static::lazy_static;
use nalgebra::{Matrix3, Vector3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::serde::{de_m3_64, de_v3_64, ser_m3_64, ser_v3_64, ser_arc_layer, de_arc_layer};

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
        atoms: AtomTable,
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
    pub fn new_fill() -> Self {
        let (atoms, bonds) = empty_tables();
        Self::Fill { atoms, bonds }
    }

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
                todo!()
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
    pub fn overlay(base: Arc<Self>, config: LayerConfig) -> Result<Self, &'static str> {
        let cached = config.read(&base.cached)?;
        Ok(Self {
            config,
            base: Some(base.clone()),
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
}
