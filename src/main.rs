use std::net::SocketAddr;

use axum::{
    middleware,
    routing::{delete, get, post, patch, put},Router,
};
use clap::Parser;
use data_manager::create_server_store;

use handler::{server::*, workspace::*, stack::*, namespace::*};

mod data_manager;
mod handler;
mod serde;
mod utils;
mod error;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() {
    let Args {
        listen
    } = Args::parse();

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
        .route("/", put(overlay_to))
        .route("/", delete(remove_stack))
        .route("/base", post(clone_base))
        .route_layer(middleware::from_fn(stack_middleware));

    let workspace_rt = Router::new()
        .route("/export", get(export_workspace))
        .route("/stacks", get(read_stacks))
        .route("/stacks", post(new_stack))
        .nest("/stacks/:stack_id", stack_rt)
        .nest("/namespace", namespace_rt)
        .layer(middleware::from_fn_with_state(
            store.clone(),
            workspace_middleware,
        ))
        .route("/", post(create_workspace))
        .route("/", delete(remove_workspace));

    let router = Router::new()
        .nest("/workspaces/:ws", workspace_rt)
        .with_state(store);

    axum::Server::bind(&listen)
        .serve(router.into_make_service())
        .await
        .unwrap()
}
