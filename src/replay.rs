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

use std::{
    fs::File,
    io::{self, BufRead, Lines, Write, Read},
    process::exit,
    sync::{Arc, atomic::AtomicU64}
};
use clap::ArgMatches;
use failure::err_msg;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use ton_block::{Account, ConfigParams, Deserializable, Message, Serializable,
                Transaction, TransactionDescr, Block, HashmapAugType};
use ton_client::{
    net::{AggregationFn, FieldAggregation, OrderBy, ParamsOfAggregateCollection,
          ParamsOfQueryCollection, SortDirection, aggregate_collection, query_collection},
};
use ton_executor::{BlockchainConfig, ExecuteParams, OrdinaryTransactionExecutor,
                   TickTockTransactionExecutor, TransactionExecutor};
use ton_types::{BuilderData, SliceData, UInt256, write_boc};
use ton_vm::executor::{Engine, EngineTraceInfo};

use crate::config::Config;
use crate::helpers::{create_client, get_blockchain_config};

pub static CONFIG_ADDR: &str  = "-1:5555555555555555555555555555555555555555555555555555555555555555";

pub const DUMP_NONE:  u8 = 0x00;
pub const DUMP_ACCOUNT:  u8 = 0x01;
pub const DUMP_CONFIG:   u8 = 0x02;
pub const DUMP_EXECUTOR_CONFIG: u8 = 0x04;
pub const DUMP_ALL:   u8 = 0xFF;

pub fn construct_blockchain_config(config_account: &Account) -> Result<BlockchainConfig, String> {
    construct_blockchain_config_err(config_account).map_err(|e| format!("Failed to construct config: {}", e))
}

fn construct_blockchain_config_err(config_account: &Account) -> ton_types::Result<BlockchainConfig> {
    let config_cell = config_account
        .get_data().ok_or(err_msg("Failed to get account's data"))?
        .reference(0).ok();
    let config_params = ConfigParams::with_address_and_params(
        UInt256::with_array([0x55; 32]), config_cell);
    BlockchainConfig::with_config(config_params)
}

pub async fn fetch(config: &Config, account_address: &str, filename: &str, lt_bound: Option<u64>, rewrite_file: bool) -> Result<(), String> {
    if !rewrite_file && std::path::Path::new(filename).exists() {
        if !config.is_json {
            println!("File exists");
        }
        return Ok(())
    }
    let context = create_client(config)?;

    let filter = if let Some(lt_bound) = lt_bound {
        let lt_bound = format!("0x{:x}", lt_bound);
        serde_json::json!({
            "account_addr": {
                "eq": account_address
            },
            "lt": { "le": lt_bound },
        })
    } else {
        serde_json::json!({
            "account_addr": {
                "eq": account_address
            },
        })
    };

    let tr_count = aggregate_collection(
        context.clone(),
        ParamsOfAggregateCollection {
            collection: "transactions".to_owned(),
            filter: Some(filter),
            fields: Some(vec![
                FieldAggregation {
                    field: "fn".to_owned(),
                    aggregation_fn: AggregationFn::COUNT
                },
            ]),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| format!("Failed to fetch txns count: {}", e))?;
    let tr_count = u64::from_str_radix(
        tr_count.values.as_array().ok_or("Failed to parse value".to_string())?
        .get(0).ok_or("Failed to parse value".to_string())?
        .as_str().ok_or("Failed to parse value".to_string())?, 10)
        .map_err(|e| format!("Failed to parse decimal int: {}", e))?;

    let file = File::create(filename)
        .map_err(|e| format!("Failed to create file: {}", e))?;
    let mut writer = std::io::LineWriter::new(file);

    let zerostates = query_collection(
        context.clone(),
        ParamsOfQueryCollection {
            collection: "zerostates".to_owned(),
            filter: None,
            result: "accounts { id boc }".to_owned(),
            limit: Some(1),
            order: None,
            ..Default::default()
        },
    )
    .await;

    let mut zerostate_found = false;
    if let Ok(zerostates) = zerostates {
        let result = &zerostates.result.to_vec();
        let accounts = result[0]["accounts"].as_array().ok_or("Failed to parse value".to_string())?;
        for account in accounts {
            if account["id"] == account_address {
                let data = format!("{}\n", account);
                writer.write_all(data.as_bytes()).map_err(|e| format!("Failed to write to file: {}", e))?;
                zerostate_found = true;
                break;
            }
        }
    }

    if !zerostate_found {
        if !config.is_json {
            println!("account {}: zerostate not found, writing out default initial state", account_address);
        }
        let data = format!("{{\"id\":\"{}\",\"boc\":\"{}\"}}\n",
            account_address, base64::encode(&Account::default().write_to_bytes()
                .map_err(|e| format!("failed to serialize account: {}", e))?));
        writer.write_all(data.as_bytes()).map_err(|e| format!("Failed to write to file: {}", e))?;
    }
    let retry_strategy =
        tokio_retry::strategy::ExponentialBackoff::from_millis(10).take(5);

    let mut count = 0u64;
    let pb = indicatif::ProgressBar::new(tr_count);
    let mut lt = String::from("0x0");
    loop {
        let action = || async {
            let filter = if let Some(lt_bound) = lt_bound {
                let lt_bound = format!("0x{:x}", lt_bound);
                serde_json::json!({
                    "account_addr": {
                        "eq": account_address
                    },
                    "lt": { "gt": lt, "le": lt_bound },
                })
            } else {
                serde_json::json!({
                    "account_addr": {
                        "eq": account_address
                    },
                    "lt": { "gt": lt },
                })
            };
            let query = query_collection(
                context.clone(),
                ParamsOfQueryCollection {
                    collection: "transactions".to_owned(),
                    filter: Some(filter),
                    result: "id lt block { start_lt } boc".to_owned(),
                    limit: None,
                    order: Some(vec![
                        OrderBy { path: "lt".to_owned(), direction: SortDirection::ASC }
                    ]),
                    ..Default::default()
                },
            );
            query.await
        };

        let transactions = tokio_retry::Retry::spawn(retry_strategy.clone(), action).await
            .map_err(|e| format!("Failed to fetch transactions: {}", e))?;

        if transactions.result.is_empty() {
            break;
        }

        for txn in &transactions.result {
            let data = format!("{}\n", txn);
            writer.write_all(data.as_bytes()).map_err(|e| format!("Failed to write to file: {}", e))?;
        }

        let last = transactions.result.last().ok_or("Failed to get last txn".to_string())?;
        lt = last["lt"].as_str().ok_or("Failed to parse value".to_string())?.to_owned();
        count += transactions.result.len() as u64;
        pb.set_position(std::cmp::min(count, tr_count));
    }
    Ok(())
}

struct TransactionExt {
    id: String,
    block_lt: u64,
    tr: Transaction,
}

struct State {
    account: Account,
    account_addr: String,
    tr: Option<TransactionExt>,
    lines: Option<Lines<std::io::BufReader<File>>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            account: Account::default(),
            account_addr: "".to_string(),
            tr: None,
            lines: None
        }
    }
}

impl State {
    fn new(filename: &str) -> Result<Self, String> {
        let file = File::open(filename)
            .map_err(|e| format!("failed to open file {}: {}", filename, e))?;
        let mut lines = io::BufReader::new(file).lines();

        let first_line = lines.next()
            .ok_or("file is empty")?
            .map_err(|e| format!("failed to read first line: {}", e))?;
        let value = serde_json::from_str::<Value>(first_line.as_str())
            .map_err(|e| format!("failed to deserialize value: {}", e))?;
        let boc = value["boc"].as_str().ok_or("failed to decode boc")?;
        let account = Account::construct_from_base64(boc)
            .map_err(|e| format!("failed to load account from the boc: {}", e))?;
        let account_addr = String::from(value["id"].as_str()
                                            .ok_or("failed to load account address")?);

        Ok(Self { account, account_addr, tr: None, lines: Some(lines) })
    }

    pub fn next_transaction(&mut self) -> Option<()> {
        if self.lines.is_some() {
            match self.lines.as_mut().unwrap().next() {
                Some(res) => {
                    let value = serde_json::from_str::<Value>(res.ok()?.as_str()).ok()?;
                    let id = String::from(value["id"].as_str()?);
                    let boc = value["boc"].as_str()?;
                    let tr = Transaction::construct_from_base64(boc).ok()?;
                    let block_lt = u64::from_str_radix(&value["block"]["start_lt"].as_str()?[2..], 16).ok()?;
                    self.tr = Some(TransactionExt { id, block_lt, tr });
                }
                None => {
                    self.tr = None;
                }
            }
        }
        Some(())
    }
}

fn choose<'a>(st1: &'a mut State, st2: &'a mut State) -> &'a mut State {
    let lt1 = st1.tr.as_ref().map_or(u64::MAX, |tr| tr.tr.logical_time());
    let lt2 = st2.tr.as_ref().map_or(u64::MAX, |tr| tr.tr.logical_time());
    if lt1 <= lt2 {
        st1
    } else {
        st2
    }
}

pub async fn replay(
    input_filename: &str,
    config_filename: &str,
    txnid: &str,
    trace_callback: Option<Arc<dyn Fn(&Engine, &EngineTraceInfo) + Send + Sync>>,
    init_trace_last_logger: impl FnOnce() -> Result<(), String>,
    dump_mask: u8,
    cli_config: &Config,
    blockchain_config: Option<BlockchainConfig>,
) -> Result<Transaction, String> {
    let mut account_state = State::new(input_filename)?;
    let account_address = account_state.account_addr.clone();
    let mut iterate_config = true;
    let (mut config, mut config_state) = if let Some(bc_config) = blockchain_config {
        iterate_config = false;
        (bc_config, State::default())
    } else {
        let config_state = State::new(config_filename)?;
        assert_eq!(config_state.account_addr, CONFIG_ADDR);
        (BlockchainConfig::default(), config_state)
    };
    let mut cur_block_lt = 0u64;

    loop {
        if account_state.tr.is_none() {
            account_state.next_transaction();
            if account_state.tr.is_none() {
                break;
            }
        }
        if config_state.tr.is_none() {
            config_state.next_transaction();
        }

        let config_account = config_state.account.clone();
        let state = choose(&mut account_state, &mut config_state);
        let tr = state.tr.as_ref().ok_or("failed to obtain state transaction")?;

        if iterate_config {
            if cur_block_lt == 0 || cur_block_lt != tr.block_lt {
                assert!(tr.block_lt > cur_block_lt);
                cur_block_lt = tr.block_lt;
                config = construct_blockchain_config(&config_account)?;
            }
        }

        let mut account_root = state.account.serialize()
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        let account_old_hash_local = account_root.repr_hash();
        let account_old_hash_remote = tr.tr.read_state_update()
            .map_err(|e| format!("failed to read state update: {}", e))?.old_hash;
        if account_old_hash_local != account_old_hash_remote {
            if !cli_config.is_json {
                println!("FAILURE\nOld hashes mismatch:\nremote {}\nlocal  {}",
                         account_old_hash_remote.to_hex_string(),
                         account_old_hash_local.to_hex_string());
            }
            exit(1);
        }
        if tr.id == txnid {
            if dump_mask & DUMP_ACCOUNT != 0 {
                let path = format!("{}-{}.boc", account_address.split(':').last().unwrap_or(""), txnid);
                account_root.write_to_file(&path).map_err(|e| format!("Failed to write file: {}", e))?;
                if !cli_config.is_json {
                    println!("Contract account was dumped to {}", path);
                }
            }
            if dump_mask & DUMP_CONFIG != 0 {
                let path = format!("config-{}.boc", txnid);
                let account = config_account.serialize()
                    .map_err(|e| format!("Failed to serialize config account: {}", e))?;
                account.write_to_file(&path).map_err(|e| format!("Failed to write file: {}", e))?;
                if !cli_config.is_json {
                    println!("Config account was dumped to {}", path);
                }
            }
            if dump_mask & DUMP_EXECUTOR_CONFIG != 0 {
                // config.boc suitable for creating ton-labs-executor tests
                let cell = config_account.get_data()
                    .ok_or("Failed to get config data")?;
                let mut config_data = SliceData::load_cell(cell)
                    .map_err(|e| format!("Failed to load config data cell: {}", e))?;
                let mut cfg = BuilderData::default();
                cfg.append_raw(&config_data.get_next_bytes(32)
                    .map_err(|e| format!("Failed to read config data: {}", e))?, 256)
                    .map_err(|e| format!("Failed to append config data: {}", e))?;
                let cell = config_data.reference(0)
                    .map_err(|e| format!("Failed to get config zero reference: {}", e))?;
                cfg.checked_append_reference(cell)
                    .map_err(|e| format!("Failed to append config reference: {}", e))?;
                let path = format!("config-{}-test.boc", txnid);
                cfg.into_cell().map_err(|e| format!("Failed to finalize builder: {}", e))?
                    .write_to_file(&path).map_err(|e| format!("Failed to write file: {}", e))?;
                if !cli_config.is_json {
                    println!("Config for executor was dumped to {}", path);
                }
            }
            if trace_callback.is_some() {
                init_trace_last_logger()?;
                let executor = Box::new(OrdinaryTransactionExecutor::new(config.clone()));
                let msg = tr.tr.in_msg_cell().map(|c| Message::construct_from_cell(c)
                    .map_err(|e| format!("failed to construct message: {}", e))).transpose()?;
                let params = ExecuteParams {
                    block_unixtime: tr.tr.now(),
                    block_lt: tr.tr.logical_time(),
                    last_tr_lt: Arc::new(AtomicU64::new(tr.tr.logical_time())),
                    trace_callback,
                    ..ExecuteParams::default()
                };
                let tr = executor.execute_with_libs_and_params(
                    msg.as_ref(),
                    &mut account_root,
                    params).map_err(|e| format!("Failed to execute txn: {}", e))?;
                return Ok(tr);
            }
        }
        let executor: Box<dyn TransactionExecutor> =
            match tr.tr.read_description()
                .map_err(|e| format!("failed to read transaction: {}", e))? {
                TransactionDescr::TickTock(desc) => {
                    Box::new(TickTockTransactionExecutor::new(config.clone(), desc.tt))
                }
                TransactionDescr::Ordinary(_) => {
                    Box::new(OrdinaryTransactionExecutor::new(config.clone()))
                }
                _ => {
                    panic!("Unknown transaction type");
                }
            };

        let msg = tr.tr.in_msg_cell().map(|c| Message::construct_from_cell(c)
            .map_err(|e| format!("failed to construct message: {}", e))).transpose()?;

        let params = ExecuteParams {
            block_unixtime: tr.tr.now(),
            block_lt: tr.tr.logical_time(),
            last_tr_lt: Arc::new(AtomicU64::new(tr.tr.logical_time())),
            ..ExecuteParams::default()
        };
        let tr_local = executor.execute_with_libs_and_params(
            msg.as_ref(),
            &mut account_root,
            params).map_err(|e| format!("Failed to execute txn: {}", e))?;
        state.account = Account::construct_from_cell(account_root.clone())
            .map_err(|e| format!("Failed to construct account: {}", e))?;

        let account_new_hash_local = tr_local.read_state_update()
            .map_err(|e| format!("failed to read state update: {}", e))?
            .new_hash;
        let account_new_hash_remote = tr.tr.read_state_update()
            .map_err(|e| format!("failed to read state update: {}", e))?
            .new_hash;
        if account_new_hash_local != account_new_hash_remote {
            if !cli_config.is_json {
                println!("FAILURE\nNew hashes mismatch:\nremote {}\nlocal  {}\nTR id: {}",
                         account_new_hash_remote.to_hex_string(),
                         account_new_hash_local.to_hex_string(), tr.id);
            }
            let local_desc = tr_local.read_description()
                .map_err(|e| format!("failed to read description: {}", e))?;
            let remote_desc = tr.tr.read_description()
                .map_err(|e| format!("failed to read description: {}", e))?;
            assert_eq!(remote_desc, local_desc);
            exit(2);
        }

        if tr.id == txnid {
            if !cli_config.is_json {
                println!("DONE");
            } else {
                println!("{{}}");
            }
            return Ok(tr_local);
        }
        state.tr = None;
    }
    Err("Specified transaction was not found.".to_string())
}

pub async fn fetch_block(config: &Config, block_id: &str, filename: &str) -> ton_types::Status {
    let context = create_client(config)
        .map_err(|e| err_msg(format!("Failed to create ctx: {}", e)))?;

    let block = query_collection(
        context.clone(),
        ParamsOfQueryCollection {
            collection: "blocks".to_owned(),
            filter: Some(serde_json::json!({
                "id": {
                    "eq": block_id
                },
            })),
            result: "workchain_id end_lt boc".to_owned(),
            limit: None,
            order: None,
            ..Default::default()
        },
    ).await?;

    if block.result.len() != 1 {
        return Err(err_msg("Failed to fetch the block"))
    }

    let mut accounts = vec!();

    let wid = block.result.get(0).unwrap()["workchain_id"].as_i64().unwrap();
    let end_lt = block.result.get(0).unwrap()["end_lt"].as_str().unwrap().trim_start_matches("0x");
    let end_lt = u64::from_str_radix(end_lt, 16).unwrap();
    let block = Block::construct_from_base64(block.result.get(0).unwrap()["boc"].as_str().unwrap())?;
    let extra = block.read_extra()?;
    let account_blocks = extra.read_account_blocks()?;

    account_blocks.iterate_objects(|account_block| {
        let mut slice = account_block.account_id().clone();
        let id = UInt256::construct_from(&mut slice)?;
        let account_name = format!("{}:{}", wid, id.to_hex_string());
        let mut txns = vec!();
        account_block.transaction_iterate(|tr| {
            let cell = tr.serialize()?;
            let bytes = write_boc(&cell)?;
            txns.push((cell.repr_hash().to_hex_string(), base64::encode(&bytes)));
            Ok(true)
        })?;
        accounts.push((account_name, txns));
        Ok(true)
    })?;

    if accounts.is_empty() {
        return Err(err_msg("The block is empty"))
    }

    for (account, _) in &accounts {
        println!("Fetching transactions of {}", account);
        fetch(config,
            account.as_str(),
            format!("{}.txns", account).as_str(),
            Some(end_lt), false).await.map_err(err_msg)?;
    }

    let config_txns_path = format!("{}.txns", CONFIG_ADDR);
    if !std::path::Path::new(config_txns_path.as_str()).exists() {
        println!("Fetching transactions of {}", CONFIG_ADDR);
        fetch(config,
            CONFIG_ADDR,
            config_txns_path.as_str(),
            Some(end_lt), false).await.map_err(err_msg)?;
    }

    let acc = accounts[0].0.as_str();
    let txnid = accounts[0].1[0].0.as_str();

    let config_path = format!("config-{}.boc", txnid);
    if !std::path::Path::new(config_path.as_str()).exists() {
        println!("Computing config: replaying {} up to {}", acc, txnid);
        replay(format!("{}.txns", acc).as_str(),config_txns_path.as_str(),
               txnid, None, || Ok(()), DUMP_CONFIG,
               config, None
        ).await.map_err(err_msg)?;
    } else {
        println!("Using pre-computed config {}", config_path);
    }

    println!("Pre-replaying block accounts");
    let tasks: Vec<_> = accounts.iter().map(|(account, txns)| {
        let account_filename = account.split(':').last().unwrap_or("").to_owned();
        let _config = config.clone().to_owned();
        let txnid = txns[0].0.clone();
        tokio::spawn(async move {
            if !std::path::Path::new(format!("{}-{}.boc", account_filename, txnid).as_str()).exists() {
                replay(
                    format!("{}.txns", account_filename).as_str(),
                    format!("{}.txns", CONFIG_ADDR).as_str(),
                    &txnid,
                    None,
                    || Ok(()),
                    DUMP_ACCOUNT,
                    &_config,
                    None,
                ).await.map_err(err_msg).unwrap();
            }
        })
    }).collect();
    futures::future::join_all(tasks).await;

    println!("Writing block");
    let mut config_data = Vec::new();
    let mut config_file = File::open(format!("config-{}.boc", txnid))?;
    config_file.read_to_end(&mut config_data)?;

    let mut block = BlockDescr {
        id: block_id.to_string(),
        config_boc: base64::encode(&config_data),
        accounts: vec!(),
    };

    for (account, txns) in &accounts {
        let txnid = txns[0].0.as_str();
        let mut account_file = File::open(format!("{}-{}.boc", account, txnid))?;
        let mut account_data = Vec::new();
        account_file.read_to_end(&mut account_data)?;
        let mut transactions = vec!();
        for (_, txn) in txns {
            transactions.push(txn.clone());
        }
        block.accounts.push(BlockAccountDescr {
            account_boc: base64::encode(&account_data),
            transactions,
        });
    }

    let writer = std::io::BufWriter::new(File::create(filename)?);
    serde_json::to_writer_pretty(writer, &block)?;
    println!("Block written to {}", filename);
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct BlockDescr {
    id: String,
    config_boc: String,
    accounts: Vec<BlockAccountDescr>,
}

#[derive(Serialize, Deserialize)]
struct BlockAccountDescr {
    account_boc: String,
    transactions: Vec<String>,
}

pub async fn fetch_block_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    fetch_block(config,
        m.value_of("BLOCKID").ok_or("Missing block id")?,
        m.value_of("OUTPUT").ok_or("Missing output filename")?
    ).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn fetch_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    fetch(config,
        m.value_of("ADDRESS").ok_or("Missing account address")?,
        m.value_of("OUTPUT").ok_or("Missing output filename")?,
        None,
        true
    ).await?;
    if config.is_json {
        println!("{{}}");
    } else {
        println!("Succeeded");
    }
    Ok(())
}

pub async fn replay_command(m: &ArgMatches<'_>, cli_config: &Config) -> Result<(), String> {
    let (config_txns, bc_config) = if m.is_present("DEFAULT_CONFIG") {
        ("", Some(get_blockchain_config(cli_config, None).await?))
    } else {
        (m.value_of("CONFIG_TXNS").ok_or("Missing config txns filename")?, None)
    };
    let _ = replay(m.value_of("INPUT_TXNS").ok_or("Missing input txns filename")?,
        config_txns, m.value_of("TXNID").ok_or("Missing final txn id")?,
        None, ||{Ok(())}, DUMP_ALL, cli_config, bc_config
    ).await?;
    Ok(())
}
