use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use axum::{routing::get, Router, extract::{State, Path}};
use layer::Layer;

mod layer;
pub mod utils;

type ProjectStore =Arc<RwLock<HashMap<String, Arc<RwLock<Vec<Box<dyn Layer>>>>>>>;

#[tokio::main]
async fn main() {
    let projects: ProjectStore  =
        Arc::new(RwLock::new(HashMap::new()));

    let app = Router::new().route("/", get(hello_world));

    axum::Server::bind(&"127.0.0.1:35182".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn hello_world() -> &'static str {
    "hello, world"
}

async fn create_project(State(projects): State<ProjectStore>, Path(project_name): Path<String>) {
    if projects.read().unwrap().contains_key(&project_name) {
        
    } else {
        projects.write().unwrap().insert(project_name, Arc::new(RwLock::new(Vec::new())));
    }
}
