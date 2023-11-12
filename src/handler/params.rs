use serde::Deserialize;

#[derive(Deserialize)]
pub struct AtomPathParam {
    pub atom_idx: usize,
}

#[derive(Deserialize)]
pub struct NamePathParam {
    pub name: String,
}

#[derive(Deserialize)]
pub struct AtomNamePathParam {
    pub atom_idx: usize,
    pub name: String,
}

#[derive(Deserialize)]
pub struct StackNamePathParam {
    pub stack_id: usize,
    pub name: String,
}
