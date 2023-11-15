

use axum::{
    middleware,
    routing::{delete, get, post},Router,
};
use data_manager::create_server_store;

use handler::{server::*, workspace::*, stack::{stack_middleware, read_stack}};

mod data_manager;
mod handler;
mod serde;
mod utils;

#[tokio::main]
async fn main() {
    let store = create_server_store();

    let stack_rt = Router::new()
        .route("/", get(read_stack))
        .route_layer(middleware::from_fn(stack_middleware));

    let workspace_rt = Router::new()
        .route("/export", get(export_workspace))
        .route("/stacks", get(read_stacks))
        .route("/stack", post(new_stack))
        .nest("/stack/:stack_id", stack_rt)
        .layer(middleware::from_fn_with_state(
            store.clone(),
            workspace_middleware,
        ))
        .route("/", post(create_workspace))
        .route("/", delete(remove_workspace));

    let router = Router::new()
        .nest("/workspaces/:ws", workspace_rt)
        // .route("/stacks/:idx", get(read_stack))
        // .route("/stacks/:idx", patch(write_to_layer))
        // .route("/stacks/:idx", put(overlay_to))
        // .route("/atoms/:idx/id/:id", post(set_id))
        // .route("/atoms/:idx/class/:class", post(set_to_class))
        // .route("/atoms/:idx/class/:class", delete(remove_from_class))
        // .route("/atoms/:idx/class", delete(remove_from_all_class))
        // .route("/ids/:idx", delete(remove_id))
        // .route("/classes/:class", delete(remove_class))
        .with_state(store);

    axum::Server::bind(&"127.0.0.1:10810".parse().unwrap())
        .serve(router.into_make_service())
        .await
        .unwrap()
}

// async fn get_stacks(State(store): State<WorkspaceStore>) -> Json<Vec<usize>> {
//     Json(
//         store.read().unwrap().get_stacks()
//     )
// }

// async fn new_empty_stack(State(store): State<WorkspaceStore>) -> StatusCode {
//     store.write().unwrap().new_empty_stack();
//     StatusCode::OK
// }

// async fn overlay_to(
//     State(store): State<WorkspaceStore>,
//     Path(idx): Path<usize>,
//     Json(config): Json<Layer>,
// ) -> StatusCode {
//     match store.write().unwrap().overlay_to(idx, config) {
//         Ok(_) => StatusCode::OK,
//         Err(err) => match err {
//             WorkspaceError::NoSuchStack => StatusCode::NOT_FOUND,
//             _ => StatusCode::INTERNAL_SERVER_ERROR
//         }
//     }
// }

// async fn write_to_layer(
//     State(store): State<WorkspaceStore>,
//     Path(idx): Path<usize>,
//     Json(patch): Json<Molecule>,
// ) -> StatusCode {
//     match store.write().unwrap().write_to_layer(idx, &patch) {
//         Ok(_) => StatusCode::OK,
//         Err(err) => match err {
//             WorkspaceError::NotFillLayer => StatusCode::BAD_REQUEST,
//             WorkspaceError::NoSuchStack => StatusCode::NOT_FOUND,
//             _ => StatusCode::INTERNAL_SERVER_ERROR
//         }

//     }
// }

// async fn set_id(
//     State(store): State<WorkspaceStore>,
//     Path(idx): Path<usize>,
//     Path(id): Path<String>,
// ) -> (StatusCode, Json<Option<usize>>) {
//     if let InsertResult::Duplicated(duplicated_with) = store.write().unwrap().set_id(idx, id)
//     {
//         (StatusCode::BAD_REQUEST, Json(Some(duplicated_with)))
//     } else {
//         (StatusCode::OK, Json(None))
//     }
// }

// async fn set_to_class(
//     State(store): State<WorkspaceStore>,
//     Path(idx): Path<usize>,
//     Path(class): Path<String>,
// ) -> StatusCode {
//     store.write().unwrap().set_to_class(idx, class);
//     StatusCode::OK
// }

// async fn remove_id(State(store): State<WorkspaceStore>, Path(idx): Path<usize>) -> StatusCode {
//     store.write().unwrap().remove_id(idx);
//     StatusCode::OK
// }

// async fn remove_from_class(
//     State(store): State<WorkspaceStore>,
//     Path(idx): Path<usize>,
//     Path(class): Path<String>,
// ) -> StatusCode {
//     store.write().unwrap().remove_from_class(idx, &class);
//     StatusCode::OK
// }

// async fn remove_from_all_class(
//     State(store): State<WorkspaceStore>,
//     Path(idx): Path<usize>,
// ) -> StatusCode {
//     store.write().unwrap().remove_from_all_class(idx);
//     StatusCode::OK
// }

// async fn remove_class(State(store): State<WorkspaceStore>, Path(class): Path<String>) -> StatusCode {
//     store.write().unwrap().remove_class(&class);
//     StatusCode::OK
// }

// async fn export_workspace(
//     State(store): State<WorkspaceStore>,
// ) -> Json<(LayerTree, HashMap<usize, String>, HashSet<(usize, String)>)> {
//     Json(store.read().unwrap().export())
// }

// async fn load_workspace(
//     State(store): State<WorkspaceStore>,
//     Json((layer_tree, ids, classes)): Json<(
//         LayerTree,
//         HashMap<usize, String>,
//         HashSet<(usize, String)>,
//     )>,
// ) -> StatusCode {
//     if let Ok((root, mut others)) = layer_tree.to_stack(None) {
//         let mut stacks = vec![root];
//         stacks.append(&mut others);
//         if let Ok(id_map) = UniqueValueMap::from_map(ids) {
//             let class_map = NtoN::from(classes);
//             *store.write().unwrap() = Workspace::from((stacks, id_map, class_map));
//             StatusCode::OK
//         } else {
//             StatusCode::BAD_REQUEST
//         }
//     } else {
//         StatusCode::INTERNAL_SERVER_ERROR
//     }
// }

// async fn read_stack(
//     State(store): State<WorkspaceStore>,
//     Path(idx): Path<usize>,
// ) -> (StatusCode, Json<Option<Molecule>>) {
//     if let Some(stack) = store.read().unwrap().get_stack(idx) {
//         (StatusCode::OK, Json(Some(stack.read().clone())))
//     } else {
//         (StatusCode::NOT_FOUND, Json(None))
//     }
// }
