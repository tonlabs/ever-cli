/*
 * Copyright 2018-2021 TON DEV SOLUTIONS LTD.
 *
 * Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
 * this file except in compliance with the License.
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific TON DEV software governing permissions and
 * limitations under the License.
 */

extern crate core;

use std::collections::BTreeMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use ton_client::abi::AbiContract;

#[derive(Serialize, Deserialize, Clone)]
pub struct ContractData {
    pub abi_path: Option<String>,
    pub address: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FullConfig {
    pub config: Config,
    endpoints_map: BTreeMap<String, Vec<String>>,
    pub aliases: BTreeMap<String, ContractData>,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub url: String,
    pub wc: i32,
    pub addr: Option<String>,
    pub method: Option<String>,
    pub parameters: Option<String>,
    pub wallet: Option<String>,
    pub pubkey: Option<String>,
    pub abi_path: Option<String>,
    pub keys_path: Option<String>,
    pub retries: u8,
    pub timeout: u32,
    pub message_processing_timeout: u32,
    pub out_of_sync_threshold: u32,
    pub is_json: bool,
    pub depool_fee: f32,
    pub lifetime: u32,
    pub no_answer: bool,
    pub balance_in_tons: bool,
    pub local_run: bool,
    pub async_call: bool,
    pub debug_fail: String,
    pub endpoints: Vec<String>,
}

const CONFIG_BASE_NAME: &str = "tonos-cli.conf.json";

fn print_paths(prefix: &str) {
    let folder = if !prefix.contains('/') {
        "./"
    } else {
        prefix.trim_end_matches(|c| c != '/')
    };
    let paths = std::fs::read_dir(folder);
    if paths.is_err() {
        return;
    }
    let mut saved_path: Vec<PathBuf> = vec![];
    for path in paths.unwrap().flatten() {
        let path = path.path();
        let path_str = path.to_str().unwrap();
        if path_str.starts_with(prefix) {
            saved_path.push(path);
        }
    }
    if saved_path.len() == 1 && saved_path[0].is_dir() {
        let paths = std::fs::read_dir(saved_path[0].to_str().unwrap());
        for path in paths.unwrap() {
            println!("{}", path.unwrap().path().to_str().unwrap());
        }
    } else {
        for path in saved_path {
            println!("{}{}", path.to_str().unwrap(), if path.is_dir() {"/"} else {""});
        }
    }
}

fn main() {
    let args : Vec<String> = std::env::args().collect();
    let word_being_completed = &args[2];
    let prev_word = args.last().unwrap();

    let cmd_line = std::env::var("COMP_LINE");
    if cmd_line.is_err() {
        print_paths(word_being_completed);
        return;
    }
    let words = cmd_line.unwrap().split(' ').map(|s| s.to_string())
        .collect::<Vec<String>>();
    let mut options_map = BTreeMap::new();
    for (index, word) in words.iter().enumerate() {
        if word.starts_with('-') && index != words.len() - 1 {
            options_map.insert(word.as_str(), words[index + 1].as_str());
        }
    }
    let config_path = options_map.get(&"-c")
        .or(options_map.get(&"--config")
            .or(Some(&CONFIG_BASE_NAME))).unwrap();
    let conf_str = std::fs::read_to_string(config_path).ok().unwrap_or_default();
    let config: serde_json::Result<FullConfig> = serde_json::from_str(&conf_str);
    if config.is_err() {
        print_paths(word_being_completed);
        return;
    }
    let config = config.unwrap();
    let aliases = config.aliases;
    if prev_word == "--addr" {
        if word_being_completed.is_empty() {
            for alias in aliases.keys() {
                println!("{}", alias);
            }
        } else {
            for alias in aliases {
                if alias.0.starts_with(word_being_completed) {
                    println!("{}", alias.0);
                }
            }
        }
        return;
    }
    if prev_word == "-m" || prev_word == "--method" {
        let abi_path = match options_map.get(&"--abi") {
            Some(path) => Some(path.to_string()),
            None => {
                if (options_map.contains_key("--addr")) && aliases.contains_key(&options_map.get("--addr").unwrap().to_string()) {
                    aliases.get(&options_map.get("--addr").unwrap().to_string()).unwrap().clone().abi_path
                } else {
                    None
                }
            }
        }.or(config.config.abi_path);
        if abi_path.is_none() {
            return;
        }
        if let Ok(abi) = std::fs::read_to_string(&abi_path.unwrap()) {
            if let Ok(abi_contract) = serde_json::from_str::<AbiContract>(&abi) {
                for function in abi_contract.functions {
                    if function.name.starts_with(word_being_completed) {
                        println!("{}", function.name);
                    }
                }
            }
        }
        return;
    }
    print_paths(word_being_completed);
}