use serde::Serialize;

#[derive(Serialize)]
pub enum LMECoreError {
    NotFillLayer,
    PluginLayerError(isize, String),
    NoSuchStack,
}
