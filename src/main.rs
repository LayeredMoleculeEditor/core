use layer::{LayerConfig, Molecule};
use many_to_many::ManyToMany;
use utils::UniqueValueMap;

mod layer;
pub mod serde;
mod utils;

struct Project {
    lower: (LayerConfig, Option<Molecule>),
    upper: Vec<Vec<(LayerConfig, Option<Molecule>)>>,
    id_map: UniqueValueMap<usize, String>,
    class_map: ManyToMany<usize, String>,
}

#[tokio::main]
async fn main() {}
