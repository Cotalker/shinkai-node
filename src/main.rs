// main.rs
mod agent;
mod cron_tasks;
mod crypto_identities;
mod db;
mod managers;
mod network;
mod payments;
mod planner;
mod schemas;
mod tools;
mod utils;
mod vector_fs;
mod runner;

use runner::{initialize_node, run_node_tasks};

#[tokio::main]
pub async fn main() {
    let result = initialize_node().await.unwrap();
    let _ = run_node_tasks(result.1, result.2, result.3).await;
}
