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
use crate::{contract_data_from_matches_or_config_alias, FullConfig, print_args,
            unpack_alternative_params};
use clap::{ArgMatches, SubCommand, Arg, App};
use crate::config::Config;
use crate::helpers::{load_ton_address, create_client, load_abi, now_ms, construct_account_from_tvc,
                     query_account_field, query_with_limit, create_client_verbose,
                     abi_from_matches_or_config, load_debug_info, wc_from_matches_or_config,
                     TEST_MAX_LEVEL, MAX_LEVEL, get_blockchain_config};
use crate::replay::{
    fetch, CONFIG_ADDR, replay, DUMP_NONE, DUMP_CONFIG, DUMP_ACCOUNT
};
use std::io::{Write, BufRead};
use std::collections::{HashSet, HashMap};
use ton_block::{Message, Account, Serializable, Deserializable, OutMessages, Transaction,
                MsgAddressInt, CurrencyCollection, GasLimitsPrices, ConfigParamEnum};
use ton_types::{UInt256, HashmapE, Cell, AccountId};
use ton_client::abi::{CallSet, Signer, FunctionHeader, encode_message, ParamsOfEncodeMessage};
use ton_executor::{
    BlockchainConfig, ExecuteParams, OrdinaryTransactionExecutor, TransactionExecutor
};
use std::sync::{Arc, atomic::AtomicU64};
use ton_client::net::{OrderBy, ParamsOfQueryCollection, query_collection, SortDirection};
use crate::crypto::load_keypair;
use std::fmt;
use std::fs::File;
use serde_json::{Value, json};
use ton_labs_assembler::DbgInfo;
use ton_vm::executor::{Engine, EngineTraceInfo, EngineTraceInfoType};
use crate::decode::msg_printer::serialize_msg;
use crate::deploy::prepare_deploy_message;

const DEFAULT_TRACE_PATH: &str = "./trace.log";
const DEFAULT_CONFIG_PATH: &str = "config.txns";
const DEFAULT_CONTRACT_PATH: &str = "contract.txns";
const TRANSACTION_QUANTITY: u32 = 10;


pub struct DebugLogger {
    tvm_trace: String,
    ordinary_log_level: log::LevelFilter,
}

impl DebugLogger {
    pub fn new(path: String) -> Self {
        if std::path::Path::new(&path).exists() {
            std::fs::remove_file(&path)
                .expect("Failed to remove old trace log.");
        }

        DebugLogger {
            tvm_trace: path,
            ordinary_log_level: if std::env::var("RUST_LOG")
                .unwrap_or_default()
                .eq_ignore_ascii_case("debug")
            {
                TEST_MAX_LEVEL
            } else {
                MAX_LEVEL
            },
        }
    }
}

impl log::Log for DebugLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        match record.target() {
            "tvm" | "executor" => {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .append(true)
                    .create(true)
                    .open(&self.tvm_trace)
                    .as_mut()
                {
                    Ok(file) => {
                        let _ = file.write(format!("{}\n", record.args()).as_bytes())
                            .expect("Failed to write trace");
                    }
                    Err(_) => {
                        println!("{}", record.args());
                    }
                }
            }
            _ => {
                if record.level() <= self.ordinary_log_level {
                    match record.level() {
                        log::Level::Error | log::Level::Warn => {
                            eprintln!("{}", record.args());
                        }
                        _ => {
                            println!("{}", record.args());
                        }
                    }
                }
            }
        }
    }

    fn flush(&self) {}
}

pub fn create_debug_command<'a, 'b>() -> App<'a, 'b> {
    let output_arg = Arg::with_name("LOG_PATH")
        .help("Path where to store the trace. Default path is \"./trace.log\". Note: old file will be removed.")
        .takes_value(true)
        .long("--output")
        .short("-o");

    let dbg_info_arg = Arg::with_name("DBG_INFO")
        .help("Path to the file with debug info.")
        .takes_value(true)
        .long("--dbg_info")
        .short("-d");

    let address_arg = Arg::with_name("ADDRESS")
        .long("--addr")
        .takes_value(true)
        .help("Contract address or path the file with saved contract state if corresponding flag is used. Can be specified in th config file.");

    let method_arg = Arg::with_name("METHOD")
        .long("--method")
        .short("-m")
        .takes_value(true)
        .help("Name of the function being called. Can be specified in the config file.");

    let params_arg = Arg::with_name("PARAMS")
        .help("Function arguments. Must be a list of `--name value` pairs, a json string with all arguments or path to the file with parameters. Can be specified in the config file.")
        .multiple(true);

    let sign_arg = Arg::with_name("KEYS")
        .long("--keys")
        .takes_value(true)
        .help("Seed phrase or path to the file with keypair used to sign the message. Can be specified in the config.");

    let abi_arg = Arg::with_name("ABI")
        .long("--abi")
        .takes_value(true)
        .help("Path or link to the contract ABI file or pure json ABI data. Can be specified in the config file.");

    let decode_abi_arg = Arg::with_name("DECODE_ABI")
        .long("--decode_abi")
        .takes_value(true)
        .help("Path to the ABI file used to decode output messages. Can be specified in the config file.");

    let full_trace_arg = Arg::with_name("FULL_TRACE")
        .long("--full_trace")
        .help("Flag that changes trace to full version.");

    let boc_arg = Arg::with_name("BOC")
        .long("--boc")
        .conflicts_with("TVC")
        .help("Flag that changes behavior of the command to work with the saved account state (account BOC).");

    let tx_id_arg = Arg::with_name("TX_ID")
        .required(true)
        .takes_value(true)
        .help("ID of the transaction that should be replayed.");

    let config_path_arg = Arg::with_name("CONFIG_PATH")
        .help("Path to the file with saved config contract state.")
        .long("--config")
        .short("-c")
        .takes_value(true);

    let default_config_arg = Arg::with_name("DEFAULT_CONFIG")
        .help("Execute debug with current blockchain config or default if it is not available.")
        .long("--default_config")
        .short("-e")
        .conflicts_with_all(&["CONFIG_PATH", "CONFIG_BOC"]);

    let config_save_path_arg = Arg::with_name("CONFIG_PATH")
        .help("Path to the file with saved config contract transactions. If not set and config contract state is not specified with other options transactions will be fetched to file \"config.txns\".")
        .long("--config")
        .short("-c")
        .takes_value(true)
        .conflicts_with_all(&["DEFAULT_CONFIG", "CONFIG_BOC"]);

    let contract_path_arg = Arg::with_name("CONTRACT_PATH")
        .help("Path to the file with saved target contract transactions. If not set transactions will be fetched to file \"contract.txns\".")
        .long("--contract")
        .short("-t")
        .takes_value(true);

    let dump_config_arg = Arg::with_name("DUMP_CONFIG")
        .help("Dump the replayed config contract account state.")
        .long("--dump_config")
        .conflicts_with("CONFIG_BOC");

    let dump_contract_arg = Arg::with_name("DUMP_CONTRACT")
        .help("Dump the replayed target contract account state.")
        .long("--dump_contract");

    let update_arg = Arg::with_name("UPDATE_BOC")
        .long("--update")
        .short("-u")
        .requires("BOC")
        .help("Update contract BOC after execution");

    let now_arg = Arg::with_name("NOW")
        .takes_value(true)
        .long("--now")
        .help("Now timestamp (in milliseconds) for execution. If not set it is equal to the current timestamp.");

    let msg_cmd = SubCommand::with_name("message")
        .about("Play message locally with trace")
        .arg(output_arg.clone())
        .arg(dbg_info_arg.clone())
        .arg(address_arg.clone())
        .arg(full_trace_arg.clone())
        .arg(decode_abi_arg.clone())
        .arg(boc_arg.clone())
        .arg(config_path_arg.clone())
        .arg(update_arg.clone())
        .arg(now_arg.clone())
        .arg(Arg::with_name("MESSAGE")
            .takes_value(true)
            .required(true)
            .help("Message in Base64 or path to file with message.")
        );

    let run_cmd = SubCommand::with_name("run")
        .about("Play getter locally with trace")
        .arg(output_arg.clone())
        .arg(dbg_info_arg.clone())
        .arg(address_arg.clone())
        .arg(method_arg.clone())
        .arg(params_arg.clone())
        .arg(abi_arg.clone())
        .arg(full_trace_arg.clone())
        .arg(decode_abi_arg.clone())
        .arg(boc_arg.clone())
        .arg(Arg::with_name("TVC")
            .long("--tvc")
            .conflicts_with("BOC")
            .help("Flag that changes behavior of the command to work with the saved contract state (stateInit TVC)."))
        .arg(Arg::with_name("ACCOUNT_ADDRESS")
            .takes_value(true)
            .long("--tvc_address")
            .help("Account address for account constructed from TVC.")
            .requires("TVC"))
        .arg(now_arg.clone())
        .arg(config_path_arg.clone());

    let deploy_cmd = SubCommand::with_name("deploy")
        .about("Play deploy locally with trace")
        .arg(output_arg.clone())
        .arg(dbg_info_arg.clone())
        .arg(abi_arg.clone())
        .arg(full_trace_arg.clone())
        .arg(decode_abi_arg.clone())
        .arg(sign_arg.clone())
        .arg(now_arg.clone())
        .arg(config_path_arg.clone())
        .arg(Arg::with_name("TVC")
            .required(true)
            .takes_value(true)
            .help("Path to the TVC file with contract stateinit."))
        .arg(Arg::with_name("WC")
            .takes_value(true)
            .long("--wc")
            .help("Workchain ID"))
        .arg(params_arg.clone())
        .arg(Arg::with_name("INIT_BALANCE")
            .long("--init_balance")
            .help("Do not fetch account from the network, but create dummy account with big balance."));

    let call_cmd = run_cmd.clone().name("call")
        .about("Play call locally with trace")
        .arg(sign_arg.clone())
        .arg(update_arg.clone());

    let config_boc_arg = Arg::with_name("CONFIG_BOC")
        .help("Path to the config contract boc.")
        .long("--config_boc")
        .takes_value(true)
        .conflicts_with_all(&["CONFIG_PATH", "DEFAULT_CONFIG"]);

    SubCommand::with_name("debug")
        .about("Debug commands.")
        .subcommand(SubCommand::with_name("transaction")
            .about("Replay transaction with specified ID.")
            .arg(default_config_arg.clone())
            .arg(config_save_path_arg.clone())
            .arg(contract_path_arg.clone())
            .arg(output_arg.clone())
            .arg(dbg_info_arg.clone())
            .arg(full_trace_arg.clone())
            .arg(decode_abi_arg.clone())
            .arg(tx_id_arg.clone())
            .arg(dump_config_arg.clone())
            .arg(dump_contract_arg.clone())
            .arg(config_boc_arg.clone()))
        .subcommand(SubCommand::with_name("account")
            .about("Loads list of the last transactions for the specified account. User should choose which one to debug.")
            .arg(default_config_arg.clone())
            .arg(config_save_path_arg.clone())
            .arg(contract_path_arg.clone())
            .arg(output_arg.clone())
            .arg(dbg_info_arg.clone())
            .arg(full_trace_arg.clone())
            .arg(decode_abi_arg.clone())
            .arg(address_arg.clone())
            .arg(dump_config_arg.clone())
            .arg(dump_contract_arg.clone())
            .arg(config_boc_arg.clone()))
        .subcommand(SubCommand::with_name("replay")
            .about("Replay transaction on the saved account state.")
            .arg(output_arg.clone())
            .arg(dbg_info_arg.clone())
            .arg(full_trace_arg.clone())
            .arg(tx_id_arg.clone())
            .arg(config_path_arg.clone())
            .arg(decode_abi_arg.clone())
            .arg(Arg::with_name("UPDATE_STATE")
                .help("Update state of the contract.")
                .long("--update")
                .short("-u"))
            .arg(Arg::with_name("INPUT")
                .help("Path to the saved account state.")
                .required(true)
                .takes_value(true)))
        .subcommand(SubCommand::with_name("sequence-diagram")
            .setting(clap::AppSettings::AllowLeadingHyphen)
            .about("Produces UML sequence diagram for provided accounts.")
            .arg(Arg::with_name("ADDRESSES")
                .required(true)
                .takes_value(true)
                .help("File with a list of addresses, one per line.")))
        .subcommand(call_cmd)
        .subcommand(run_cmd)
        .subcommand(deploy_cmd)
        .subcommand(msg_cmd)
}

pub async fn debug_command(matches: &ArgMatches<'_>, full_config: &FullConfig) -> Result<(), String> {
    let config = &full_config.config;
    if let Some(matches) = matches.subcommand_matches("transaction") {
        return debug_transaction_command(matches, config, false).await;
    }
    if let Some(matches) = matches.subcommand_matches("account") {
        return debug_transaction_command(matches, config, true).await;
    }
    if let Some(matches) = matches.subcommand_matches("call") {
        return debug_call_command(matches, full_config, false).await;
    }
    if let Some(matches) = matches.subcommand_matches("run") {
        return debug_call_command(matches, full_config, true).await;
    }
    if let Some(matches) = matches.subcommand_matches("message") {
        return debug_message_command(matches, config).await;
    }
    if let Some(matches) = matches.subcommand_matches("replay") {
        return replay_transaction_command(matches, config).await;
    }
    if let Some(matches) = matches.subcommand_matches("deploy") {
        return debug_deploy_command(matches, config).await;
    }
    if let Some(matches) = matches.subcommand_matches("sequence-diagram") {
        return sequence_diagram_command(matches, config).await;
    }
    Err("unknown command".to_owned())
}

async fn debug_transaction_command(matches: &ArgMatches<'_>, config: &Config, is_account: bool) -> Result<(), String> {
    let trace_path = Some(matches.value_of("LOG_PATH").unwrap_or(DEFAULT_TRACE_PATH));
    let config_path = matches.value_of("CONFIG_PATH");
    let contract_path = matches.value_of("CONTRACT_PATH");
    let is_default_config = matches.is_present("DEFAULT_CONFIG");
    let config_boc = matches.value_of("CONFIG_BOC");
    let (tx_id, address) = if !is_account {
        let tx_id = matches.value_of("TX_ID");
        if !config.is_json {
            print_args!(tx_id, trace_path, config_path, contract_path);
        }
        let address = query_address(tx_id.unwrap(), &config).await?;
        (tx_id.unwrap().to_string(), address)
    } else {
        let address =
            Some(matches.value_of("ADDRESS")
               .map(|s| s.to_string())
               .or(config.addr.clone())
               .ok_or("ADDRESS is not defined. Supply it in the config file or command line."
                   .to_string())?);
        if !config.is_json {
            print_args!(address, trace_path, config_path, contract_path);
        }
        let address = address.unwrap();
        let transactions = query_transactions(&address, &config).await?;
        let tr_id = choose_transaction(transactions)?;
        (tr_id, address)
    };

    let config_path = if is_default_config || config_boc.is_some() {
        ""
    } else {
        match config_path {
            Some(config_path) => {
                config_path
            },
            _ => {
                if !config.is_json {
                    println!("Fetching config contract transactions...");
                }
                fetch(config, CONFIG_ADDR, DEFAULT_CONFIG_PATH, None, true).await?;
                DEFAULT_CONFIG_PATH
            }
        }
    };
    let contract_path = match contract_path {
        Some(contract_path) => {
            contract_path
        },
        _ => {
            if !config.is_json {
                println!("Fetching contract transactions...");
            }
            fetch(config, &address, DEFAULT_CONTRACT_PATH, None, true).await?;
            DEFAULT_CONTRACT_PATH
        }
    };

    let trace_path = trace_path.unwrap().to_string();
    let init_logger = || {
        log::set_max_level(log::LevelFilter::Trace);
        log::set_boxed_logger(
            Box::new(DebugLogger::new(trace_path.clone()))
        ).map_err(|e| format!("Failed to set logger: {}", e))?;
        Ok(())
    };

    let mut dump_mask = DUMP_NONE;
    if matches.is_present("DUMP_CONFIG") {
        dump_mask |= DUMP_CONFIG;
    }
    if matches.is_present("DUMP_CONTRACT") {
        dump_mask |= DUMP_ACCOUNT;
    }
    if !config.is_json {
        println!("Replaying the transactions...");
    }

    let blockchain_config = if is_default_config || config_boc.is_some() {
        let bc_config = get_blockchain_config(config, config_boc).await?;
        Some(bc_config)
    } else {
      None
    };

    let tr = replay(
        contract_path,
        config_path,
        &tx_id,
        generate_callback(Some(matches), config),
        init_logger,
        dump_mask,
        config,
        blockchain_config
    ).await?;

    decode_messages(tr.out_msgs, load_decode_abi(matches, config), config).await?;
    if !config.is_json {
        println!("Log saved to {}.", trace_path);
    } else {
        println!("{{}}");
    }
    Ok(())
}

async fn replay_transaction_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let tx_id = matches.value_of("TX_ID");
    let config_path = matches.value_of("CONFIG_PATH");
    let output = Some(matches.value_of("LOG_PATH").unwrap_or(DEFAULT_TRACE_PATH));
    let input = matches.value_of("INPUT");
    let do_update = matches.is_present("UPDATE_STATE");

    if !config.is_json {
        print_args!(input, tx_id, output, config_path);
    }

    let ton_client = create_client(&config)?;
    let trans = query_collection(
        ton_client.clone(),
        ParamsOfQueryCollection {
            collection: "transactions".to_owned(),
            filter: Some(json!({
                "id": {
                    "eq": tx_id.unwrap()
                },
            })),
            result: "lt block { start_lt } boc".to_string(),
            limit: Some(1),
            order: None,
            ..Default::default()
        },
    ).await
        .map_err(|e| format!("Failed to query transaction: {}", e))?;

    if trans.result.is_empty() {
        return Err("Transaction with specified id was not found".to_string());
    }

    let trans = trans.result[0].clone();
    let block_lt = trans["block"]["start_lt"].as_str()
        .ok_or("Failed to parse block_lt.".to_string())?;
    let block_lt = u64::from_str_radix(&block_lt[2..], 16)
        .map_err(|e| format!("Failed to convert block_lt: {}", e))?;
    let boc = trans["boc"].as_str()
        .ok_or("Failed to parse boc.".to_string())?;

    let trans = Transaction::construct_from_base64(boc)
        .map_err(|e| format!("Failed to parse transaction: {}", e))?;

    let mut account = Account::construct_from_file(input.unwrap())
        .map_err(|e| format!("Failed to construct account from the file: {}", e))?
        .serialize()
        .map_err(|e| format!("Failed to serialize account: {}", e))?;

    let msg = trans.in_msg_cell().map(|c| Message::construct_from_cell(c)
        .map_err(|e| format!("failed to construct message: {}", e))).transpose()?;

    log::set_max_level(log::LevelFilter::Trace);
    log::set_boxed_logger(
        Box::new(DebugLogger::new(output.unwrap().to_string()))
    ).map_err(|e| format!("Failed to set logger: {}", e))?;

    let result_trans = execute_debug(
        Some(matches),
        get_blockchain_config(config, config_path).await?,
        &mut account,
        msg.as_ref(),
        trans.now(),
        block_lt,
        trans.logical_time(),
        false,
        config
    ).await;

    if do_update && result_trans.is_ok() {
        Account::construct_from_cell(account.clone())
            .map_err(|e| format!("Failed to construct account: {}", e))?
            .write_to_file(input.unwrap())
            .map_err(|e| format!("Failed to save account state: {}", e))?;
        if !config.is_json {
            println!("Contract state was updated.");
        }
    }

    match result_trans {
        Ok(result_trans) => {
            decode_messages(result_trans.out_msgs,load_decode_abi(matches, config), config).await?;
            if !config.is_json {
                println!("Execution finished.");
            }
        }
        Err(e) => {
            if !config.is_json {
                println!("Execution failed: {}", e);
            }
        }
    }
    if !config.is_json {
        println!("Log saved to {}", output.unwrap());
    }
    Ok(())
}

fn parse_now(matches: &ArgMatches<'_>) -> Result<u64, String> {
    Ok(match matches.value_of("NOW") {
        Some(now) => u64::from_str_radix(now, 10)
            .map_err(|e| format!("Failed to convert now to u64: {}", e))?,
        _ => now_ms()
    })
}

fn load_decode_abi(matches: &ArgMatches<'_>, config: &Config) -> Option<String> {
    let abi = matches.value_of("DECODE_ABI")
        .map(|s| s.to_owned())
        .or(abi_from_matches_or_config(matches, &config).ok());
    match abi {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(res) => Some(res),
            Err(e) => {
                if !config.is_json {
                    println!("Failed to read abi: {}", e);
                }
                None
            }
        }
        _ => None
    }
}

async fn debug_call_command(matches: &ArgMatches<'_>, full_config: &FullConfig, is_getter: bool) -> Result<(), String> {
    let (input, opt_abi, sign) = contract_data_from_matches_or_config_alias(matches, full_config)?;
    let input = input.as_ref();
    let output = Some(matches.value_of("LOG_PATH").unwrap_or(DEFAULT_TRACE_PATH));
    let method = Some(matches.value_of("METHOD").or(full_config.config.method.as_deref())
        .ok_or("Method is not defined. Supply it in the config file or command line.")?);
    let is_boc = matches.is_present("BOC");
    let is_tvc = matches.is_present("TVC");
    let loaded_abi = load_abi(opt_abi.as_ref().unwrap(), &full_config.config).await?;
    let params = unpack_alternative_params(
        matches,
        opt_abi.as_ref().unwrap(),
        method.unwrap(),
        &full_config.config
    ).await?;

    if !full_config.config.is_json {
        print_args!(input, method, params, sign, opt_abi, output);
    }

    let ton_client = create_client(&full_config.config)?;  // TODO: create local for boc and tvc
    let input = input.unwrap();
    let mut account = if is_tvc {
        construct_account_from_tvc(input,
                                   matches.value_of("ACCOUNT_ADDRESS"),
                                   Some(u64::MAX))?
    } else if is_boc {
        Account::construct_from_file(input)
            .map_err(|e| format!(" failed to load account from the file {}: {}", input, e))?
    } else {
        let address = load_ton_address(input, &full_config.config)?;
        let account = query_account_field(ton_client.clone(), &address, "boc").await?;
        Account::construct_from_base64(&account)
            .map_err(|e| format!("Failed to construct account: {}", e))?
    };

    let params = serde_json::from_str(&params.unwrap())
        .map_err(|e| format!("params are not in json format: {}", e))?;

    let keys = sign.map(|k| load_keypair(&k)).transpose()?;

    let now = parse_now(matches)?;

    let header = FunctionHeader {
        expire: Some((now / 1000) as u32 + full_config.config.lifetime),
        time: Some(now),
        ..Default::default()
    };
    let call_set = CallSet {
        function_name: method.unwrap().to_string(),
        input: Some(params),
        header: Some(header)
    };
    let msg_params = ParamsOfEncodeMessage {
        abi: loaded_abi,
        address: Some(format!("0:{}", "0".repeat(64))),  // TODO: add option or get from input
        call_set: Some(call_set),
        signer: if keys.is_some() {
            Signer::Keys { keys: keys.unwrap() }
        } else {
            Signer::None
        },
        ..Default::default()
    };

    let message = encode_message(
        ton_client.clone(),
        msg_params
    ).await
        .map_err(|e| format!("Failed to encode message: {}", e))?;

    let message = Message::construct_from_base64(&message.message)
        .map_err(|e| format!("Failed to construct message: {}", e))?;

    if is_getter {
        account.set_balance(CurrencyCollection::with_grams(u64::MAX));
    }
    let mut acc_root = account.serialize()
        .map_err(|e| format!("Failed to serialize account: {}", e))?;

    let trace_path = output.unwrap().to_string();

    log::set_max_level(log::LevelFilter::Trace);
    log::set_boxed_logger(
        Box::new(DebugLogger::new(trace_path.clone()))
    ).map_err(|e| format!("Failed to set logger: {}", e))?;

    let trans = execute_debug(
        Some(matches),
        get_blockchain_config(&full_config.config, matches.value_of("CONFIG_PATH")).await?,
        &mut acc_root,
        Some(&message),
        (now / 1000) as u32,
        now,
        now,
        is_getter,
        &full_config.config,
    ).await;

    let mut out_res = vec![];
    let msg_string = match trans {
        Ok(trans) => {
            out_res = decode_messages(trans.out_msgs,load_decode_abi(matches, &full_config.config), &full_config.config).await?;
            "Execution finished.".to_string()
        }
        Err(e) => {
            if !is_getter {
                format!("Execution failed: {}", e)
            } else {
                if e.to_string().contains("Contract did not accept message") {
                    "Execution finished.".to_string()
                } else {
                    format!("Execution failed: {}", e)
                }
            }
        }
    };

    if matches.is_present("UPDATE_BOC") {
        Account::construct_from_cell(acc_root)
            .map_err(|e| format!("Failed to construct account: {}", e))?
            .write_to_file(input)
            .map_err(|e| format!("Failed to dump account: {}", e))?;
        if !full_config.config.is_json {
            println!("{} successfully updated", input);
        }
    }

    let print_res = is_getter && !out_res.is_empty();
    if print_res {
        if !full_config.config.is_json {
            print!("Output: ");
        }
        for msg in out_res {
            println!("{}", serde_json::to_string_pretty(&msg)
                .map_err(|e| format!("Failed to serialize result: {}", e))?);
        }
    }

    if !full_config.config.is_json {
        println!("{}", msg_string);
        println!("Log saved to {}", trace_path);
    } else if !print_res {
        println!("{{}}");
    }
    Ok(())
}

async fn debug_message_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let input = matches.value_of("ADDRESS");
    let output = Some(matches.value_of("LOG_PATH").unwrap_or(DEFAULT_TRACE_PATH));
    let debug_info = matches.value_of("DBG_INFO").map(|s| s.to_string());
    let is_boc = matches.is_present("BOC");
    let message = matches.value_of("MESSAGE");
    if !config.is_json {
        print_args!(input, message, output, debug_info);
    }

    let ton_client = create_client(&config)?;
    let input = input.unwrap();
    let account = if is_boc {
        Account::construct_from_file(input)
            .map_err(|e| format!(" failed to load account from the file {}: {}", input, e))?
    } else {
        let address = load_ton_address(input, &config)?;
        let account = query_account_field(ton_client.clone(), &address, "boc").await?;
        Account::construct_from_base64(&account)
            .map_err(|e| format!("Failed to construct account: {}", e))?
    };

    let message = message.unwrap();
    let message = if std::path::Path::new(message).exists() {
        Message::construct_from_file(message)
    } else {
        Message::construct_from_base64(message)
    }.map_err(|e| format!("Failed to decode message: {}", e))?;
    let mut acc_root = account.serialize()
        .map_err(|e| format!("Failed to serialize account: {}", e))?;

    let trace_path = output.unwrap().to_string();

    log::set_max_level(log::LevelFilter::Trace);
    log::set_boxed_logger(
        Box::new(DebugLogger::new(trace_path.clone()))
    ).map_err(|e| format!("Failed to set logger: {}", e))?;

    let now = parse_now(matches)?;
    let trans = execute_debug(
        Some(matches),
        get_blockchain_config(config, matches.value_of("CONFIG_PATH")).await?,
        &mut acc_root,
        Some(&message),
        (now / 1000) as u32,
        now,
        now,
        false,
        config
    ).await;

    let msg_string = match trans {
        Ok(trans) => {
            decode_messages(trans.out_msgs,load_decode_abi(matches, config), config).await?;
            "Execution finished.".to_string()
        }
        Err(e) => {
            format!("Execution failed: {}", e)
        }
    };

    if matches.is_present("UPDATE_BOC") {
        Account::construct_from_cell(acc_root)
            .map_err(|e| format!("Failed to construct account: {}", e))?
            .write_to_file(input)
            .map_err(|e| format!("Failed to dump account: {}", e))?;
        if !config.is_json {
            println!("{} successfully updated", input);
        }
    }

    if !config.is_json {
        println!("{}", msg_string);
        println!("Log saved to {}", trace_path);
    } else {
        println!("{{}}");
    }
    Ok(())
}

async fn debug_deploy_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let tvc = matches.value_of("TVC");
    let output = Some(matches.value_of("LOG_PATH").unwrap_or(DEFAULT_TRACE_PATH));
    let opt_abi = Some(abi_from_matches_or_config(matches, &config)?);
    let debug_info = matches.value_of("DBG_INFO").map(|s| s.to_string())
        .or(load_debug_info(opt_abi.as_ref().unwrap()));
    let sign = matches.value_of("KEYS")
        .map(|s| s.to_string())
        .or(config.keys_path.clone());
    let params = unpack_alternative_params(
        matches,
        opt_abi.as_ref().unwrap(),
        "constructor",
        config
    ).await?;
    let wc = Some(wc_from_matches_or_config(matches, config)?);
    if !config.is_json {
        print_args!(tvc, params, sign, opt_abi, output, debug_info);
    }

    let (msg, address) = prepare_deploy_message(
        tvc.unwrap(),
        opt_abi.as_ref().unwrap(),
        params.as_ref().unwrap(),
        sign,
        wc.unwrap(),
        config
    ).await?;
    let init_balance = matches.is_present("INIT_BALANCE");
    let ton_client = create_client(config)?;
    let enc_msg = encode_message(ton_client.clone(), msg.clone()).await
        .map_err(|e| format!("failed to create inbound message: {}", e))?;

    let account = if init_balance {
        Account::with_address_and_ballance(
            &MsgAddressInt::with_standart(
                None,
                wc.unwrap() as i8,
                AccountId::from_string(address.split(':').collect::<Vec<&str>>()[1])
                    .map_err(|e| format!("{}", e))?)
                .map_err(|e| format!("{}", e))?,
            &CurrencyCollection::with_grams(u64::MAX)
        )
    } else {
        let account = query_account_field(
            ton_client.clone(),
            &address,
            "boc"
        ).await?;
        Account::construct_from_base64(&account)
            .map_err(|e| format!("Failed to construct account: {}", e))?
    };

    let message = Message::construct_from_base64(&enc_msg.message)
        .map_err(|e| format!("Failed to construct message: {}", e))?;

    let mut acc_root = account.serialize()
        .map_err(|e| format!("Failed to serialize account: {}", e))?;

    let trace_path = output.unwrap().to_string();

    log::set_max_level(log::LevelFilter::Trace);
    log::set_boxed_logger(
        Box::new(DebugLogger::new(trace_path.clone()))
    ).map_err(|e| format!("Failed to set logger: {}", e))?;

    let now = parse_now(matches)?;

    let trans = execute_debug(
        Some(matches),
        get_blockchain_config(config, matches.value_of("CONFIG_PATH")).await?,
        &mut acc_root,
        Some(&message),
        (now / 1000) as u32,
        now,
        now,
        false,
        config,
    ).await;

    let msg_string = match trans {
        Ok(trans) => {
            decode_messages(trans.out_msgs,load_decode_abi(matches, config), config).await?;
            "Execution finished.".to_string()
        }
        Err(e) => {
            format!("Execution failed: {}", e)
        }
    };
    if !config.is_json {
        println!("{}", msg_string);
        println!("Log saved to {}", trace_path);
    } else {
        println!("{{}}");
    }

    Ok(())
}

async fn decode_messages(msgs: OutMessages, abi: Option<String>, config: &Config) -> Result<Vec<Value>, String> {
    if !msgs.is_empty() {
        log::debug!(target: "executor", "Output messages:\n----------------");
    }
    let msgs = msgs.export_vector()
        .map_err(|e| format!("Failed to parse out messages: {}", e))?;

    let mut res = vec![];
    for msg in msgs {
        let mut ser_msg = serialize_msg(&msg.0, abi.clone(), config).await
            .map_err(|e| format!("Failed to serialize message: {}", e))?;
        let msg_str = base64::encode(
            &ton_types::cells_serialization::serialize_toc(
                &msg.0
                    .serialize()
                    .map_err(|e| format!("Failed to serialize out message: {}", e))?
            ).map_err(|e| format!("failed to encode out message: {}", e))?
        );
        ser_msg["Message_base64"] = serde_json::Value::String(msg_str);
        if ser_msg["BodyCall"].is_object() {
            res.push(ser_msg["BodyCall"].clone());
        }
        log::debug!(target: "executor", "\n{}\n", serde_json::to_string_pretty(&ser_msg)
            .map_err(|e| format!("Failed to serialize json: {}", e))?);
    }
    Ok(res)
}

async fn query_address(tr_id: &str, config: &Config) -> Result<String, String> {
    let ton_client = create_client(config)?;
    let query_result = query_with_limit(
        ton_client,
        "transactions",
        json!({
            "id": {
                "eq": tr_id
            }
        }),
        "account_addr",
        None,
        Some(1)
    ).await
        .map_err(|e| format!("Failed to query address: {}", e))?;
    match query_result.len() {
        0 => Err("Transaction was not found".to_string()),
        _ => Ok(query_result[0]["account_addr"]
            .to_string()
            .trim_start_matches(|c| c == '"')
            .trim_end_matches(|c| c == '"')
            .to_string())
    }
}

struct TrDetails {
    transaction_id: String,
    timestamp: String,
    source_address: String,
    message_type: String
}

impl fmt::Display for TrDetails {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "\ttransaction_id: {}", self.transaction_id)?;
        writeln!(f, "\ttimestamp     : {}", self.timestamp)?;
        writeln!(f, "\tmessage_type  : {}", self.message_type)?;
        writeln!(f, "\tsource_address: {}", self.source_address)
    }
}

async fn query_transactions(address: &str, config: &Config) -> Result<Vec<TrDetails>, String> {
    let ton_client = create_client(config)?;
    let order = vec![OrderBy{ path: "lt".to_string(), direction: SortDirection::DESC }];
    let query_result = query_with_limit(
        ton_client,
        "transactions",
        json!({
            "account_addr": {
                "eq": address
            }
        }),
        "id now_string in_message { src msg_type_name }",
        Some(order),
        Some(TRANSACTION_QUANTITY)
    ).await
        .map_err(|e| format!("Failed to query address: {}", e))?;
    match query_result.len() {
        0 => Err("Transaction list is empty.".to_string()),
        _ => {
            Ok(query_result.iter().map(|query| {
                TrDetails{
                    transaction_id: query["id"].to_string(),
                    timestamp: query["now_string"].to_string(),
                    source_address: query["in_message"]["src"].to_string(),
                    message_type: query["in_message"]["msg_type_name"].to_string()
                }
            }).collect::<Vec<TrDetails>>())
        }
    }
}

fn choose_transaction(transactions: Vec<TrDetails>) -> Result<String, String> {
    println!("\n\nChoose transaction you want to debug:");
    for index in 1..=transactions.len() {
        println!("{}){}", index, transactions[index - 1]);
    }
    println!("\n\nEnter number of the chosen transaction (from 1 to {}):", transactions.len());
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let chosen: usize = input.trim().parse()
        .map_err(|e| format!("Failed to parse user input as integer: {}", e))?;
    if !(1..=transactions.len()).contains(&chosen) {
        return Err("Wrong transaction number".to_string());
    }
    Ok(transactions[chosen-1].transaction_id.trim_start_matches(|c| c == '"').trim_end_matches(|c| c == '"').to_string())
}

pub async fn execute_debug(
    matches: Option<&ArgMatches<'_>>,
    bc_config: BlockchainConfig,
    account: &mut Cell,
    message: Option<&Message>,
    block_unixtime: u32,
    block_lt: u64,
    last_tr_lt: u64,
    is_getter: bool,
    tonos_config: &Config,
) -> Result<Transaction, String> {
    let bc_config = if is_getter {
        let mut config = bc_config.raw_config().to_owned();
        let gas = GasLimitsPrices {
            gas_price: 65536000,
            flat_gas_limit: 100,
            flat_gas_price: 1000000,
            gas_limit: u64::MAX,
            special_gas_limit: u64::MAX,
            gas_credit: u64::MAX,
            block_gas_limit: u64::MAX,
            freeze_due_limit: 100000000,
            delete_due_limit:1000000000,
            max_gas_threshold:u128::MAX,
        };
        let c20 = ConfigParamEnum::ConfigParam20(gas.clone());
        let c21 = ConfigParamEnum::ConfigParam21(gas);
        config.set_config(c20).unwrap();
        config.set_config(c21).unwrap();
        BlockchainConfig::with_config(config).map_err(|e| format!("Failed to construct config: {}", e))?
    } else {
        bc_config
    };

    let executor = Box::new(
        OrdinaryTransactionExecutor::new(
            bc_config.clone(),
        )
    );
    let params = ExecuteParams {
        state_libs: HashmapE::default(),
        block_unixtime,
        block_lt,
        last_tr_lt: Arc::new(AtomicU64::new(last_tr_lt)),
        seed_block: UInt256::default(),
        debug: true,
        trace_callback: generate_callback(matches, tonos_config),
        ..ExecuteParams::default()
    };

    executor.execute_with_libs_and_params(
        message,
         account,
        params
    ).map_err(|e| format!("Debug failed: {}", e))
}

fn trace_callback(info: &EngineTraceInfo, debug_info: &Option<DbgInfo>) {
    if info.info_type == EngineTraceInfoType::Dump {
        log::info!(target: "tvm", "{}", info.cmd_str);
        return;
    }

    log::info!(target: "tvm", "{}: {}",
             info.step,
             info.cmd_str
    );
    log::info!(target: "tvm", "{} {}",
             info.cmd_code.remaining_bits(),
             info.cmd_code.to_hex_string()
    );

    log::info!(target: "tvm", "\nGas: {} ({})",
             info.gas_used,
             info.gas_cmd
    );

    let position = get_position(info, debug_info);
    if position.is_some() {
        log::info!(target: "tvm", "Position: {}", position.unwrap());
    } else {
        log::info!(target: "tvm", "Position: Undefined");
    }

    log::info!(target: "tvm", "\n--- Stack trace ------------------------");
    for item in info.stack.iter() {
        log::info!(target: "tvm", "{}", item);
    }
    log::info!(target: "tvm", "----------------------------------------\n");
}


fn trace_callback_minimal(info: &EngineTraceInfo, debug_info: &Option<DbgInfo>) {
    let position = match get_position(info, &debug_info) {
        Some(position) => position,
        _ => "".to_string()
    };
    log::info!(target: "tvm", "{} {} {} {} {}", info.step, info.gas_used, info.gas_cmd, info.cmd_str, position);
}

fn get_position(info: &EngineTraceInfo, debug_info: &Option<DbgInfo>) -> Option<String> {
    if let Some(debug_info) = debug_info {
        let cell_hash = info.cmd_code.cell().repr_hash();
        let offset = info.cmd_code.pos();
        let position = match debug_info.get(&cell_hash) {
            Some(offset_map) => match offset_map.get(&offset) {
                Some(pos) => format!("{}:{}", pos.filename, pos.line),
                None => String::from("-:0 (offset not found)")
            },
            None => String::from("-:0 (cell hash not found)")
        };
        return Some(position)
    }
    None
}

fn generate_callback(matches: Option<&ArgMatches<'_>>, config: &Config) -> Option<Arc<dyn Fn(&Engine, &EngineTraceInfo) + Send + Sync>> {
    match matches {
        Some(matches) => {
            let opt_abi = abi_from_matches_or_config(matches, config);
            let debug_info = matches.value_of("DBG_INFO").map(|s| s.to_string())
                .or(
                    if opt_abi.is_ok() {
                        load_debug_info(opt_abi.as_ref().unwrap())
                    } else {
                        None
                    }
                );
            let debug_info: Option<DbgInfo> = match debug_info {
                Some(dbg_info) => {
                    match File::open(dbg_info.clone()) {
                        Ok(file ) => match serde_json::from_reader(file) {
                            Ok(info) => Some(info),
                            Err(e) => {
                                println!("Failed to serde debug info from file {}: {}", dbg_info,e);
                                None
                            },
                        },
                        Err(e) =>  {
                            println!("Failed to open file {}: {}", dbg_info, e);
                            None
                        }
                    }
                },
                _ => None
            };
            Some(if matches.is_present("FULL_TRACE") {
                Arc::new(move |_, info| trace_callback(info, &debug_info))
            } else {
                Arc::new(move |_, info| trace_callback_minimal(info, &debug_info))
            })
        },
        _ => {
            Some(if config.debug_fail == "Full".to_string() {
                Arc::new(move |_, info| trace_callback(info, &None))
            } else {
                Arc::new(move |_, info| trace_callback_minimal(info, &None))
            })
        }
    }

}

const RENDER_NONE: u8    = 0x00;
const RENDER_GAS: u8     = 0x01;

pub async fn sequence_diagram_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let filename = matches.value_of("ADDRESSES").unwrap();
    let file = std::fs::File::open(filename)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut addresses = vec!();
    let lines = std::io::BufReader::new(file).lines();
    for line in lines {
        if let Ok(line) = line {
            if !line.is_empty() && !line.starts_with('#'){
                addresses.push(load_ton_address(&line, config)?);
            }
        }
    }
    if addresses.iter().collect::<HashSet<_>>().len() < addresses.len() {
        return Err("Addresses are not unique".to_owned())
    }
    let mut output = std::fs::File::create(format!("{}.plantuml", filename))
        .map_err(|e| format!("Failed to create file: {}", e))?;
    make_sequence_diagram(config, addresses, RENDER_NONE, &mut output).await.map(|res| {
        println!("{}", res);
    })
}

fn infer_address_width(input: &Vec<String>, min_width: usize) -> Result<usize, String> {
    let max_width = input.iter().fold(0, |acc, item| {
        std::cmp::max(acc, item.len())
    });
    let addresses = input.iter().map(|address|
        format!("{:>max_width$}", address)
    ).collect::<Vec<_>>();

    let mut width = min_width;
    loop {
        let set = addresses.iter().map(|s| s.split_at(width).1).collect::<HashSet<_>>();
        if set.len() == addresses.len() {
            break;
        }
        width += 1;
    }
    Ok(width)
}

static ACCOUNT_WIDTH: usize = 8;
static MESSAGE_WIDTH: usize = 6;

struct TransactionExt {
    id: String,
    address: String,
    tr: Transaction,
}

async fn fetch_transactions(config: &Config, addresses: &Vec<String>) -> Result<Vec<TransactionExt>, String> {
    let context = create_client_verbose(config)?;
    let retry_strategy = tokio_retry::strategy::ExponentialBackoff::from_millis(10).take(5);

    let mut txns = vec!();
    for address in addresses {
        let mut lt = String::from("0x0");
        loop {
            let action = || async {
                query_collection(
                    context.clone(),
                    ParamsOfQueryCollection {
                        collection: "transactions".to_owned(),
                        filter: Some(serde_json::json!({
                            "account_addr": {
                                "eq": address.clone()
                            },
                            "lt": {
                                "gt": lt
                            }
                        })),
                        result: "lt boc id workchain_id".to_owned(),
                        order: Some(vec![
                            OrderBy { path: "lt".to_owned(), direction: SortDirection::ASC }
                        ]),
                        limit: None,
                    },
                ).await
            };

            let transactions = tokio_retry::Retry::spawn(retry_strategy.clone(), action).await
                .map_err(|e| format!("Failed to fetch transactions: {}", e))?;

            if transactions.result.is_empty() {
                break;
            }

            for txn in &transactions.result {
                let boc = txn["boc"].as_str().unwrap();
                let id = txn["id"].as_str().unwrap();
                let workchain_id = txn["workchain_id"].as_i64().unwrap();
                let txn = Transaction::construct_from_base64(boc)
                    .map_err(|e| format!("Failed to deserialize txn: {}", e))?;
                txns.push(TransactionExt {
                    id: id.to_owned(),
                    address: format!("{}:{}", workchain_id, txn.account_id().to_hex_string()),
                    tr: txn,
                });
            }

            let last = transactions.result.last().ok_or("Failed to get last txn".to_string())?;
            lt = last["lt"].as_str().ok_or("Failed to parse value".to_string())?.to_owned();
        }
    }
    txns.sort_by(|tr1, tr2| tr1.tr.logical_time().partial_cmp(&tr2.tr.logical_time()).unwrap());
    Ok(txns)
}

fn map_inbound_messages_onto_tr(txns: &Vec<TransactionExt>) -> HashMap<UInt256, Transaction> {
    let mut map = HashMap::default();
    for txn in txns {
        let hash = txn.tr.in_msg.as_ref().unwrap().hash();
        map.insert(hash, txn.tr.clone());
    }
    map
}

fn sort_outbound_messages(tr: &Transaction, map: &HashMap<UInt256, Transaction>) -> Result<Vec<Message>, String> {
    let mut messages = vec!();
    tr.iterate_out_msgs(|msg| {
        let hash = msg.serialize().unwrap().repr_hash();
        let lt = if let Some(tr) = map.get(&hash) {
            tr.logical_time()
        } else {
            u64::MAX
        };
        messages.push((lt, msg));
        Ok(true)
    }).unwrap();
    messages.sort_by(|(lt1, _), (lt2, _)| lt2.partial_cmp(lt1).unwrap());
    Ok(messages.iter().map(|(_, v)| v.clone()).collect())
}

async fn make_sequence_diagram(
    config: &Config,
    addresses: Vec<String>,
    render_flags: u8,
    output: &mut File
) -> Result<String, String> {
    let _name_length = infer_address_width(&addresses, 6)?;
    let name_length = ACCOUNT_WIDTH;

    let name_map = addresses.iter().enumerate().map(|(index, address)| {
        (address.clone(), (index, address.split_at(name_length).0.to_owned()))
    }).collect::<HashMap<String, (usize, String)>>();

    let txns = fetch_transactions(config, &addresses).await?;
    let inbound_map = map_inbound_messages_onto_tr(&txns);

    let mut url = config.url.replace(".dev", ".live");
    if !url.starts_with("https://") {
        url = format!("https://{}", url);
    }

    let url_account_prefix = format!("{}/accounts/accountDetails?id=", url);
    let url_message_prefix = format!("{}/messages/messageDetails?id=", url);
    let url_txn_prefix = format!("{}/transactions/transactionDetails?id=", url);

    writeln!(output, "@startuml").unwrap();
    for address in addresses {
        let (index, name) = &name_map[&address];
        writeln!(output, "participant \"[[{url_account_prefix}{} {}]]\" as {}", address, name, index).unwrap();
    }

    let mut last_own_index = None;
    let mut last_tr_id: Option<String> = None;
    let mut rendered = HashSet::<UInt256>::default();
    for TransactionExt { id, address, tr } in txns {
        writeln!(output, "' {}", id).unwrap();

        let is_separate = last_tr_id.as_ref() != Some(&id);
        let tr_name = id.split_at(MESSAGE_WIDTH).0;
        let (own_index, _) = &name_map[&address];
        let in_msg_cell = tr.in_msg.as_ref().unwrap();

        if rendered.insert(in_msg_cell.hash()) || is_separate {
            let in_msg = in_msg_cell.read_struct().unwrap();
            let msg_id = in_msg_cell.hash().to_hex_string();
            let msg_name = msg_id.split_at(MESSAGE_WIDTH).0;
            if let Some(src) = in_msg.src_ref() { // internal message
                let src_address = src.to_string();
                if let Some((src_index, _)) = name_map.get(&src_address) {
                    // message from an inner account
                    writeln!(output, "{} ->> {} : m:[[{url_message_prefix}{} {}]]\\nt:[[{url_txn_prefix}{} {}]]",
                        src_index, own_index, msg_id, msg_name, id, tr_name).unwrap();
                } else {
                    // message from an out of the scope account
                    writeln!(output, "[->> {} : m:[[{url_message_prefix}{} {}]]\\nt:[[{url_txn_prefix}{} {}]]",
                        own_index, msg_id, msg_name, id, tr_name).unwrap();
                }
            } else { // external message
                assert!(in_msg.is_inbound_external());
                writeln!(output, "[o->> {} : m:[[{url_message_prefix}{} {}]]\\nt:[[{url_txn_prefix}{} {}]]",
                    own_index, msg_id, msg_name, id, tr_name).unwrap();
            }
        } else if last_own_index == Some(own_index) { // rendered, adjacent, and active participant stays unchanged
            writeln!(output, "{} [hidden]-> {}", own_index, own_index).unwrap();
        }

        let desc = tr.read_description().map_err(|e| format!("Failed to read tr desc: {}", e))?;
        let (tr_color, tr_gas) = match desc.compute_phase_ref() {
            None | Some(ton_block::TrComputePhase::Skipped(_)) => ("", None),
            Some(ton_block::TrComputePhase::Vm(tr_compute_phase_vm)) => {
                let gas = tr_compute_phase_vm.gas_used.to_string();
                if tr_compute_phase_vm.success {
                    ("#YellowGreen", Some(gas))
                } else {
                    ("#Tomato", Some(gas))
                }
            }
        };

        writeln!(output, "activate {} {}", own_index, tr_color).unwrap();
        last_tr_id = None;
        let out_msgs = sort_outbound_messages(&tr, &inbound_map)?;
        for out_msg in out_msgs {
            let out_hash = out_msg.serialize().unwrap().repr_hash();
            let out_id = out_hash.to_hex_string();
            let out_name = out_id.split_at(MESSAGE_WIDTH).0;
            if let Some(out_address) = out_msg.dst_ref() { // internal message
                let out_address = out_address.to_string();
                if let Some((out_index, _)) = name_map.get(&out_address) {
                    // message to an inner account
                    if let Some(tr) = inbound_map.get(&out_hash) {
                        // message spawns a known transaction
                        let tr_id =  tr.serialize().unwrap().repr_hash().to_hex_string();
                        let tr_name = tr_id.split_at(MESSAGE_WIDTH).0;
                        writeln!(output, "{} ->> {} : m:[[{url_message_prefix}{} {}]]\\nt:[[{url_txn_prefix}{} {}]]",
                            own_index, out_index, out_id, out_name, tr_id, tr_name).unwrap();
                        last_tr_id = Some(tr_id);
                    } else {
                        // transaction spawned by the message is out of the scope
                        writeln!(output, "{} ->> {} : m:[[{url_message_prefix}{} {}]]",
                            own_index, out_index, out_id, out_name).unwrap();
                    }
                } else {
                    // message to an out of the scope account
                    writeln!(output, "{} ->>] : m:[[{url_message_prefix}{} {}]] to [[{url_account_prefix}{} {}]]",
                        own_index, out_id, out_name,
                        out_address, out_address.split_at(ACCOUNT_WIDTH).0).unwrap();
                }
            } else { // external message
                assert!(out_msg.is_outbound_external());
                writeln!(output, "{} ->>o] : m:[[{url_message_prefix}{} {}]]", own_index, out_id, out_name).unwrap();
            }
            rendered.insert(out_hash);
        }
        if tr.msg_count() == 0 {
            writeln!(output, "{} [hidden]-> {}", own_index, own_index).unwrap();
        }
        if render_flags & RENDER_GAS != 0 {
            if let Some(tr_gas) = tr_gas {
                writeln!(output, "note over {}: {}", own_index, tr_gas).unwrap();
            }
        }
        writeln!(output, "deactivate {}", own_index).unwrap();
        last_own_index = Some(own_index);
    }

    writeln!(output, "@enduml").unwrap();
    Ok("{{}}".to_owned())
}
