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
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use clap::ArgMatches;
use lazy_static::lazy_static;
use regex::Regex;
use crate::global_config_path;
use crate::helpers::default_config_name;

const TESTNET: &str = "net.evercloud.dev";
const MAINNET: &str = "main.evercloud.dev";
pub const LOCALNET: &str = "http://127.0.0.1/";

fn default_url() -> String {
    TESTNET.to_string()
}

fn default_wc() -> i32 {
    0
}

fn default_retries() -> u8 {
    5
}

fn default_depool_fee() -> f32 {
    0.5
}

fn default_timeout() -> u32 {
    40000
}

fn default_out_of_sync() -> u32 { 15 }

fn default_false() -> bool {
    false
}

fn default_true() -> bool { true }

fn default_lifetime() -> u32 {
    60
}

fn default_endpoints() -> Vec<String> {
    vec![]
}

fn default_aliases() -> BTreeMap<String, ContractData> {
    BTreeMap::new()
}

fn default_endpoints_map() -> BTreeMap<String, Vec<String>> {
    FullConfig::default_map()
}

fn default_trace() -> String { "None".to_string() }

fn default_config() -> Config {
    Config::new()
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Config {
    #[serde(default = "default_url")]
    pub url: String,
    #[serde(default = "default_wc")]
    pub wc: i32,
    pub addr: Option<String>,
    pub method: Option<String>,
    pub parameters: Option<String>,
    pub wallet: Option<String>,
    pub pubkey: Option<String>,
    pub abi_path: Option<String>,
    pub keys_path: Option<String>,
    #[serde(default = "default_retries")]
    pub retries: u8,
    #[serde(default = "default_timeout")]
    pub timeout: u32,
    #[serde(default = "default_timeout")]
    pub message_processing_timeout: u32,
    #[serde(default = "default_out_of_sync")]
    pub out_of_sync_threshold: u32,
    #[serde(default = "default_false")]
    pub is_json: bool,
    #[serde(default = "default_depool_fee")]
    pub depool_fee: f32,
    #[serde(default = "default_lifetime")]
    pub lifetime: u32,
    #[serde(default = "default_true")]
    pub no_answer: bool,
    #[serde(default = "default_false")]
    pub balance_in_tons: bool,
    #[serde(default = "default_false")]
    pub local_run: bool,
    #[serde(default = "default_false")]
    pub async_call: bool,
    #[serde(default = "default_trace")]
    pub debug_fail: String,

    // SDK authentication parameters
    pub project_id: Option<String>,
    pub access_key: Option<String>,
    ////////////////////////////////

    #[serde(default = "default_endpoints")]
    pub endpoints: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ContractData {
    pub abi_path: Option<String>,
    pub address: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FullConfig {
    #[serde(default = "default_config")]
    pub config: Config,
    #[serde(default = "default_endpoints_map")]
    pub endpoints_map: BTreeMap<String, Vec<String>>,
    #[serde(default = "default_aliases")]
    pub aliases: BTreeMap<String, ContractData>,
    #[serde(default = "default_config_name")]
    pub path: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            url: default_url(),
            wc: default_wc(),
            addr: None,
            method: None,
            parameters: None,
            wallet: None,
            pubkey: None,
            abi_path: None,
            keys_path: None,
            retries: default_retries(),
            timeout: default_timeout(),
            message_processing_timeout: default_timeout(),
            is_json: default_false(),
            depool_fee: default_depool_fee(),
            lifetime: default_lifetime(),
            no_answer: default_true(),
            balance_in_tons: default_false(),
            local_run: default_false(),
            async_call: default_false(),
            endpoints: default_endpoints(),
            out_of_sync_threshold: default_out_of_sync(),
            debug_fail: default_trace(),
            project_id: None,
            access_key: None,
        }
    }
}

impl Default for FullConfig {
    fn default() -> Self {
        FullConfig {
            config: default_config(),
            endpoints_map: default_endpoints_map(),
            aliases: default_aliases(),
            path: default_config_name(),
        }
    }
}

impl Config {
    fn new() -> Self {
        let url = default_url();
        let endpoints = FullConfig::default_map()[&url].clone();
        Config {
            url,
            wc: default_wc(),
            addr: None,
            method: None,
            parameters: None,
            wallet: None,
            pubkey: None,
            abi_path: None,
            keys_path: None,
            retries: default_retries(),
            timeout: default_timeout(),
            message_processing_timeout: default_timeout(),
            is_json: default_false(),
            depool_fee: default_depool_fee(),
            lifetime: default_lifetime(),
            no_answer: default_true(),
            balance_in_tons: default_false(),
            local_run: default_false(),
            async_call: default_false(),
            endpoints,
            out_of_sync_threshold: default_out_of_sync(),
            debug_fail: default_trace(),
            project_id: None,
            access_key: None,
        }
    }
}


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
        if FullConfig::default_map().contains_key(&net) {
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

impl FullConfig {
    fn new(config: Config,path: String) -> Self {
        FullConfig {
            config,
            endpoints_map: Self::default_map(),
            aliases: BTreeMap::new(),
            path,
        }
    }

    pub fn default_map() -> BTreeMap<String, Vec<String>> {
        [(MAINNET.to_owned(), MAIN_ENDPOINTS.to_owned()),
            (TESTNET.to_owned(), NET_ENDPOINTS.to_owned()),
            (LOCALNET.to_owned(), SE_ENDPOINTS.to_owned()),
        ].iter().cloned().collect()
    }

    pub fn from_file(path: &str) -> FullConfig {
        let conf_str = std::fs::read_to_string(path).ok().unwrap_or_default();
        let config: serde_json::error::Result<Config>  = serde_json::from_str(&conf_str);
        if let Ok(config) = config {
            if config != Config::default() {
                return FullConfig::new(config, path.to_string());
            }
        }
        let full_config: serde_json::error::Result<FullConfig> = serde_json::from_str(&conf_str);
        let mut full_config = if let Ok(full_config) = full_config {
            full_config
        } else {
            let conf_str = std::fs::read_to_string(global_config_path()).ok()
                .unwrap_or_default();
            let mut global_config = serde_json::from_str::<FullConfig>(&conf_str)
                .unwrap_or(FullConfig::default());
            global_config.path = path.to_string();
            global_config
        };
        full_config.path = path.to_string();
        full_config
    }

    pub fn to_file(&self, path: &str) -> Result<(), String>{
        let conf_str = serde_json::to_string_pretty(self)
            .map_err(|_| "failed to serialize config object".to_string())?;
        std::fs::write(path, conf_str).map_err(|e| format!("failed to write config file {}: {}", path, e))?;
        Ok(())
    }

    pub fn print_endpoints(path: &str) {
        let fconf = FullConfig::from_file(path);
        println!(
            "{}",
            serde_json::to_string_pretty(&fconf.endpoints_map).unwrap_or(
                "Failed to print endpoints map.".to_owned()
            )
        );
    }

    pub fn print_aliases(&self) {
        println!(
            "{}",
            serde_json::to_string_pretty(&self.aliases).unwrap_or(
                "Failed to print aliases map.".to_owned()
            )
        );
    }

    pub fn add_alias(&mut self, alias: &str, address: Option<String>, abi: Option<String>, key_path: Option<String>) -> Result<(), String> {
        self.aliases.insert(alias.to_owned(), ContractData {abi_path: abi, address, key_path} );
        self.to_file(&self.path)
    }

    pub fn remove_alias(&mut self, alias: &str) -> Result<(), String> {
        self.aliases.remove(alias);
        self.to_file(&self.path)
    }

    pub fn add_endpoint(path: &str, url: &str, endpoints: &str) -> Result<(), String> {
        let mut fconf = FullConfig::from_file(path);
        let mut new_endpoints : Vec<String> = endpoints
            .replace(['[', ']'], "")
            .split(',')
            .map(|s| s.to_string())
            .collect();

        let old_endpoints = fconf.endpoints_map.entry(url.to_string()).or_insert(vec![]);
        old_endpoints.append(&mut new_endpoints);
        old_endpoints.sort();
        old_endpoints.dedup();
        fconf.to_file(path)
    }

    pub fn remove_endpoint(path: &str, url: &str) -> Result<(), String> {
        let mut fconf = FullConfig::from_file(path);
        if !fconf.endpoints_map.contains_key(url) {
            return Err("Endpoints map doesn't contain such url.".to_owned());
        }
        fconf.endpoints_map.remove(url);
        fconf.to_file(path)
    }

    pub fn reset_endpoints(path: &str) -> Result<(), String> {
        let mut fconf = FullConfig::from_file(path);
        fconf.endpoints_map = FullConfig::default_map();
        fconf.to_file(path)
    }
}

pub fn clear_config(
    full_config: &mut FullConfig,
    matches: &ArgMatches,
    is_json: bool,
) -> Result<(), String> {
    let config = &mut full_config.config;
    let is_json = config.is_json || is_json;
    if matches.is_present("URL") {
        let url = default_url();
        config.endpoints = FullConfig::default_map()[&url].clone();
        config.url = url;
    }
    if matches.is_present("ADDR") {
        config.addr = None;
    }
    if matches.is_present("WALLET") {
        config.wallet = None;
    }
    if matches.is_present("ABI") {
        config.abi_path = None;
    }
    if matches.is_present("KEYS") {
        config.keys_path = None;
    }
    if matches.is_present("METHOD") {
        config.method = None;
    }
    if matches.is_present("PARAMETERS") {
        config.parameters = None;
    }
    if matches.is_present("PUBKEY") {
        config.pubkey = None;
    }
    if matches.is_present("RETRIES") {
        config.retries = default_retries();
    }
    if matches.is_present("LIFETIME") {
        config.lifetime = default_lifetime();
    }
    if matches.is_present("TIMEOUT") {
        config.timeout = default_timeout();
    }
    if matches.is_present("MSG_TIMEOUT") {
        config.timeout = default_timeout();
    }
    if matches.is_present("WC") {
        config.wc = default_wc();
    }
    if matches.is_present("DEPOOL_FEE") {
        config.depool_fee = default_depool_fee();
    }
    if matches.is_present("NO_ANSWER") {
        config.no_answer = default_true();
    }
    if matches.is_present("BALANCE_IN_TONS") {
        config.balance_in_tons = default_false();
    }
    if matches.is_present("LOCAL_RUN") {
        config.local_run = default_false();
    }
    if matches.is_present("ASYNC_CALL") {
        config.async_call = default_false();
    }
    if matches.is_present("DEBUG_FAIL") {
        config.debug_fail = default_trace();
    }
    if matches.is_present("OUT_OF_SYNC") {
        config.out_of_sync_threshold = default_out_of_sync();
    }
    if matches.is_present("IS_JSON") {
        config.is_json = default_false();
    }
    if matches.is_present("PROJECT_ID") {
        config.project_id = None;
        if config.access_key.is_some() && !config.is_json {
            println!("Warning: You have access_key set without project_id. It has no sense in case of authentication.");
        }
    }
    if matches.is_present("ACCESS_KEY") {
        config.access_key = None;
    }

    if matches.args.is_empty() {
        *config = Config::new();
    }

    full_config.to_file(&full_config.path)?;
    if !is_json {
        println!("Succeeded.");
    }
    Ok(())
}

pub fn set_config(
    full_config: &mut FullConfig,
    matches: &ArgMatches,
    is_json: bool,
) -> Result<(), String> {
    let config= &mut full_config.config;
    if let Some(s) = matches.value_of("URL") {
        let resolved_url = resolve_net_name(s).unwrap_or(s.to_owned());
        let empty : Vec<String> = Vec::new();
        config.endpoints = full_config.endpoints_map.get(&resolved_url).unwrap_or(&empty).clone();
        config.url = resolved_url;
    }
    if let Some(s) = matches.value_of("ADDR") {
        config.addr = Some(s.to_string());
    }
    if let Some(method) = matches.value_of("METHOD") {
        config.method = Some(method.to_string());
    }
    if let Some(parameters) = matches.value_of("PARAMETERS") {
        config.parameters = Some(parameters.to_string());
    }
    if let Some(s) = matches.value_of("WALLET") {
        config.wallet = Some(s.to_string());
    }
    if let Some(s) = matches.value_of("PUBKEY") {
        config.pubkey = Some(s.to_string());
    }
    if let Some(s) = matches.value_of("ABI") {
        config.abi_path = Some(s.to_string());
    }
    if let Some(s) = matches.value_of("KEYS") {
        config.keys_path = Some(s.to_string());
    }
    if let Some(retries) = matches.value_of("RETRIES") {
        config.retries = u8::from_str_radix(retries, 10)
            .map_err(|e| format!(r#"failed to parse "retries": {}"#, e))?;
    }
    if let Some(lifetime) = matches.value_of("LIFETIME") {
        config.lifetime = u32::from_str_radix(lifetime, 10)
            .map_err(|e| format!(r#"failed to parse "lifetime": {}"#, e))?;
        if config.lifetime < 2 * config.out_of_sync_threshold {
            config.out_of_sync_threshold = config.lifetime >> 1;
        }
    }
    if let Some(timeout) = matches.value_of("TIMEOUT") {
        config.timeout = u32::from_str_radix(timeout, 10)
            .map_err(|e| format!(r#"failed to parse "timeout": {}"#, e))?;
    }
    if let Some(message_processing_timeout) = matches.value_of("MSG_TIMEOUT") {
        config.message_processing_timeout = u32::from_str_radix(message_processing_timeout, 10)
            .map_err(|e| format!(r#"failed to parse "message_processing_timeout": {}"#, e))?;
    }
    if let Some(wc) = matches.value_of("WC") {
        config.wc = i32::from_str_radix(wc, 10)
            .map_err(|e| format!(r#"failed to parse "workchain id": {}"#, e))?;
    }
    if let Some(depool_fee) = matches.value_of("DEPOOL_FEE") {
        config.depool_fee = depool_fee.parse::<f32>()
            .map_err(|e| format!(r#"failed to parse "depool_fee": {}"#, e))?;
        if config.depool_fee < 0.5 {
            return Err("Minimal value for depool fee is 0.5".to_string());
        }
    }
    if let Some(no_answer) = matches.value_of("NO_ANSWER") {
        config.no_answer = no_answer.parse::<bool>()
            .map_err(|e| format!(r#"failed to parse "no_answer": {}"#, e))?;
    }
    if let Some(balance_in_tons) = matches.value_of("BALANCE_IN_TONS") {
        config.balance_in_tons = balance_in_tons.parse::<bool>()
            .map_err(|e| format!(r#"failed to parse "balance_in_tons": {}"#, e))?;
    }
    if let Some(local_run) = matches.value_of("LOCAL_RUN") {
        config.local_run = local_run.parse::<bool>()
            .map_err(|e| format!(r#"failed to parse "local_run": {}"#, e))?;
    }
    if let Some(async_call) = matches.value_of("ASYNC_CALL") {
        config.async_call = async_call.parse::<bool>()
            .map_err(|e| format!(r#"failed to parse "async_call": {}"#, e))?;
    }
    if let Some(out_of_sync_threshold) = matches.value_of("OUT_OF_SYNC") {
        let time = u32::from_str_radix(out_of_sync_threshold, 10)
            .map_err(|e| format!(r#"failed to parse "out_of_sync_threshold": {}"#, e))?;
        if time * 2 > config.lifetime {
            return  Err("\"out_of_sync\" should not exceed 0.5 * \"lifetime\".".to_string());
        }
        config.out_of_sync_threshold = time;
    }
    if let Some(debug_fail) = matches.value_of("DEBUG_FAIL") {
        let debug_fail = debug_fail.to_lowercase();
        config.debug_fail = if debug_fail == "full" {
            "Full".to_string()
        } else if debug_fail == "minimal" {
            "Minimal".to_string()
        } else if debug_fail == "none" {
            "None".to_string()
        } else {
            return Err(r#"Wrong value for "debug_fail" config."#.to_string())
        };
    }
    if let Some(is_json) = matches.value_of("IS_JSON") {
        config.is_json = is_json.parse::<bool>()
            .map_err(|e| format!(r#"failed to parse "is_json": {}"#, e))?;
    }
    if let Some(s) = matches.value_of("PROJECT_ID") {
        config.project_id = Some(s.to_string());
    }
    if let Some(s) = matches.value_of("ACCESS_KEY") {
        config.access_key = Some(s.to_string());
        if config.project_id.is_none() && !(config.is_json || is_json) {
            println!("Warning: You have access_key set without project_id. It has no sense in case of authentication.");
        }
    }

    full_config.to_file(&full_config.path)?;
    if !(full_config.config.is_json || is_json) {
        println!("Succeeded.");
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::{resolve_net_name, LOCALNET, TESTNET, MAINNET};

    #[test]
    fn test_endpoints_resolver() {
        assert_eq!(resolve_net_name(""), None);
        assert_eq!(resolve_net_name("http://os.ton.dev"), None);
        assert_eq!(resolve_net_name("https://rustnet.ton.dev"), None);
        assert_eq!(resolve_net_name("rustnet.ton.com"), None);
        assert_eq!(resolve_net_name("https://example.com"), None);
        assert_eq!(resolve_net_name("http://localhost"), Some(LOCALNET.to_owned()));
        assert_eq!(resolve_net_name("https://localhost"), Some(LOCALNET.to_owned()));
        assert_eq!(resolve_net_name("localhost"), Some(LOCALNET.to_owned()));
        assert_eq!(resolve_net_name("http://127.0.0.1"), Some(LOCALNET.to_owned()));
        assert_eq!(resolve_net_name("https://127.0.0.1"), Some(LOCALNET.to_owned()));
        assert_eq!(resolve_net_name("https://127.0.0.2"), None);
        assert_eq!(resolve_net_name("https://127.1.0.1"), None);
        assert_eq!(resolve_net_name("https://0.0.0.1"), None);
        assert_eq!(resolve_net_name("https://1.0.0.0"), None);

        assert_eq!(resolve_net_name("https://main.ton.dev"), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("https://main.everos.dev"), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("https://main.evercloud.dev"), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("http://main.ton.dev"), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("  http://main.ton.dev  "), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("  https://main.ton.dev  "), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("main.ton.dev"), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("main.everos.dev"), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("main.evercloud.dev"), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("main"), Some(MAINNET.to_owned()));
        assert_eq!(resolve_net_name("main.ton.com"), None);

        assert_eq!(resolve_net_name("https://net.ton.dev"), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("https://net.everos.dev"), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("https://net.evercloud.dev"), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("http://net.ton.dev"), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("  http://net.ton.dev  "), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("  https://net.ton.dev  "), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("net.ton.dev"), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("dev"), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("devnet"), Some(TESTNET.to_owned()));
        assert_eq!(resolve_net_name("net.ton.com"), None);
    }
}
