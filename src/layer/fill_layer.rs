use std::{sync::Arc, collections::HashMap};

use uuid::Uuid;
use lazy_static::lazy_static;

use crate::utils::Pair;

use super::{Atom, AtomTable, BondTable, Layer, LAYER_MERGER};


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

    fn id(&self) -> &Uuid {
        &self.state_id
    }
}

impl FillLayer {
    pub fn new() -> Self {
        Self { atoms: HashMap::new(), bonds: HashMap::new(), state_id: Uuid::new_v4() }
    }
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
        self.id()
    }
}

pub struct TransparentLayer;
pub struct BlankLayer;

lazy_static! {
    static ref BLANK_LAYER_ID: Uuid = Uuid::new_v4();
    pub static ref BLANK_LAYER: BlankLayer = BlankLayer;
}

impl Layer for BlankLayer {
    fn read(&self, _: &[Arc<dyn Layer>]) -> (AtomTable, BondTable) {
        (HashMap::new(), HashMap::new())
    }

    fn id(&self) -> &Uuid {
        &BLANK_LAYER_ID
    }
}
