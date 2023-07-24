#[macro_use]
extern crate dlopen_derive;
use std::path::Path;

use crate::plugins::Plugins;

pub mod plugins;
pub mod config;

pub use plugins::{PluginApi, NodeLink, NodeLinkClient};
pub use config::{WORD_COUNT, HD_PATH, LOCALNET};

pub fn run_query_accounts(link_name: &str, addresses: Vec<String>) -> Result<Vec<String>, String>{
    let mut client = Plugins::new(Path::new("plugins").into()).create_client(&link_name);
    return client.query_accounts(addresses).
        map_err(|e| format!("Error query accounts: {}", e));
}