use axum::{response::IntoResponse, http::StatusCode, Json};
use serde::Serialize;

#[derive(Serialize)]
pub enum LMECoreError {
    NoSuchAtom,
    NotFillLayer,
    PluginLayerError(isize, String),
    NoSuchStack,
}

impl IntoResponse for LMECoreError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::NoSuchAtom | Self::NoSuchStack => (StatusCode::NOT_FOUND, self).into_response(),
            Self::NotFillLayer => (StatusCode::NOT_ACCEPTABLE, self).into_response(),
            Self::PluginLayerError(error_code, error_info) => (StatusCode::INTERNAL_SERVER_ERROR, Json((error_code, error_info))).into_response()
        }
    }
}
