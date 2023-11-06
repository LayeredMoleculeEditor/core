use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use lazy_static::lazy_static;
use nalgebra::{Matrix3, Rotation3, Unit, Vector3};
use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::utils::Pair;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Atom {
    element: usize,
    #[serde(serialize_with = "ser_vec3_f64", deserialize_with = "der_vec3_f64")]
    position: Vector3<f64>,
}

fn ser_vec3_f64<S>(v3: &Vector3<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    v3.as_slice().serialize(serializer)
}

fn der_vec3_f64<'de, D>(deserializer: D) -> Result<Vector3<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    <[f64; 3]>::deserialize(deserializer).map(|value| Vector3::from(value))
}

pub type AtomTable = HashMap<usize, Option<Atom>>;
pub type BondTable = HashMap<Pair<usize>, Option<f64>>;

pub trait Layer {
    fn read(&self, base: &[Arc<dyn Layer>]) -> (AtomTable, BondTable);
    fn uuid(&self) -> &Uuid;
}

pub struct LayerMerger {
    cache: Arc<RwLock<HashMap<Vec<String>, (AtomTable, BondTable)>>>,
}

impl LayerMerger {
    fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::from([(
                vec![],
                (HashMap::new(), HashMap::new()),
            )]))),
        }
    }
    fn merge_base(&self, base: &[Arc<dyn Layer>]) -> (AtomTable, BondTable) {
        let path = base
            .iter()
            .map(|layer| layer.uuid().to_string())
            .collect::<Vec<_>>();
        if let Some(cached) = self
            .cache
            .read()
            .expect("Failed to load cache from RwLock")
            .get(&path)
        {
            cached.clone()
        } else {
            if let Some((last, base)) = base.split_last() {
                let result = last.read(base);
                self.cache
                    .write()
                    .expect("Failed to write to cache in RwLock")
                    .insert(path, result.clone());
                result
            } else {
                (HashMap::new(), HashMap::new())
            }
        }
    }
}

lazy_static! {
    static ref LAYER_MERGER: LayerMerger = LayerMerger::new();
}

pub struct FillLayer {
    atoms: HashMap<usize, Option<Atom>>,
    bonds: HashMap<Pair<usize>, Option<f64>>,
    state_id: Uuid,
}

impl Layer for FillLayer {
    fn read(&self, base: &[Arc<dyn Layer>]) -> (AtomTable, BondTable) {
        let (mut atoms, mut bonds) = LAYER_MERGER.merge_base(base);
        atoms.extend(&self.atoms);
        bonds.extend(&self.bonds);
        (atoms, bonds)
    }

    fn uuid(&self) -> &Uuid {
        &self.state_id
    }
}

impl FillLayer {
    pub fn patch(&mut self, atoms: &AtomTable, bonds: &BondTable) -> &Uuid {
        self.atoms.extend(atoms);
        self.bonds.extend(bonds);
        self.update_uuid()
    }

    pub fn patch_atoms(&mut self, atoms: &AtomTable) -> &Uuid {
        self.atoms.extend(atoms);
        self.update_uuid()
    }

    pub fn patch_bonds(&mut self, bonds: &BondTable) -> &Uuid {
        self.bonds.extend(bonds);
        self.update_uuid()
    }

    fn update_uuid(&mut self) -> &Uuid {
        self.state_id = Uuid::new_v4();
        self.uuid()
    }
}

pub struct RotationLayer {
    center: Vector3<f64>,
    matrix: Matrix3<f64>,
    layer_id: Uuid,
}

impl RotationLayer {
    pub fn new(center: Vector3<f64>, vector: Vector3<f64>, angle: f64) -> Self {
        Self {
            center,
            matrix: Rotation3::from_axis_angle(&Unit::new_normalize(vector), angle)
                .matrix()
                .clone(),
            layer_id: Uuid::new_v4(),
        }
    }
}

impl Layer for RotationLayer {
    fn read(&self, base: &[Arc<dyn Layer>]) -> (AtomTable, BondTable) {
        let (mut atom_table, bond_table) = LAYER_MERGER.merge_base(base);
        let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
            .par_iter()
            .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
            .unzip();
        let rotated = atoms
            .into_par_iter()
            .map(|Atom { element, position }| {
                let vector = Vector3::from(position - self.center).transpose();
                let rotated = vector * self.matrix;
                let position = rotated.transpose() + self.center;
                Some(Atom { element, position })
            })
            .collect::<Vec<_>>();
        atom_table.extend(idxs.into_iter().zip(rotated));
        (atom_table, bond_table)
    }

    fn uuid(&self) -> &Uuid {
        &self.layer_id
    }
}

pub struct TranslateLayer {
    vector: Vector3<f64>,
    layer_id: Uuid,
}

impl TranslateLayer {
    pub fn new(vector: Vector3<f64>) -> Self {
        Self {
            vector,
            layer_id: Uuid::new_v4(),
        }
    }
}

impl Layer for TranslateLayer {
    fn read(&self, base: &[Arc<dyn Layer>]) -> (AtomTable, BondTable) {
        let (mut atom_table, bond_table) = LAYER_MERGER.merge_base(base);
        let (idxs, atoms): (Vec<usize>, Vec<Atom>) = atom_table
            .par_iter()
            .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
            .unzip();
        let translated = atoms
            .into_par_iter()
            .map(|Atom { element, position }| {
                Some(Atom {
                    element,
                    position: position + self.vector,
                })
            })
            .collect::<Vec<_>>();
        atom_table.extend(idxs.into_iter().zip(translated));
        (atom_table, bond_table)
    }

    fn uuid(&self) -> &Uuid {
        &self.layer_id
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExchangeData {
    atoms: Vec<Atom>,
    bonds: HashMap<(usize, usize), f64>,
    maps: Vec<usize>,
    origin_bonds: Vec<Pair<usize>>,
    origin_len: usize,
}

impl From<(AtomTable, BondTable)> for ExchangeData {
    fn from((atom_table, bond_table): (AtomTable, BondTable)) -> Self {
        let origin_len = atom_table.len();
        let origin_bonds: Vec<Pair<usize>> = bond_table.keys().copied().collect();
        let (maps, atoms): (Vec<usize>, Vec<Atom>) = atom_table
            .into_iter()
            .filter_map(|(idx, atom)| atom.and_then(|atom| Some((idx, atom))))
            .unzip();
        let bonds = bond_table
            .into_iter()
            .filter_map(|(pair, bond)| {
                let (a, b) = pair.to_tuple();
                maps.iter()
                    .position(|idx| idx == a)
                    .zip(maps.iter().position(|idx| idx == b))
                    .zip(bond)
            })
            .collect::<HashMap<_, _>>();
        Self {
            atoms,
            bonds,
            maps,
            origin_bonds,
            origin_len,
        }
    }
}

impl Into<(AtomTable, BondTable)> for ExchangeData {
    fn into(self) -> (AtomTable, BondTable) {
        let mut bond_table: BondTable = self
            .origin_bonds
            .into_iter()
            .map(|pair| (pair, None))
            .collect();
        bond_table.extend(self.bonds.into_iter().map(|((a, b), bond)| {
            let a = *self.maps.get(a).expect("Index out of range");
            let b = *self.maps.get(b).expect("Index out of range");
            (Pair::new(a, b), Some(bond))
        }));
        let mut atom_table: AtomTable = (0..=self.origin_len).map(|idx| (idx, None)).collect();
        let update_atoms: AtomTable = self
            .maps
            .into_iter()
            .zip(self.atoms.into_iter().map(|atom| Some(atom)))
            .collect();
        atom_table.extend(update_atoms);
        (atom_table, bond_table)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ExternalProgramLayer {
    program: String,
    arguments: Vec<String>,
    #[serde(skip, default = "Uuid::new_v4")]
    layer_id: Uuid,
}

// impl Layer for ExternalProgramLayer {
//     fn read(&self, base: &[Arc<dyn Layer>]) -> (AtomTable, BondTable) {
//         let current = LAYER_MERGER.merge_base(base);
//         let exchange_data = ExchangeData::from(current);

//     }

//     fn uuid(&self) -> &Uuid {
//         &self.layer_id
//     }
// }
