extern crate tonos_cli;

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::runtime::Runtime;
use regex::Regex;
use std::collections::BTreeMap;
use lazy_static::lazy_static;

use tonos_cli::{NodeLink, NodeLinkClient, WORD_COUNT, HD_PATH};
use ton_client::{ClientConfig, ClientContext, net::{query_collection, ParamsOfQueryCollection, NetworkConfig},
    crypto::{CryptoConfig, MnemonicDictionary}, abi::{AbiConfig}};

const TESTNET: &str = "net.evercloud.dev";
const MAINNET: &str = "main.evercloud.dev";
const LOCALNET: &str = "http://127.0.0.1/";

lazy_static! {
    static ref MAIN_ENDPOINTS: Vec<String> = vec![
        "https://mainnet.evercloud.dev".to_string()
    ];

    static ref NET_ENDPOINTS: Vec<String> = vec![
        "https://devnet.evercloud.dev".to_string()
    ];

    static ref SE_ENDPOINTS: Vec<String> = vec![
        "http://0.0.0.0".to_string(),
        "http://127.0.0.1".to_string(),
        "http://localhost".to_string(),
    ];
}

const ACCOUNT_FIELDS: &str = r#"
    id
    acc_type_name
    balance(format: DEC)
    last_paid
    last_trans_lt
    data
    boc
    code_hash
"#;

struct FrontNodeLink;
struct FrontNodeLinkClient {
    ton: Arc<ClientContext>,
    rt: Runtime
}

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        match record.level() {
            log::Level::Error | log::Level::Warn => {
                eprintln!("{}", record.args());
            }
            _ => {
                println!("{}", record.args());
            }
        }
    }

    fn flush(&self) {}
}

const TEST_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
const MAX_LEVEL: log::LevelFilter = log::LevelFilter::Warn;

fn debug_level_from_env() -> log::LevelFilter {
    if let Ok(debug_level) = std::env::var("RUST_LOG") {
        if debug_level.eq_ignore_ascii_case("debug") {
            return log::LevelFilter::Debug;
        } else if debug_level.eq_ignore_ascii_case("trace") {
            return log::LevelFilter::Trace;
        }
        return TEST_MAX_LEVEL;
    }
    return MAX_LEVEL;
}

fn default_url() -> String {
    TESTNET.to_string()
}

fn default_wc() -> i32 {
    0
}

fn default_retries() -> u8 {
    5
}

fn default_timeout() -> u32 {
    40000
}

fn default_out_of_sync() -> u32 { 15 }

fn default_false() -> bool {
    false
}

fn default_lifetime() -> u32 {
    60
}

fn default_endpoints() -> Vec<String> {
    vec![]
}

/// Configuration of front-link plugin
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct FrontConfig {
    #[serde(default = "default_url")]
    pub url: String,
    #[serde(default = "default_wc")]
    pub wc: i32,
    #[serde(default = "default_retries")]
    pub retries: u8,
    #[serde(default = "default_timeout")]
    pub timeout: u32,
    #[serde(default = "default_timeout")]
    pub message_processing_timeout: u32,
    #[serde(default = "default_out_of_sync")]
    pub out_of_sync_threshold: u32,
    #[serde(default = "default_false")]
    pub verbose: bool,
    #[serde(default = "default_lifetime")]
    pub lifetime: u32,

    // SDK authentication parameters
    pub project_id: Option<String>,
    pub access_key: Option<String>,
    ////////////////////////////////

    #[serde(default = "default_endpoints")]
    pub endpoints: Vec<String>,
}

pub fn get_server_endpoints(config: &FrontConfig) -> Vec<String> {
    let mut cur_endpoints = match config.endpoints.len() {
        0 => vec![config.url.clone()],
        _ => config.endpoints.clone(),
    };
    cur_endpoints.iter_mut().map(|end| {
        let mut end = end.trim_end_matches('/').to_owned();
        if config.project_id.is_some() {
            end.push_str("/");
            end.push_str(&config.project_id.clone().unwrap());
        }
        end.to_owned()
    }).collect::<Vec<String>>()
}

pub fn default_map() -> BTreeMap<String, Vec<String>> {
    [(MAINNET.to_owned(), MAIN_ENDPOINTS.to_owned()),
        (TESTNET.to_owned(), NET_ENDPOINTS.to_owned()),
        (LOCALNET.to_owned(), SE_ENDPOINTS.to_owned()),
    ].iter().cloned().collect()
}

pub fn resolve_net_name(url: &str) -> Option<String> {
    let url_regex = Regex::new(r"^\s*(?:https?://)?(?P<net>\w+\.evercloud\.dev)\s*")
        .expect("Regex compilation error");
    let ton_url_regex = Regex::new(r"^\s*(?:https?://)?(?P<net>\w+\.ton\.dev)\s*")
        .expect("Regex compilation error");
    let everos_url_regex = Regex::new(r"^\s*(?:https?://)?(?P<net>\w+\.everos\.dev)\s*")
        .expect("Regex compilation error");
    let mut net = None;
    for regex in [url_regex, ton_url_regex, everos_url_regex] {
        if let Some(captures) = regex.captures(url) {
            net = Some(captures.name("net")
                .expect("Unexpected: capture <net> was not found")
                .as_str()
                .replace("ton", "evercloud")
                .replace("everos", "evercloud"));
        }
    }
    if let Some(net) = net {
        if default_map().contains_key(&net) {
            return Some(net);
        }
    }
    if url == "main" {
        return Some(MAINNET.to_string());
    }
    if url == "dev" || url == "devnet" {
        return Some(TESTNET.to_string());
    }
    if url.contains("127.0.0.1") ||
        url.contains("0.0.0.0") ||
        url.contains("localhost") {
        return Some(LOCALNET.to_string());
    }
    None
}

pub fn create_client(config: &FrontConfig) -> Result<Arc<ClientContext>, String> {
    let modified_endpoints = get_server_endpoints(config);
    if !config.verbose {
        println!("Connecting to:\n\tUrl: {}", config.url);
        println!("\tEndpoints: {:?}\n", modified_endpoints);
    }
    let endpoints_cnt = if resolve_net_name(&config.url).unwrap_or(config.url.clone()).eq(LOCALNET) {
        1_u8
    } else {
        modified_endpoints.len() as u8
    };
    let cli_conf = ClientConfig {
        abi: AbiConfig {
            workchain: config.wc,
            message_expiration_timeout: config.lifetime * 1000,
            message_expiration_timeout_grow_factor: 1.3,
        },
        crypto: CryptoConfig {
            mnemonic_dictionary: MnemonicDictionary::English,
            mnemonic_word_count: WORD_COUNT,
            hdkey_derivation_path: HD_PATH.to_string(),
        },
        network: NetworkConfig {
            server_address: Some(config.url.to_owned()),
            sending_endpoint_count: endpoints_cnt,
            endpoints: if modified_endpoints.is_empty() {
                    None
                } else {
                    Some(modified_endpoints)
                },
            message_retries_count: config.retries as i8,
            message_processing_timeout: 30000,
            wait_for_timeout: config.timeout,
            out_of_sync_threshold: Some(config.out_of_sync_threshold * 1000),
            access_key: config.access_key.clone(),
            ..Default::default()
        },
        ..Default::default()
    };
    let cli = ClientContext::new(cli_conf).map_err(|e| format!("failed to create tonclient: {}", e))?;
    Ok(Arc::new(cli))
}

pub fn create_client_verbose(config: &FrontConfig) -> Result<Arc<ClientContext>, String> {
    let level = debug_level_from_env();
    log::set_max_level(level);
    log::set_boxed_logger(Box::new(SimpleLogger))
        .map_err(|e| format!("failed to init logger: {}", e))?;
    create_client(config)
}

impl NodeLink for FrontNodeLink {
    fn create_client(&self, cfg_str: String) -> Box<dyn NodeLinkClient> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build().expect("Can't create tokio runtime");
        let config: serde_json::error::Result<FrontConfig> = serde_json::from_str(&cfg_str);
        let config = config.expect("Incorect front link config");
        let ton = create_client_verbose(&config).expect("Front link client creation error");
        return Box::new(FrontNodeLinkClient { ton, rt });
    }
    fn name(&self) -> String {
        "front".to_string()
    }
}

impl NodeLinkClient for FrontNodeLinkClient {
    fn query_accounts(&mut self, addresses: Vec<String>) -> Result<Vec<String>, String> {
        let mut rv = Vec::<String>::new();
        let mut it = 0;
        loop {
            if it >= addresses.len() {
                break;
            }
            let mut filter = json!({ "id": { "eq": addresses[it] } });
            let mut cnt = 1;
            for address in addresses.iter().skip(it).take(50) {
                cnt += 1;
                filter = json!({ "id": { "eq": address },
                    "OR": filter
                });
            }
            it += cnt;
            let query_result = self.rt.block_on(query_collection(
                self.ton.clone(),
                ParamsOfQueryCollection {
                    collection: "accounts".to_owned(),
                    filter: Some(filter),
                    result: ACCOUNT_FIELDS.to_string(),
                    limit: Some(cnt as u32),
                    ..Default::default()
                },
            )).map_err(|e| format!("failed to query account info: {}", e))?;
            rv.extend(query_result.result.iter().map(|x|x.to_string()));
        }
        Ok(rv)
    }
}

#[no_mangle]
pub fn get_node_link() -> *mut dyn NodeLink {
    // Return a raw pointer to an instance of our plugin
    Box::into_raw(Box::new(FrontNodeLink {}))
}
