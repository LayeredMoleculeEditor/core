use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    middleware,
    routing::{delete, post, put},
    Router,
};
use clap::Parser;
use handler::*;
use lme_core::Workspace;
use tokio::sync::{Mutex, RwLock};
mod error;
mod handler;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    listen: SocketAddr,
}

pub type WorkspaceAccessor = Arc<Mutex<Workspace>>;
pub type ServerState = Arc<RwLock<HashMap<String, WorkspaceAccessor>>>;

#[tokio::main]
async fn main() {
    let Args { listen } = Args::parse();

    let state: ServerState = Arc::new(RwLock::new(HashMap::new()));

    let ws_router = Router::new()
        .route("/stack/clone_stack", post(clone_stack))
        .route("/stack/clone_base", post(clone_base))
        .route("/stack/layer", put(add_layer_to_stack))
        .route("/stack/write", put(write_to_stack))
        .route("/stack", post(create_stack))
        .route("/export", post(workspace_export))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            workspace_middleware,
        ));

    let router = Router::new()
        .nest("/ws/:ws", ws_router)
        .route("/ws/:ws", delete(remove_workspace))
        .route("/ws/:ws", post(create_workspace))
        .with_state(state);

    axum::Server::bind(&listen)
        .serve(router.into_make_service())
        .await
        .unwrap()
}
