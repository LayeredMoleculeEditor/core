use std::net::SocketAddr;

use axum::{
    middleware,
    routing::{delete, get, patch, post, put},
    Router,
};
use clap::Parser;
use data_manager::create_server_store;

use handler::{namespace::*, server::*, stack::*, workspace::*};

mod data_manager;
mod error;
mod handler;
mod serde;
mod utils;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() {
    let Args { listen } = Args::parse();

    let store = create_server_store();

    let namespace_rt = Router::new()
        .route("/id/:name/stack/:stack_id", get(id_to_index))
        .route("/class/:name/stack/:stack_id", get(class_indexes))
        .route_layer(middleware::from_fn(stack_middleware))
        .route("/id", get(list_ids))
        .route("/id", post(set_id))
        .route("/id/atom/:atom_idx", delete(remove_id))
        .route("/id/atom/:atom_idx", get(index_to_id))
        .route("/class", get(list_classes))
        .route("/class", post(set_to_class))
        .route("/class/:name/atom/:atom_idx", delete(remove_from_class))
        .route("/class/atom/:atom_idx", get(get_classes))
        .route("/class/atom/:atom_idx", delete(remove_from_all_class))
        .route("/class/:name", delete(remove_class));

    let stack_rt = Router::new()
        .route("/", get(read_stack))
        .route("/", patch(write_to_layer))
        .route("/", delete(remove_stack))
        .route("/writable", get(is_writable))
        .route("/cleaned", get(read_cleaned))
        .route("/clone_stack", post(clone_stack))
        .route("/clone_base", post(clone_base))
        .route("/rotation/class/:name", put(rotation_atoms))
        .route("/translation/class/:name", put(translation_atoms))
        .route("/atom/:atom_idx/neighbor", get(get_neighbors))
        .route("/import/:name", post(import_structure))
        .route("/substitute", post(add_substitute))
        .route_layer(middleware::from_fn(stack_middleware));

    let workspace_rt = Router::new()
        .route("/export", get(export_workspace))
        .route("/stacks", get(read_stacks))
        .route("/stacks", post(new_stack))
        .route("/stacks/overlay_to", put(overlay_to))
        .nest("/stacks/:stack_id", stack_rt)
        .nest("/namespace", namespace_rt)
        .layer(middleware::from_fn_with_state(
            store.clone(),
            workspace_middleware,
        ))
        .route("/", post(create_workspace))
        .route("/", delete(remove_workspace));

    let router = Router::new()
        .nest("/ws/:ws", workspace_rt)
        .with_state(store);

    axum::Server::bind(&listen)
        .serve(router.into_make_service())
        .await
        .unwrap()
}
