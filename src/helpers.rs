/*
 * Copyright 2018-2023 EverX.
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
use std::env;
use std::path::PathBuf;
use crate::config::{Config, LOCALNET};
use crate::debug::debug_level_from_env;

use std::sync::Arc;
use std::time::{Duration, SystemTime};
use ton_client::abi::{
    Abi, AbiConfig, AbiContract, DecodedMessageBody, DeploySet, ParamsOfDecodeMessageBody,
    ParamsOfEncodeMessage, Signer,
};
use ton_client::crypto::{CryptoConfig, KeyPair, MnemonicDictionary};
use ton_client::error::ClientError;
use ton_client::net::{query_collection, OrderBy, ParamsOfQueryCollection, NetworkConfig};
use ton_client::{ClientConfig, ClientContext};
use ton_block::{Account, MsgAddressInt, Deserializable, CurrencyCollection, StateInit, Serializable};
use ton_vm::executor::{Engine, EngineTraceInfo};
use std::str::FromStr;
use clap::ArgMatches;
use serde_json::{Value, json};
use ton_client::abi::Abi::Contract;
use ton_executor::BlockchainConfig;
use url::Url;
use crate::call::parse_params;
use crate::{FullConfig, resolve_net_name};
use crate::replay::{CONFIG_ADDR, construct_blockchain_config};

pub const HD_PATH: &str = "m/44'/396'/0'/0/0";
pub const WORD_COUNT: u8 = 12;

const CONFIG_BASE_NAME: &str = "tonos-cli.conf.json";
const GLOBAL_CONFIG_PATH: &str = ".tonos-cli.global.conf.json";

pub type CallbackType = Arc<dyn Fn(&Engine, &EngineTraceInfo) + Send + Sync>;

pub fn default_config_name() -> String {
    env::current_dir()
        .map(|dir| {
            dir.join(PathBuf::from(CONFIG_BASE_NAME)).to_str().unwrap().to_string()
        })
        .unwrap_or(CONFIG_BASE_NAME.to_string())
}

pub fn global_config_path() -> String {
    env::current_exe()
        .map(|mut dir| {
            dir.set_file_name(GLOBAL_CONFIG_PATH);
            dir.to_str().unwrap().to_string()
        })
        .unwrap_or(GLOBAL_CONFIG_PATH.to_string())
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

pub fn read_keys(filename: &str) -> Result<KeyPair, String> {
    let keys_str = std::fs::read_to_string(filename)
        .map_err(|e| format!("failed to read the keypair file: {}", e))?;
    let keys: KeyPair = serde_json::from_str(&keys_str)
        .map_err(|e| format!("failed to load keypair: {}", e))?;
    Ok(keys)
}

pub fn load_ton_address(addr: &str, config: &Config) -> Result<String, String> {
    let addr = if addr.find(':').is_none() {
        format!("{}:{}", config.wc, addr)
    } else {
        addr.to_owned()
    };
    let _ = MsgAddressInt::from_str(&addr)
        .map_err(|e| format!("Address is specified in the wrong format. Error description: {}", e))?;
    Ok(addr)
}

pub fn now() -> u32 {
    (now_ms() / 1000) as u32
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|e| panic!("failed to obtain system time: {}", e))
        .as_millis() as u64
}

pub type TonClient = Arc<ClientContext>;

pub fn create_client_local() -> Result<TonClient, String> {
    let cli = ClientContext::new(ClientConfig::default())
        .map_err(|e| format!("failed to create tonclient: {}", e))?;
    Ok(Arc::new(cli))
}

pub fn get_server_endpoints(config: &Config) -> Vec<String> {
    let mut cur_endpoints = match config.endpoints.len() {
        0 => vec![config.url.clone()],
        _ => config.endpoints.clone(),
    };
    cur_endpoints.iter_mut().map(|end| {
            let mut end = end.trim_end_matches('/').to_owned();
        if config.project_id.is_some() {
            end.push('/');
            end.push_str(&config.project_id.clone().unwrap());
        }
        end.to_owned()
    }).collect::<Vec<String>>()
}

pub fn create_client(config: &Config) -> Result<TonClient, String> {
    let modified_endpoints = get_server_endpoints(config);
    if !config.is_json {
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
    let cli =
        ClientContext::new(cli_conf).map_err(|e| format!("failed to create tonclient: {}", e))?;
    Ok(Arc::new(cli))
}

pub fn create_client_verbose(config: &Config) -> Result<TonClient, String> {
    let level = debug_level_from_env();
    log::set_max_level(level);
    log::set_boxed_logger(Box::new(SimpleLogger))
        .map_err(|e| format!("failed to init logger: {}", e))?;
    create_client(config)
}

pub async fn query_raw(
    config: &Config,
    collection: &str,
    filter: Option<&str>,
    limit: Option<&str>,
    order: Option<&str>,
    result: &str
) -> Result<(), String>
{
    let context = create_client_verbose(config)?;

    let filter = filter.map(serde_json::from_str).transpose()
        .map_err(|e| format!("Failed to parse filter field: {}", e))?;
    let limit = limit.map(|s| s.parse::<u32>()).transpose()
        .map_err(|e| format!("Failed to parse limit field: {}", e))?;
    let order = order.map(serde_json::from_str).transpose()
        .map_err(|e| format!("Failed to parse order field: {}", e))?;

    let query = ton_client::net::query_collection(
        context.clone(),
        ParamsOfQueryCollection {
            collection: collection.to_owned(),
            filter,
            limit,
            order,
            result: result.to_owned(),
        }
    ).await.map_err(|e| format!("Failed to execute query: {}", e))?;

    println!("{:#}", Value::Array(query.result));
    Ok(())
}

pub async fn query_with_limit(
    ton: TonClient,
    collection: &str,
    filter: Value,
    result: &str,
    order: Option<Vec<OrderBy>>,
    limit: Option<u32>,
) -> Result<Vec<Value>, ClientError> {
    query_collection(
        ton,
        ParamsOfQueryCollection {
            collection: collection.to_owned(),
            filter: Some(filter),
            result: result.to_owned(),
            order,
            limit,
        },
    )
        .await
        .map(|r| r.result)
}

pub async fn query_message(
    ton: TonClient,
    message_id: &str,
) -> Result<String, String> {
    let messages = query_with_limit(
        ton.clone(),
        "messages",
        json!({ "id": { "eq": message_id } }),
        "boc",
        None,
        Some(1),
    ).await
        .map_err(|e| format!("failed to query account data: {}", e))?;
    if messages.is_empty() {
        Err("message with specified id was not found.".to_string())
    }
    else {
        Ok(messages[0]["boc"].as_str().ok_or("Failed to obtain message boc.".to_string())?.to_string())
    }
}

pub async fn query_account_field(ton: TonClient, address: &str, field: &str) -> Result<String, String> {
    let accounts = query_with_limit(
        ton.clone(),
        "accounts",
        json!({ "id": { "eq": address } }),
        field,
        None,
        Some(1),
    ).await
        .map_err(|e| format!("failed to query account data: {}", e))?;
    if accounts.is_empty() {
        return Err(format!("account with address {} not found", address));
    }
    let data = accounts[0][field].as_str();
    if data.is_none() {
        return Err(format!("account doesn't contain {}", field));
    }
    Ok(data.unwrap().to_string())
}


pub async fn decode_msg_body(
    ton: TonClient,
    abi_path: &str,
    body: &str,
    is_internal: bool,
    config: &Config,
) -> Result<DecodedMessageBody, String> {

    let abi = load_abi(abi_path, config).await?;
    ton_client::abi::decode_message_body(
        ton,
        ParamsOfDecodeMessageBody {
            abi,
            body: body.to_owned(),
            is_internal,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| format!("failed to decode body: {}", e))
}

pub async fn load_abi_str(abi_path: &str, config: &Config) -> Result<String, String> {
    let abi_from_json = serde_json::from_str::<AbiContract>(abi_path);
    if abi_from_json.is_ok() {
        return Ok(abi_path.to_string());
    }
    if Url::parse(abi_path).is_ok() {
        let abi_bytes = load_file_with_url(abi_path, config.timeout as u64).await?;
        return String::from_utf8(abi_bytes)
            .map_err(|e| format!("Downloaded string contains not valid UTF8 characters: {}", e));
    }
    std::fs::read_to_string(abi_path)
        .map_err(|e| format!("failed to read ABI file: {}", e))
}

pub async fn load_abi(abi_path: &str, config: &Config) -> Result<Abi, String> {
    let abi_str = load_abi_str(abi_path, config).await?;
    Ok(Contract(serde_json::from_str::<AbiContract>(&abi_str)
            .map_err(|e| format!("ABI is not a valid json: {}", e))?,
    ))
}

pub async fn load_ton_abi(abi_path: &str, config: &Config) -> Result<ton_abi::Contract, String> {
    let abi_str = load_abi_str(abi_path, config).await?;
    ton_abi::Contract::load(abi_str.as_bytes())
        .map_err(|e| format!("Failed to load ABI: {}", e))
}

pub async fn load_file_with_url(url: &str, timeout: u64) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout))
        .build()
        .map_err(|e| format!("Failed to create client: {e}"))?;
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to send get request: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to get response bytes: {e}"))?;
    Ok(res.to_vec())

}


pub async fn calc_acc_address(
    tvc: &[u8],
    wc: i32,
    pubkey: Option<String>,
    init_data: Option<&str>,
    abi: Abi,
) -> Result<String, String> {
    let ton = create_client_local()?;

    let init_data_json = init_data
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| format!("initial data is not in json: {}", e))?;

    let dset = DeploySet {
        tvc: Some(base64::encode(tvc)),
        workchain_id: Some(wc),
        initial_data: init_data_json,
        initial_pubkey: pubkey.clone(),
        ..Default::default()
    };
    let result = ton_client::abi::encode_message(
        ton.clone(),
        ParamsOfEncodeMessage {
            abi,
            deploy_set: Some(dset),
            signer: if pubkey.is_some() {
                Signer::External {
                    public_key: pubkey.unwrap(),
                }
            } else {
                Signer::None
            },
            ..Default::default()
        },
    )
    .await
    .map_err(|e| format!("cannot generate address: {}", e))?;
    Ok(result.address)
}

pub fn answer_filter(src: &str, dst: &str, since: u32) -> serde_json::Value {
    json!({
        "src": { "eq": src },
        "dst": { "eq": dst },
        "created_at": {"ge": since }
    })
}

pub fn events_filter(addr: &str, since: u32) -> serde_json::Value {
    json!({
        "src": { "eq": addr },
        "msg_type": {"eq": 2 },
        "created_at": {"ge": since }
    })
}

pub async fn print_message(ton: TonClient, message: &Value, abi: &str, is_internal: bool) -> Result<(String, String), String> {
    println!("Id: {}", message["id"].as_str().unwrap_or("Undefined"));
    let value = message["value"].as_str().unwrap_or("0x0");
    let value = u64::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|e| format!("failed to decode msg value: {}", e))?;
    let value: f64 = value as f64 / 1e9;
    println!("Value: {:.9}", value);
    println!("Created at: {} ({})",
        message["created_at"].as_u64().unwrap_or(0),
        message["created_at_string"].as_str().unwrap_or("Undefined")
    );

    let body = message["body"].as_str();
    if body.is_some() {
        let body = body.unwrap();
        let def_config = Config::default();
        let result = ton_client::abi::decode_message_body(
            ton.clone(),
            ParamsOfDecodeMessageBody {
                abi: load_abi(abi, &def_config).await?,
                body: body.to_owned(),
                is_internal,
                ..Default::default()
            },
        ).await;
        let (name, args) = if result.is_err() {
            ("unknown".to_owned(), "{}".to_owned())
        } else {
            let result = result.unwrap();
            (result.name, serde_json::to_string(&result.value)
                .map_err(|e| format!("failed to serialize the result: {}", e))?)
        };
        println!("Decoded body:\n{} {}\n", name, args);
        return Ok((name, args));
    }
    println!();
    Ok(("".to_owned(), "".to_owned()))
}

pub fn json_account(
    acc_type: Option<String>,
    address: Option<String>,
    balance: Option<String>,
    last_paid: Option<String>,
    last_trans_lt: Option<String>,
    data: Option<String>,
    code_hash: Option<String>,
    state_init: Option<String>,
) -> Value {
    let mut res = json!({ });
    if acc_type.is_some() {
        res["acc_type"] = json!(acc_type.unwrap());
    }
    if address.is_some() {
        res["address"] = json!(address.unwrap());
    }
    if balance.is_some() {
        res["balance"] = json!(balance.unwrap());
    }
    if last_paid.is_some() {
        res["last_paid"] = json!(last_paid.unwrap());
    }
    if last_trans_lt.is_some() {
        res["last_trans_lt"] = json!(last_trans_lt.unwrap());
    }
    if data.is_some() {
        res["data_boc"] = json!(data.unwrap());
    }
    if code_hash.is_some() {
        res["code_hash"] = json!(code_hash.unwrap());
    }
    if state_init.is_some() {
        res["state_init"] = json!(state_init.unwrap());
    }
    res
}


pub fn print_account(
    config: &Config,
    acc_type: Option<String>,
    address: Option<String>,
    balance: Option<String>,
    last_paid: Option<String>,
    last_trans_lt: Option<String>,
    data: Option<String>,
    code_hash: Option<String>,
    state_init: Option<String>,
) {
    if config.is_json {
        let acc = json_account(
            acc_type,
            address,
            balance,
            last_paid,
            last_trans_lt,
            data,
            code_hash,
            state_init,
        );
        println!("{:#}", acc);
    } else {
        if acc_type.is_some() && acc_type.clone().unwrap() == "NonExist" {
            println!("Account does not exist.");
            return;
        }
        if address.is_some() {
            println!("address:       {}", address.unwrap());
        }
        if acc_type.is_some() {
            println!("acc_type:      {}", acc_type.unwrap());
        }
        if balance.is_some() {
            println!("balance:       {}", balance.unwrap());
        }
        if last_paid.is_some() {
            println!("last_paid:     {}", last_paid.unwrap());
        }
        if last_trans_lt.is_some() {
            println!("last_trans_lt: {}", last_trans_lt.unwrap());
        }
        if data.is_some() {
            println!("data_boc:      {}", data.unwrap());
        }
        if code_hash.is_some() {
            println!("code_hash:     {}", code_hash.unwrap());
        }
        if state_init.is_some() {
            println!("state_init:    {}", state_init.unwrap());
        }
    }
}

pub fn construct_account_from_tvc(tvc_path: &str, address: Option<&str>, balance: Option<u64>) -> Result<Account, String> {
    Account::active_by_init_code_hash(
        match address {
            Some(address) => MsgAddressInt::from_str(address)
                .map_err(|e| format!("Failed to set address: {}", e))?,
            _ => MsgAddressInt::default()
        },
        match balance {
            Some(balance) => CurrencyCollection::with_grams(balance),
            _ => CurrencyCollection::default()
        },
        0,
        StateInit::construct_from_file(tvc_path)
            .map_err(|e| format!(" failed to load TVC from the file {}: {}", tvc_path, e))?,
        true
    ).map_err(|e| format!(" failed to create account with the stateInit: {}",e))
}

pub fn check_dir(path: &str) -> Result<(), String> {
    if !path.is_empty() && !std::path::Path::new(path).exists() {
        std::fs::create_dir(path)
            .map_err(|e| format!("Failed to create folder {}: {}", path, e))?;
    }
    Ok(())
}

#[derive(PartialEq)]
pub enum AccountSource {
    Network,
    Boc,
    Tvc,
}

pub async fn load_account(
    source_type: &AccountSource,
    source: &str,
    ton_client: Option<TonClient>,
    config: &Config
) -> Result<(Account, String), String> {
    match source_type {
        AccountSource::Network => {
            let ton_client = match ton_client {
                Some(ton_client) => ton_client,
                None => {
                    create_client(config)?
                }
            };
            let boc = query_account_field(ton_client.clone(),source, "boc").await?;
            Ok((Account::construct_from_base64(&boc)
                .map_err(|e| format!("Failed to construct account: {}", e))?,
                boc))
        },
        _ => {
            let account = if source_type == &AccountSource::Boc {
                Account::construct_from_file(source)
                    .map_err(|e| format!(" failed to load account from the file {}: {}", source, e))?
            } else {
                construct_account_from_tvc(source, None, None)?
            };
            let account_bytes = account.write_to_bytes()
                .map_err(|e| format!(" failed to load data from the account: {}", e))?;
            Ok((account, base64::encode(account_bytes)))
        },
    }
}


pub fn load_debug_info(abi: &str) -> Option<String> {
    check_file_exists(abi, &[".json", ".abi"], &[".dbg.json", ".debug.json", ".map.json"])
}

pub fn load_abi_from_tvc(tvc: &str) -> Option<String> {
    check_file_exists(tvc, &[".tvc"], &[".abi.json"])
}

pub fn check_file_exists(path: &str, trim: &[&str], ending: &[&str]) -> Option<String> {
    let mut path = path;
    for end in trim {
        path = path.trim_end_matches(end);
    }
    let path = path.to_string();
    for end in ending {
        let mut new_path = path.clone();
        new_path.push_str(end);
        if std::path::Path::new(&new_path).exists() {
            return Some(new_path);
        }
    }
    None
}

pub fn abi_from_matches_or_config(matches: &ArgMatches<'_>, config: &Config) -> Result<String, String> {
    matches.value_of("ABI")
        .map(|s| s.to_string())
        .or(config.abi_path.clone())
        .ok_or("ABI file is not defined. Supply it in the config file or command line.".to_string())
}

pub fn parse_lifetime(lifetime: Option<&str>, config: &Config) -> Result<u32, String> {
    Ok(lifetime.map(|val| {
        u32::from_str_radix(val, 10)
            .map_err(|e| format!("failed to parse lifetime: {}", e))
    })
        .transpose()?
        .unwrap_or(config.lifetime))
}


#[macro_export]
macro_rules! print_args {
    ($( $arg:expr ),* ) => {
        println!("Input arguments:");
        $(
            println!(
                "{:>width$}: {}",
                stringify!($arg),
                if let Some(ref arg) = $arg { arg.as_ref() } else { "None" },
                width = 8
            );
        )*
    };
}

pub fn load_params(params: &str) -> Result<String, String> {
    if params.find('{').is_none() {
        std::fs::read_to_string(params)
            .map_err(|e| format!("failed to load params from file: {}", e))
    } else {
        Ok(params.to_string())
    }
}

pub async fn unpack_alternative_params(matches: &ArgMatches<'_>, abi_path: &str, method: &str, config: &Config) -> Result<String, String> {
    if let Some(params) = matches.values_of("PARAMS") {
        let params = params.collect();
        parse_params(params, abi_path, method, config).await
    } else {
        Ok(config.parameters.clone().unwrap_or("{}".to_string()))
    }
}

pub fn wc_from_matches_or_config(matches: &ArgMatches<'_>, config: &Config) -> Result<i32 ,String> {
    Ok(matches.value_of("WC")
        .map(|v| i32::from_str_radix(v, 10))
        .transpose()
        .map_err(|e| format!("failed to parse workchain id: {}", e))?
        .unwrap_or(config.wc))
}

pub struct ContractData {
    pub address: Option<String>,
    pub abi: Option<String>,
    pub keys: Option<String>,
}

pub fn contract_data_from_matches_or_config_alias(
    matches: &ArgMatches<'_>,
    full_config: &FullConfig
) -> Result<ContractData, String> {
    let address = matches.value_of("ADDRESS")
        .map(|s| s.to_string())
        .or(full_config.config.addr.clone())
        .ok_or("ADDRESS is not defined. Supply it in the config file or command line.".to_string())?;
    let (address, abi, keys) = if full_config.aliases.contains_key(&address) {
        let alias = full_config.aliases.get(&address).unwrap();
        (alias.address.clone(), alias.abi_path.clone(), alias.key_path.clone())
    } else {
        (Some(address), None, None)
    };
    let abi = matches.value_of("ABI")
        .map(|s| s.to_string())
        .or(full_config.config.abi_path.clone())
        .or(abi)
        .ok_or("ABI file is not defined. Supply it in the config file or command line.".to_string())?;
    let keys = matches.value_of("KEYS")
        .map(|s| s.to_string())
        .or(full_config.config.keys_path.clone())
        .or(keys);
    Ok(ContractData { address, abi: Some(abi), keys })
}

pub fn blockchain_config_from_default_json() -> Result<BlockchainConfig, String> {
    // Default config params from evernode-se https://github.com/tonlabs/evernode-se/blob/master/docker/ton-node/blockchain.conf.json
    let json = r#"{
  "p0": "5555555555555555555555555555555555555555555555555555555555555555",
  "p1": "3333333333333333333333333333333333333333333333333333333333333333",
  "p2": "0000000000000000000000000000000000000000000000000000000000000000",
  "p7": [],
  "p8": {
    "version": 5,
    "capabilities": "1180974"
  },
  "p9": [
    0,
    1,
    9,
    10,
    12,
    14,
    15,
    16,
    17,
    18,
    20,
    21,
    22,
    23,
    24,
    25,
    28,
    34
  ],
  "p10": [
    0,
    1,
    9,
    10,
    12,
    14,
    15,
    16,
    17,
    32,
    34,
    36,
    4294966295,
    4294966296,
    4294966297
  ],
  "p11": {
    "normal_params": {
      "min_tot_rounds": 2,
      "max_tot_rounds": 3,
      "min_wins": 2,
      "max_losses": 2,
      "min_store_sec": 1000000,
      "max_store_sec": 10000000,
      "bit_price": 1,
      "cell_price": 500
    },
    "critical_params": {
      "min_tot_rounds": 4,
      "max_tot_rounds": 7,
      "min_wins": 4,
      "max_losses": 2,
      "min_store_sec": 5000000,
      "max_store_sec": 20000000,
      "bit_price": 2,
      "cell_price": 1000
    }
  },
  "p12": [
    {
      "workchain_id": 0,
      "enabled_since": 1605687562,
      "actual_min_split": 0,
      "min_split": 4,
      "max_split": 8,
      "active": true,
      "accept_msgs": true,
      "flags": 0,
      "zerostate_root_hash": "c52f085257330ec9b73b94a45b591f997849405a4de5b778edbde5f9775f9a8b",
      "zerostate_file_hash": "bd1e95b4e69afbaf5b5186eeeca15a87e16c13feff53595ae6891c12a5790b05",
      "version": 0,
      "basic": true,
      "vm_version": -1,
      "vm_mode": 0
    },
    {
      "workchain_id": 777,
      "enabled_since": 1605687544,
      "actual_min_split": 0,
      "min_split": 5,
      "max_split": 8,
      "active": true,
      "accept_msgs": false,
      "flags": 0,
      "zerostate_root_hash": "ee2f085257330ec9b73b94a45b591f997849405a4de5b778edbde5f9775f9a8b",
      "zerostate_file_hash": "ff1e95b4e69afbaf5b5186eeeca15a87e16c13feff53595ae6891c12a5790b05",
      "version": 0,
      "basic": true,
      "vm_version": -1,
      "vm_mode": 0
    }
  ],
  "p13": {
    "boc": "te6ccgEBAQEADQAAFRpRdIdugAEBIB9I"
  },
  "p14": {
    "masterchain_block_fee": "1700000000",
    "basechain_block_fee": "1000000000"
  },
  "p15": {
    "validators_elected_for": 14400,
    "elections_start_before": 7200,
    "elections_end_before": 1800,
    "stake_held_for": 7200
  },
  "p16": {
    "max_validators": 1000,
    "max_main_validators": 100,
    "min_validators": 5
  },
  "p17": {
    "min_stake": "10000000000000",
    "max_stake": "10000000000000000",
    "min_total_stake": "100000000000000",
    "max_stake_factor": 196608
  },
  "p18": [
    {
      "utime_since": 0,
      "bit_price_ps": "1",
      "cell_price_ps": "500",
      "mc_bit_price_ps": "1000",
      "mc_cell_price_ps": "500000"
    }
  ],
  "p20": {
    "flat_gas_limit": "1000",
    "flat_gas_price": "10000000",
    "gas_price": "655360000",
    "gas_limit": "1000000",
    "special_gas_limit": "100000000",
    "gas_credit": "10000",
    "block_gas_limit": "11000000",
    "freeze_due_limit": "100000000",
    "delete_due_limit": "1000000000"
  },
  "p21": {
    "flat_gas_limit": "1000",
    "flat_gas_price": "1000000",
    "gas_price": "65536000",
    "gas_limit": "1000000",
    "special_gas_limit": "1000000",
    "gas_credit": "10000",
    "block_gas_limit": "10000000",
    "freeze_due_limit": "100000000",
    "delete_due_limit": "1000000000"
  },
  "p22": {
    "bytes": {
      "underload": 131072,
      "soft_limit": 524288,
      "hard_limit": 1048576
    },
    "gas": {
      "underload": 900000,
      "soft_limit": 1200000,
      "hard_limit": 2000000
    },
    "lt_delta": {
      "underload": 1000,
      "soft_limit": 5000,
      "hard_limit": 10000
    }
  },
  "p23": {
    "bytes": {
      "underload": 131072,
      "soft_limit": 524288,
      "hard_limit": 1048576
    },
    "gas": {
      "underload": 900000,
      "soft_limit": 1200000,
      "hard_limit": 2000000
    },
    "lt_delta": {
      "underload": 1000,
      "soft_limit": 5000,
      "hard_limit": 10000
    }
  },
  "p24": {
    "lump_price": "10000000",
    "bit_price": "655360000",
    "cell_price": "65536000000",
    "ihr_price_factor": 98304,
    "first_frac": 21845,
    "next_frac": 21845
  },
  "p25": {
    "lump_price": "1000000",
    "bit_price": "65536000",
    "cell_price": "6553600000",
    "ihr_price_factor": 98304,
    "first_frac": 21845,
    "next_frac": 21845
  },
  "p28": {
    "shuffle_mc_validators": true,
    "mc_catchain_lifetime": 250,
    "shard_catchain_lifetime": 250,
    "shard_validators_lifetime": 1000,
    "shard_validators_num": 7
  },
  "p29": {
    "new_catchain_ids": true,
    "round_candidates": 3,
    "next_candidate_delay_ms": 2000,
    "consensus_timeout_ms": 16000,
    "fast_attempts": 3,
    "attempt_duration": 8,
    "catchain_max_deps": 4,
    "max_block_bytes": 2097152,
    "max_collated_bytes": 2097152
  },
  "p31": [
    "0000000000000000000000000000000000000000000000000000000000000000",
    "04f64c6afbff3dd10d8ba6707790ac9670d540f37a9448b0337baa6a5a92acac",
    "3333333333333333333333333333333333333333333333333333333333333333"
  ],
  "p34": {
    "utime_since": 1605687562,
    "utime_until": 1605698362,
    "total": 7,
    "main": 7,
    "total_weight": "119",
    "list": [
      {
        "public_key": "5457fef5bf496f65ea64d1d8bb4a90694f61fe2787cdb67d16f9ffe548d0b8d9",
        "weight": "17"
      }
    ]
  }
}"#;
    let map = serde_json::from_str::<serde_json::Map<String, Value>>(json)
        .map_err(|e| format!("Failed to parse config params as json: {e}"))?;
    let config_params = ton_block_json::parse_config(&map)
        .map_err(|e| format!("Failed to parse config params: {e}"))?;
    BlockchainConfig::with_config(config_params)
        .map_err(|e| format!("Failed to construct default config: {e}"))
}

// loads blockchain config from the config contract boc, if it is none tries to load config contract
// from the network, if it is unavailable returns default.
pub async fn get_blockchain_config(cli_config: &Config, config_contract_boc_path: Option<&str>) ->
    Result<BlockchainConfig, String> {
    match config_contract_boc_path {
        Some(config_path) => {
            let acc = Account::construct_from_file(config_path)
                .map_err(|e| format!("Failed to load config contract account from file {config_path}: {e}"))?;
            construct_blockchain_config(&acc)
        },
        None => {
            let ton_client = create_client(cli_config)?;
            let config = query_account_field(
                ton_client.clone(),
                CONFIG_ADDR,
                "boc",
            ).await;
            let config_account = config.and_then(|config|
                Account::construct_from_base64(&config)
                    .map_err(|e| format!("Failed to construct config account: {e}")));
            match config_account {
                Ok(config) => construct_blockchain_config(&config),
                Err(_) => blockchain_config_from_default_json()
            }
        }
    }
}

pub fn decode_data(data: &str, param_name: &str) -> Result<Vec<u8>, String> {
    if let Ok(data) = base64::decode(data) {
        Ok(data)
    } else if let Ok(data) = hex::decode(data) {
        Ok(data)
    } else {
        Err(format!("the {} parameter should be base64 or hex encoded", param_name))
    }
}
