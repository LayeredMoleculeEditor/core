use std::sync::Arc;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::data_manager::Stack;

pub fn ser_arc_layer<S>(value: &Option<Arc<Stack>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value
        .as_ref()
        .map(|layer| layer.as_ref())
        .serialize(serializer)
}

pub fn de_arc_layer<'de, D>(deserializer: D) -> Result<Option<Arc<Stack>>, D::Error>
where
    D: Deserializer<'de>,
{
    Stack::deserialize(deserializer).map(|layer| Some(Arc::new(layer)))
}
