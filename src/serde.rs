use std::sync::Arc;

use nalgebra::{Matrix3, Vector3};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::data_manager::Stack;

pub fn ser_v3_64<S>(value: &Vector3<f64>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value.as_slice().serialize(s)
}

pub fn de_v3_64<'de, D>(de: D) -> Result<Vector3<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    <[f64; 3]>::deserialize(de).and_then(|value| Ok(Vector3::from(value)))
}

pub fn ser_m3_64<S>(value: &Matrix3<f64>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value.as_slice().serialize(s)
}

pub fn de_m3_64<'de, D>(de: D) -> Result<Matrix3<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    <[f64; 9]>::deserialize(de).and_then(|value| Ok(Matrix3::from_iterator(value)))
}

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
