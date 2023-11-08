pub mod filter_layer;

use std::collections::HashMap;
use nalgebra::Vector3;
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

pub enum Layer {
    FillLayer {
        atoms: AtomTable,
        bonds: BondTable,
        layer_id: Uuid,
    },
    FilterLayer {
        transformer: Box<dyn Sync + Fn((AtomTable, BondTable)) -> (AtomTable, BondTable)>,
        layer_id: Uuid,
    },
}

impl Layer {
    pub fn new_fill_layer() -> Self {
        Self::FillLayer {
            atoms: HashMap::new(),
            bonds: HashMap::new(),
            layer_id: Uuid::new_v4(),
        }
    }

    pub fn new_filter_layer(transformer: Box<dyn Sync + Fn((AtomTable, BondTable)) -> (AtomTable, BondTable)>) -> Self
    {
        Self::FilterLayer {
            transformer,
            layer_id: Uuid::new_v4(),
        }
    }

    pub fn patch_atoms(&mut self, patch: &AtomTable) -> Result<&Uuid, LayerError> {
        match self {
            Self::FilterLayer { .. } => Err(LayerError::NotFillLayer),
            Self::FillLayer {
                atoms, layer_id, ..
            } => {
                atoms.extend(patch);
                *layer_id = Uuid::new_v4();
                Ok(self.id())
            }
        }
    }

    pub fn patch_bonds(&mut self, patch: &BondTable) -> Result<&Uuid, LayerError> {
        match self {
            Self::FilterLayer { .. } => Err(LayerError::NotFillLayer),
            Self::FillLayer {
                bonds, layer_id, ..
            } => {
                bonds.extend(patch);
                *layer_id = Uuid::new_v4();
                Ok(self.id())
            }
        }
    }

    pub fn patch(&mut self, patch: (&AtomTable, &BondTable)) -> Result<&Uuid, LayerError> {
        match self {
            Self::FilterLayer { .. } => Err(LayerError::NotFillLayer),
            Self::FillLayer {
                atoms,
                bonds,
                layer_id,
            } => {
                atoms.extend(patch.0);
                bonds.extend(patch.1);
                *layer_id = Uuid::new_v4();
                Ok(self.id())
            }
        }
    }

    fn read_base(&self, base: (AtomTable, BondTable)) -> (AtomTable, BondTable) {
        match self {
            Self::FillLayer { atoms, bonds, .. } => {
                let (mut atom_table, mut bond_table) = base;
                atom_table.extend(atoms);
                bond_table.extend(bonds);
                (atom_table, bond_table)
            }
            Self::FilterLayer { transformer, .. } => transformer(base),
        }
    }

    pub fn read(
        &self,
        base: &[Self],
        cache: Option<&mut HashMap<Vec<Uuid>, (AtomTable, BondTable)>>,
    ) -> (AtomTable, BondTable) {
        if let Some(cache) = cache {
            let base_path = base
                .iter()
                .map(|item| item.id())
                .cloned()
                .collect::<Vec<Uuid>>();
            let full_path = [base_path.clone(), vec![self.id().clone()]].concat();
            if let Some(cached) = cache.get(&full_path) {
                cached.clone()
            } else if let Some(cached) = cache.get(&base_path) {
                let composed = self.read_base(cached.clone());
                cache.insert(full_path, composed.clone());
                composed
            } else {
                let base = if let Some((last, base)) = base.split_last() {
                    last.read(base, Some(cache))
                } else {
                    (HashMap::new(), HashMap::new())
                };
                let composed = self.read_base(base);
                cache.insert(full_path, composed.clone());
                composed
            }
        } else if let Some((last, base)) = base.split_last() {
            let base = last.read(base, None);
            self.read_base(base)
        } else {
            self.read_base((HashMap::new(), HashMap::new()))
        }
    }

    pub fn id(&self) -> &Uuid {
        match self {
            Self::FillLayer { layer_id, .. } => &layer_id,
            Self::FilterLayer { layer_id, .. } => &layer_id,
        }
    }
}

pub enum LayerError {
    NotFillLayer,
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
