/*
 * Copyright 2023-2023 EverX.
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

use crate::config::Config;
use crate::crypto::{self, load_keypair};
use crate::debug::{decode_messages, execute_debug, init_debug_logger, DEFAULT_TRACE_PATH};
use crate::getconfig::serialize_config_param;
use crate::helpers::{
    create_client_local, decode_data, get_blockchain_config, load_abi, now_ms, unpack_alternative_params, load_params,
};
use crate::FullConfig;
use clap::{App, Arg, ArgMatches, SubCommand};
use ed25519_dalek::Signer;
use serde_json::json;
use std::path::PathBuf;
use ton_block::{
    Account, ConfigParams, CurrencyCollection, Deserializable, Message, Serializable, TickTock,
};
use ton_client::abi::{
    encode_internal_message, encode_message, CallSet, DeploySet, FunctionHeader,
    ParamsOfEncodeInternalMessage, ParamsOfEncodeMessage, Signer as AbiSigner,
};
use ton_types::{read_single_root_boc, write_boc, SliceData, BuilderData};

pub fn create_test_sign_command<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name("sign")
        .about("Generates the ED25519 signature for bytestring.")
        .arg(
            Arg::with_name("DATA")
                .long("--data")
                .short("-d")
                .takes_value(true)
                .help("Bytestring for signing base64 or hex encoded."),
        )
        .arg(
            Arg::with_name("CELL")
                .long("--cell")
                .short("-c")
                .takes_value(true)
                .help("Serialized TOC for signing base64 or hex encoded."),
        )
}

pub fn create_test_command<'a, 'b>() -> App<'a, 'b> {
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

    let boc_path_arg = Arg::with_name("PATH")
        .required(true)
        .help("Contract path to the file with saved contract state.");

    let params_arg = Arg::with_name("PARAMS")
        .long("--params")
        .short("-p")
        .takes_value(true)
        .help("Constructor arguments. Must be a json string with all arguments or path to the file with parameters.");

    let keys_arg = Arg::with_name("KEYS")
        .long("--keys")
        .takes_value(true)
        .help("Secret key used to sign the message.");

    let abi_arg = Arg::with_name("ABI")
        .long("--abi")
        .takes_value(true)
        .required(true)
        .help("Path to the contract ABI file.");

    let full_trace_arg = Arg::with_name("FULL_TRACE")
        .long("--full_trace")
        .short("-f")
        .help("Flag that changes trace to full version.");

    let config_boc_arg = Arg::with_name("CONFIG_BOC")
        .long("--bc_config")
        .short("-c")
        .takes_value(true)
        .help("Path to the config contract boc.");

    let now_arg = Arg::with_name("NOW")
        .long("--now")
        .short("-n")
        .takes_value(true)
        .help("Now timestamp (in milliseconds) for execution. If not set it is equal to the current timestamp.");

    let deploy_cmd = SubCommand::with_name("deploy")
        .about("Deploy contract locally with trace. It uses TVC file on input and produced BOC file on output.")
        .alias("td")
        .arg(boc_path_arg.clone())
        .arg(params_arg)
        .arg(output_arg.clone())
        .arg(dbg_info_arg.clone())
        .arg(abi_arg.clone())
        .arg(full_trace_arg.clone())
        .arg(keys_arg.clone())
        .arg(now_arg.clone())
        .arg(config_boc_arg.clone())
        .arg(
            Arg::with_name("ACCOUNT_ADDRESS")
                .long("--address")
                .takes_value(true)
                .required(true)
                .allow_hyphen_values(true)
                .help("--address Address for account which will override automatically calculated one."),
        )
        .arg(
            Arg::with_name("EXTERNAL")
                .long("--external")
                .help("use external message to deploy contract instead of internal."),
        )
        // .arg(
        //     Arg::with_name("IS_TICK")
        //         .long("--tick")
        //         .help("add tick mark for account."),
        // )
        // .arg(
        //     Arg::with_name("IS_TOCK")
        //         .long("--tock")
        //         .help("add tock mark for account."),
        // )
        .arg(
            Arg::with_name("INITIAL_BALANCE")
                .long("--initial_balance")
                .takes_value(true)
                .required(true)
                .help("Initial balance in nanotokens."),
        );

    let ticktock_cmd = SubCommand::with_name("ticktock")
        .about("Make ticktock transaction")
        .alias("tt")
        .arg(boc_path_arg.clone())
        .arg(output_arg.clone())
        .arg(dbg_info_arg.clone())
        .arg(full_trace_arg.clone())
        .arg(now_arg.clone())
        .arg(config_boc_arg.clone())
        .arg(
            Arg::with_name("IS_TOCK")
                .long("--tock")
                .help("make tock transaction."),
        );

    let config_cmd = SubCommand::with_name("config")
        .about("Encode or decode config params")
        .alias("tc")
        .arg(Arg::with_name("ENCODE")
            .long("--encode")
            .takes_value(true)
            .conflicts_with("DECODE")
            .help("Encode single config param or all config params to TvmCell. JSON format of path to the file.")
        )
        .arg(Arg::with_name("DECODE")
            .alias("tcd")
            .long("--decode")
            .conflicts_with("ENCODE")
            .takes_value(true)
            .help("Decode single config param or all config params to TvmCell.")
        )
        .arg(Arg::with_name("INDEX")
            .long("--index")
            .requires("DECODE")
            .takes_value(true)
            .help("Index of config parameter to decode.")
        );

    SubCommand::with_name("test")
        .about("Test commands.")
        .subcommand(deploy_cmd)
        .subcommand(ticktock_cmd)
        .subcommand(config_cmd)
        .subcommand(create_test_sign_command().alias("ts").arg(keys_arg.clone()))
}

pub async fn test_command(
    matches: &ArgMatches<'_>,
    full_config: &FullConfig,
) -> Result<(), String> {
    let config = &full_config.config;
    if let Some(matches) = matches.subcommand_matches("deploy") {
        return test_deploy(matches, config).await;
    }
    if let Some(matches) = matches.subcommand_matches("ticktock") {
        return test_ticktock(matches, config).await;
    }
    if let Some(matches) = matches.subcommand_matches("sign") {
        return test_sign_command(matches, config);
    }
    if let Some(matches) = matches.subcommand_matches("config") {
        return test_config_command(matches, config);
    }
    Err("unknown command".to_string())
}

async fn test_deploy(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let input = matches.value_of("PATH").unwrap();
    let abi_path = matches.value_of("ABI").unwrap();
    let function_name = "constructor".to_string();
    let params = unpack_alternative_params(matches, abi_path, &function_name, config).await?;
    let bc_config = matches.value_of("CONFIG_BOC");
    let keys = matches.value_of("KEYS").map(load_keypair).transpose()?;
    let address_opt = matches.value_of("ACCOUNT_ADDRESS");
    let balance = matches.value_of("INITIAL_BALANCE").unwrap();
    let workchain_id = matches.value_of("WC").and_then(|wc| wc.parse().ok());
    let now = matches
        .value_of("NOW")
        .and_then(|now| now.parse().ok())
        .unwrap_or(now_ms());
    let trace_path = matches.value_of("LOG_PATH").unwrap_or(DEFAULT_TRACE_PATH);

    let tvc_bytes =
        std::fs::read(input).map_err(|e| format!("Failed to read TVC file {input}: {e}"))?;

    let context = create_client_local()?;
    let abi = load_abi(abi_path, config).await?;
    let (signer, initial_pubkey) = match keys {
        Some(keys) if keys.secret == "0".repeat(64) => (AbiSigner::None, Some(keys.public)),
        Some(keys) => (AbiSigner::Keys { keys }, None),
        None => (AbiSigner::None, None),
    };
    let deploy_set = Some(DeploySet {
        tvc: Some(base64::encode(&tvc_bytes)),
        workchain_id,
        initial_pubkey,
        ..Default::default()
    });
    let params = serde_json::from_str(&load_params(&params)?)
        .map_err(|e| format!("function arguments is not a json: {}", e))?;
    let header = Some(FunctionHeader {
        time: Some(now),
        ..Default::default()
    });
    let call_set = Some(CallSet {
        function_name,
        input: Some(params),
        header,
    });
    let mut account;
    let mut message;
    let is_external = matches.is_present("EXTERNAL");
    if is_external {
        let msg_params = ParamsOfEncodeMessage {
            abi,
            deploy_set,
            call_set,
            signer,
            ..Default::default()
        };
        let enc_msg = encode_message(context.clone(), msg_params)
            .await
            .map_err(|e| format!("Failed to create deploy message: {e}"))?;
        message = Message::construct_from_base64(&enc_msg.message).unwrap();
        let addr = &enc_msg
            .address
            .parse()
            .map_err(|e| format!("Failed to set address {}: {e}", enc_msg.address))?;
        let balance = balance.parse()
            .map_err(|e| format!("Failed to parse initial balance: {e}"))?;

        let balance = CurrencyCollection::with_grams(balance);
        account = Account::with_address_and_ballance(addr, &balance);
    } else {
        let src_address = "-1:0000000000000000000000000000000000000000000000000000000000000000";
        let msg_params = ParamsOfEncodeInternalMessage {
            abi: Some(abi),
            deploy_set,
            call_set,
            bounce: Some(false),
            src_address: Some(src_address.to_string()),
            value: balance.to_string(),
            ..Default::default()
        };
        let enc_msg = encode_internal_message(context.clone(), msg_params)
            .map_err(|e| format!("Failed to create deploy internal message: {e}"))?;
        message = Message::construct_from_base64(&enc_msg.message).unwrap();
        if let Some(header) = message.int_header_mut() {
            header.value.grams = balance.parse()
               .map_err(|e| format!("Failed to parse initial balance: {e}"))?;
        }
        account = Account::default();
    }
    init_debug_logger(trace_path)?;
    let bc_config = get_blockchain_config(config, bc_config).await?;
    let mut account_root = account.serialize().unwrap_or_default();
    let transaction = execute_debug(
        bc_config,
        &mut account_root,
        Some(&message),
        Some(matches),
        now,
        0,
        0,
        false,
        false,
        config,
    )
    .await?;

    decode_messages(&transaction, None, config).await?;
    account = Account::construct_from_cell(account_root)
        .map_err(|e| format!("Failed to construct resulting account: {e}"))?;
    if let Some(address) = address_opt {
        let addr = address
            .parse()
            .map_err(|e| format!("Failed to parse address {address}: {e}"))?;
        account.set_addr(addr);
    }
    let output = PathBuf::from(input).with_extension("boc");
    account
        .write_to_file(&output)
        .map_err(|e| format!("Failed write to file {:?}: {e}", output))?;
    if !config.is_json {
        println!("Account written to {:?}", output);
    }
    Ok(())
}

async fn test_ticktock(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let input = matches.value_of("PATH").unwrap();
    let bc_config = matches.value_of("CONFIG_BOC");
    let now = matches
        .value_of("NOW")
        .and_then(|now| now.parse().ok())
        .unwrap_or(now_ms());
    let trace_path = matches.value_of("LOG_PATH").unwrap_or(DEFAULT_TRACE_PATH);
    let is_tock = matches.is_present("IS_TOCK");

    let mut account = Account::construct_from_file(input)
        .map_err(|e| format!("Failed to load Account from the file {input}: {e}"))?;
    if let Some(state_init) = account.state_init_mut().as_mut() {
        state_init.set_special(TickTock::with_values(true, true));
    }

    init_debug_logger(trace_path)?;
    let bc_config = get_blockchain_config(config, bc_config).await?;
    let mut account_root = account.serialize().unwrap();
    let result = execute_debug(
        bc_config,
        &mut account_root,
        None,
        Some(matches),
        now,
        0,
        0,
        false,
        is_tock,
        config,
    )
    .await;

    match result {
        Ok(transaction) => {
            decode_messages(&transaction, None, config).await?;
        }
        Err(e) => {
            return Err(format!("Failed to perform ticktock transaction: {e}"));
        }
    }
    let account = Account::construct_from_cell(account_root)
        .map_err(|e| format!("Failed to construct Account after transaction: {e}"))?;
    account
        .write_to_file(input)
        .map_err(|e| format!("Failed write to file {:?}: {e}", input))?;
    if !config.is_json {
        println!("Account written to {:?}", input);
    }
    Ok(())
}

pub fn test_sign_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let data = if let Some(data) = matches.value_of("DATA") {
        decode_data(data, "data")?
    } else if let Some(data) = matches.value_of("CELL") {
        let data = decode_data(data, "cell")?;
        let cell = read_single_root_boc(data)
            .map_err(|err| format!("Cannot deserialize tree of cells {}", err))?;
        if cell.references_count() == 0 && (cell.bit_length() % 8) == 0 {
            // sign data
            cell.data().to_vec()
        } else {
            cell.repr_hash().into_vec()
        }
    } else {
        return Err("nor data neither cell parameter".to_string());
    };
    let pair = match matches.value_of("KEYS") {
        Some(keys) => crypto::load_keypair(keys)?,
        None => match &config.keys_path {
            Some(keys) => crypto::load_keypair(keys)?,
            None => return Err("nor signing keys in the params neither in the config".to_string()),
        },
    };
    let keypair = pair
        .decode()
        .map_err(|err| format!("cannot decode keypair {}", err))?;
    let signature = keypair.sign(&data);
    let signature = base64::encode(signature.as_ref());
    if config.is_json {
        let result = json!({
            "Data": hex::encode(data),
            "public": hex::encode(pair.public.as_bytes()),
            "Signature": signature
        });
        println!("{:#}", result);
    } else {
        println!("Signature: {}", signature);
    }

    Ok(())
}

pub fn test_config_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    if let Some(encode) = matches.value_of("ENCODE") {
        let (cell, index) = serialize_config_param(&load_params(encode)?)?;
        let result = write_boc(&cell);
        let cell = match result {
            Ok(config_bytes) => base64::encode(config_bytes),
            Err(e) => return Err(format!("Failed to serialize json {encode}: {e}")),
        };
        if config.is_json {
            println!("{:#}", json!({ "Cell": cell, "index": index }));
        } else {
            println!("Cell: \"{}\"", cell);
        }
    } else if let Some(decode) = matches.value_of("DECODE") {
        let bytes = std::fs::read(decode).unwrap();
        let cell = read_single_root_boc(bytes)
            .map_err(|e| format!("Failed to deserialize tree of cells {e}"))?;
        let result = if let Some(index) = matches.value_of("INDEX") {
            let index = index
                .parse::<u32>()
                .map_err(|e| format!("Failed to parse index {index}: {e}"))?;
            let mut params = ConfigParams::default();
            let key = SliceData::load_builder(index.write_to_new_cell().unwrap()).unwrap();
            let value = BuilderData::with_raw_and_refs(vec![], 0, [cell]).unwrap();
            params
                .config_params
                .set_builder(key, &value)
                .map_err(|e| format!("Failed to store config param with index {index}: {e}"))?;
            ton_block_json::serialize_config_param(&params, index)
                .map_err(|e| format!("Failed to serialize config param with index {index}: {e}"))?
        } else {
            let params = ConfigParams::construct_from_cell(cell)
                .map_err(|e| format!("Failed to construct ConfigParams: {e}"))?;
            ton_block_json::serialize_config_param(&params, 0)
                .map_err(|e| format!("Failed to serialize config params: {e}"))?
        };
        println!("{}", result);
    }
    Ok(())
}
