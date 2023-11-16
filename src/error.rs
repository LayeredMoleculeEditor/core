use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Serialize)]
pub enum LMECoreError {
    IdMapUniqueError,
    NoSuchAtom,
    NoSuchId,
    RootLayerError,
    NotFillLayer,
    PluginLayerError(isize, String),
    NoSuchStack,
    WorkspaceNameConflict,
    WorkspaceNotFound,
}

impl IntoResponse for LMECoreError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::IdMapUniqueError => (StatusCode::BAD_REQUEST, Json(self)).into_response(),
            Self::NoSuchAtom | Self::NoSuchStack | Self::WorkspaceNotFound | Self::NoSuchId => {
                (StatusCode::NOT_FOUND, Json(self)).into_response()
            }
            Self::WorkspaceNameConflict | Self::RootLayerError | Self::NotFillLayer => {
                (StatusCode::NOT_ACCEPTABLE, Json(self)).into_response()
            }
            Self::PluginLayerError(_, _) => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
            }
        }
    }
}

#[test]
pub fn test() {
    println!("{:#?}", LMECoreError::IdMapUniqueError.into_response())
}
