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

#![allow(clippy::from_str_radix_10)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::too_many_arguments)]

mod account;
mod call;
mod config;
mod convert;
mod crypto;
mod decode;
mod debot;
mod deploy;
mod depool;
mod depool_abi;
mod genaddr;
mod getconfig;
mod helpers;
mod multisig;
mod sendfile;
mod voting;
mod replay;
mod debug;
mod run;
mod test;
mod message;
#[cfg(feature = "sold")]
mod compile;

use account::{get_account, calc_storage, wait_for_change};
use call::{call_contract, call_contract_with_msg};
use clap::{ArgMatches, SubCommand, Arg, AppSettings, App};
use config::{Config, set_config, clear_config};
use crypto::{generate_mnemonic, extract_pubkey, generate_keypair};
use debot::{create_debot_command, debot_command};
use decode::{create_decode_command, decode_command};
use debug::{create_debug_command, debug_command};
use deploy::{deploy_contract, generate_deploy_message};
use depool::{create_depool_command, depool_command};
use genaddr::generate_address;
use getconfig::{query_global_config, dump_blockchain_config};
use helpers::{load_ton_address, load_abi, create_client_local, query_raw,
              contract_data_from_matches_or_config_alias};
use multisig::{create_multisig_command, multisig_command};
use replay::{fetch_block_command, fetch_command, replay_command};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::env;
use std::process::exit;
use test::{create_test_command, test_command, test_sign_command, create_test_sign_command};
use ton_client::abi::{ParamsOfEncodeMessageBody, CallSet};
use voting::{create_proposal, decode_proposal, vote};
use crate::account::dump_accounts;
#[cfg(feature = "sold")]
use crate::compile::{compile_command, create_compile_command};

use crate::config::{FullConfig, resolve_net_name, default_config_name, global_config_path};
use crate::getconfig::gen_update_config_message;
use crate::helpers::{abi_from_matches_or_config, AccountSource,
    load_abi_from_tvc, load_params, parse_lifetime,
    unpack_alternative_params, wc_from_matches_or_config
};
use crate::message::generate_message;
use crate::run::{run_command, run_get_method};
use std::path::Path;
use tonos_cli::plugins::Plugins;

const DEF_MSG_LIFETIME: u32 = 30;
const DEF_STORAGE_PERIOD: u32 = 60 * 60 * 24 * 365;

enum CallType {
    Call,
    Msg,
    Fee,
}

enum DeployType {
    Full,
    MsgOnly,
    Fee,
}

lazy_static::lazy_static! {
    static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .build()
        .expect("Can't create Engine tokio runtime");
}

fn main() {
    let result = RUNTIME.block_on(async move { main_internal().await });
    if let Err(err_str) = result {
        if !err_str.is_empty() { println!("{}", err_str); }
        exit(1)
    }
}

async fn main_internal() -> Result <(), String> {
    let version_string = env!("CARGO_PKG_VERSION");

    let abi_arg = Arg::with_name("ABI")
        .long("--abi")
        .takes_value(true)
        .help("Path or link to the contract ABI file or pure json ABI data. Can be specified in the config file.");

    let keys_arg = Arg::with_name("KEYS")
        .long("--keys")
        .takes_value(true)
        .help("Seed phrase or path to the file with keypair used to sign the message. Can be specified in the config file.");

    let sign_arg = Arg::with_name("SIGN")
        .long("--sign")
        .takes_value(true)
        .help("Seed phrase or path to the file with keypair used to sign the message. Can be specified in the config.");

    let method_opt_arg = Arg::with_name("METHOD")
        .takes_value(true)
        .long("--method")
        .short("-m")
        .help("Name of the function being called.");

    let address_opt_arg = Arg::with_name("ADDRESS")
        .long("--addr")
        .takes_value(true)
        .help("Contract address. Can be specified in the config file.");

    let multi_params_arg = Arg::with_name("PARAMS")
        .help("Function arguments. Must be a list of `--name value` pairs or a json string with all arguments.")
        .multiple(true);

    let author = "EverX";

    let callx_cmd = SubCommand::with_name("callx")
        .about("Sends an external message with encoded function call to the contract (alternative syntax).")
        .version(version_string)
        .author(author)
        .setting(AppSettings::AllowLeadingHyphen)
        .setting(AppSettings::TrailingVarArg)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .arg(address_opt_arg.clone())
        .arg(abi_arg.clone())
        .arg(keys_arg.clone())
        .arg(method_opt_arg.clone())
        .arg(multi_params_arg.clone());

    let tvc_arg = Arg::with_name("TVC")
        .takes_value(true)
        .required(true)
        .help("Path to the compiled smart contract (tvc file).");

    let wc_arg = Arg::with_name("WC")
        .takes_value(true)
        .long("--wc")
        .allow_hyphen_values(true)
        .help("Workchain id of the smart contract (default value is taken from the config).");

    let alias_arg_long = Arg::with_name("ALIAS")
        .long("--alias")
        .takes_value(true)
        .help("Saves contract address and abi to the aliases list to be able to call this contract with alias instaed of address.");

    let deployx_cmd = SubCommand::with_name("deployx")
        .about("Deploys a smart contract to the blockchain (alternative syntax).")
        .version(version_string)
        .author(author)
        .setting(AppSettings::AllowLeadingHyphen)
        .setting(AppSettings::TrailingVarArg)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .arg(abi_arg.clone())
        .arg(keys_arg.clone())
        .arg(wc_arg.clone())
        .arg(tvc_arg.clone())
        .arg(alias_arg_long.clone())
        .arg(multi_params_arg.clone());

    let address_boc_tvc_arg = Arg::with_name("ADDRESS")
        .takes_value(true)
        .help("Contract address or path to the saved account state if --boc or --tvc flag is specified.");

    let method_arg = Arg::with_name("METHOD")
        .required(true)
        .takes_value(true)
        .help("Name of the function being called.");

    let boc_flag = Arg::with_name("BOC")
        .long("--boc")
        .conflicts_with("TVC")
        .help("Flag that changes behavior of the command to work with the saved account state (account BOC).");

    let tvc_flag = Arg::with_name("TVC")
        .long("--tvc")
        .conflicts_with("BOC")
        .help("Flag that changes behavior of the command to work with the saved contract state (stateInit TVC).");

    let bc_config_arg = Arg::with_name("BCCONFIG")
        .long("--bc_config")
        .requires("BOC")
        .takes_value(true)
        .help("Path to the file with blockchain config.");

    let runx_cmd = SubCommand::with_name("runx")
        .about("Runs contract function locally (alternative syntax).")
        .version(version_string)
        .author(author)
        .setting(AppSettings::AllowLeadingHyphen)
        .setting(AppSettings::TrailingVarArg)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .arg(address_boc_tvc_arg.clone()
            .long("--addr"))
        .arg(abi_arg.clone())
        .arg(method_opt_arg.clone())
        .arg(multi_params_arg.clone())
        .arg(boc_flag.clone())
        .arg(tvc_flag.clone())
        .arg(bc_config_arg.clone());

    let runget_cmd = SubCommand::with_name("runget")
        .about("Runs get-method of a FIFT contract.")
        .setting(AppSettings::AllowLeadingHyphen)
        .setting(AppSettings::TrailingVarArg)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .arg(Arg::with_name("ADDRESS")
            .required(true)
            .help("Contract address or path to the saved account state if --boc or --tvc flag is specified."))
        .arg(Arg::with_name("METHOD")
            .required(true)
            .help("Name of the function being called."))
        .arg(Arg::with_name("PARAMS")
            .help("Function arguments.")
            .multiple(true))
        .arg(boc_flag.clone())
        .arg(tvc_flag.clone())
        .arg(bc_config_arg.clone());

    let version_cmd = SubCommand::with_name("version")
        .about("Prints build and version info.");

    let genphrase_cmd = SubCommand::with_name("genphrase")
        .about("Generates a seed phrase for keypair.")
        .version(version_string)
        .author(author)
        .arg(Arg::with_name("DUMP_KEYPAIR")
            .long("--dump")
            .takes_value(true)
            .help("Path where to dump keypair generated from the phrase"));

    let genpubkey_cmd = SubCommand::with_name("genpubkey")
        .about("Generates a public key from the seed phrase.")
        .version(version_string)
        .author(author)
        .arg(Arg::with_name("PHRASE")
            .takes_value(true)
            .required(true)
            .help("Seed phrase (12 words). Should be specified in quotes."));

    let getkeypair_cmd = SubCommand::with_name("getkeypair")
        .about("Generates a keypair from the seed phrase or private key and saves it to the file.")
        .version(version_string)
        .author(author)
        .arg(Arg::with_name("KEY_FILE")
            .takes_value(true)
            .long("--output")
            .short("-o")
            .help("Path to the file where to store the keypair."))
        .arg(Arg::with_name("PHRASE")
            .takes_value(true)
            .long("--phrase")
            .short("-p")
            .help("Seed phrase (12 words) or secret (private) key. Seed phrase should be specified in quotes, secret key as 64 hex chars."));

    let genaddr_cmd = SubCommand::with_name("genaddr")
        .setting(AppSettings::AllowNegativeNumbers)
        .about("Calculates smart contract address in different formats. By default, input tvc file isn't modified.")
        .version(version_string)
        .author(author)
        .arg(tvc_arg.clone())
        .arg(abi_arg.clone())
        .arg(wc_arg.clone())
        .arg(Arg::with_name("GENKEY")
            .takes_value(true)
            .long("--genkey")
            .conflicts_with("SETKEY")
            .help("Path to the file, where a new generated keypair for the contract will be saved."))
        .arg(Arg::with_name("SETKEY")
            .takes_value(true)
            .long("--setkey")
            .conflicts_with("GENKEY")
            .help("Seed phrase or path to the file with keypair."))
        .arg(Arg::with_name("DATA")
            .takes_value(true)
            .long("--data")
            .help("Initial data to insert into the contract. Should be specified in json format."))
        .arg(Arg::with_name("SAVE")
            .long("--save")
            .help("If this flag is specified, modifies the tvc file with the keypair and initial data"));

    let deploy_cmd = SubCommand::with_name("deploy")
        .setting(AppSettings::AllowNegativeNumbers)
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Deploys a smart contract to the blockchain.")
        .version(version_string)
        .author(author)
        .arg(tvc_arg.clone())
        .arg(Arg::with_name("PARAMS")
            .required(true)
            .takes_value(true)
            .help("Constructor arguments. Can be specified with a filename, which contains json data."))
        .arg(abi_arg.clone())
        .arg(sign_arg.clone())
        .arg(keys_arg.clone())
        .arg(wc_arg.clone());

    let output_arg = Arg::with_name("OUTPUT")
        .short("-o")
        .long("--output")
        .takes_value(true)
        .help("Path to the file where to store the message.");

    let raw_arg = Arg::with_name("RAW")
        .long("--raw")
        .help("Creates raw message boc.");

    let deploy_message_cmd = deploy_cmd.clone()
        .name("deploy_message")
        .about("Generates a signed message to deploy a smart contract to the blockchain.")
        .arg(output_arg.clone())
        .arg(raw_arg.clone());

    let address_arg = Arg::with_name("ADDRESS")
        .required(true)
        .takes_value(true)
        .help("Contract address.");


    let params_arg = Arg::with_name("PARAMS")
        .required(true)
        .takes_value(true)
        .help("Function arguments. Can be specified with a filename, which contains json data.");

    let call_cmd = SubCommand::with_name("call")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Sends an external message with encoded function call to the contract.")
        .version(version_string)
        .author(author)
        .arg(address_arg.clone())
        .arg(method_arg.clone())
        .arg(params_arg.clone())
        .arg(abi_arg.clone())
        .arg(keys_arg.clone())
        .arg(sign_arg.clone());

    let send_cmd = SubCommand::with_name("send")
        .about("Sends a prepared message to the contract.")
        .version(version_string)
        .author(author)
        .arg(Arg::with_name("MESSAGE")
            .required(true)
            .takes_value(true)
            .help("Message to send. Message data should be specified in quotes."))
        .arg(abi_arg.clone());

    let message_cmd = SubCommand::with_name("message")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Generates a signed message with encoded function call.")
        .version(version_string)
        .author(author)
        .arg(address_arg.clone())
        .arg(method_arg.clone())
        .arg(params_arg.clone())
        .arg(abi_arg.clone())
        .arg(keys_arg.clone())
        .arg(sign_arg.clone())
        .arg(Arg::with_name("LIFETIME")
            .long("--lifetime")
            .takes_value(true)
            .help("Period of time in seconds while message is valid."))
        .arg(Arg::with_name("TIMESTAMP")
            .long("--time")
            .takes_value(true)
            .help("Message creation time in milliseconds. If not specified, `now` is used."))
        .arg(output_arg.clone())
        .arg(raw_arg.clone());

    let body_cmd = SubCommand::with_name("body")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Generates a payload for internal function call.")
        .version(version_string)
        .author(author)
        .arg(method_arg.clone())
        .arg(params_arg.clone())
        .arg(abi_arg.clone());

    let sign_cmd = create_test_sign_command()
        .author(author)
        .version(version_string)
        .arg(keys_arg.clone());

    let run_cmd = SubCommand::with_name("run")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Runs contract function locally.")
        .version(version_string)
        .author(author)
        .arg(address_boc_tvc_arg.clone()
            .required(true))
        .arg(method_arg.clone())
        .arg(params_arg.clone())
        .arg(abi_arg.clone())
        .arg(boc_flag.clone())
        .arg(tvc_flag.clone())
        .arg(bc_config_arg.clone());

    let config_clear_cmd = SubCommand::with_name("clear")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Resets certain default values for options in the config file. Resets all values if used without options.")
        .arg(Arg::with_name("URL")
            .long("--url")
            .help("Url to connect."))
        .arg(Arg::with_name("ABI")
            .long("--abi")
            .help("Path or link to the contract ABI file or pure json ABI data."))
        .arg(keys_arg.clone())
        .arg(Arg::with_name("ADDR")
            .long("--addr")
            .help("Contract address."))
        .arg(Arg::with_name("METHOD")
            .long("--method")
            .help("Method name that can be saved to be used by some commands (runx, callx)."))
        .arg(Arg::with_name("PARAMETERS")
            .long("--parameters")
            .help("Function parameters that can be saved to be used by some commands (runx, callx)."))
        .arg(Arg::with_name("WALLET")
            .long("--wallet")
            .help("Multisig wallet address."))
        .arg(Arg::with_name("PUBKEY")
            .long("--pubkey")
            .help("User public key. Used by DeBot Browser."))
        .arg(Arg::with_name("WC")
            .long("--wc")
            .help("Workchain id."))
        .arg(Arg::with_name("RETRIES")
            .long("--retries")
            .help("Number of attempts to call smart contract function if previous attempt was unsuccessful."))
        .arg(Arg::with_name("TIMEOUT")
            .long("--timeout")
            .help("Network `wait_for` timeout in ms."))
        .arg(Arg::with_name("MSG_TIMEOUT")
            .long("--message_processing_timeout")
            .help("Network message processing timeout in ms."))
        .arg(Arg::with_name("DEPOOL_FEE")
            .long("--depool_fee")
            .help("Value added to the message sent to depool to cover it's fees (change will be returned)."))
        .arg(Arg::with_name("LIFETIME")
            .long("--lifetime")
            .help("Period of time in seconds while message is valid. Change of this parameter may affect \"out_of_sync\" parameter, because \"lifetime\" should be at least 2 times greater than \"out_of_sync\"."))
        .arg(Arg::with_name("NO_ANSWER")
            .long("--no-answer")
            .help("Flag whether to wait for depool answer when calling a depool function."))
        .arg(Arg::with_name("BALANCE_IN_TONS")
            .long("--balance_in_tons")
            .help("Print balance for account command in tons. If false balance is printed in nanotons."))
        .arg(Arg::with_name("LOCAL_RUN")
            .long("--local_run")
            .help("Enable preliminary local run before deploy and call commands."))
        .arg(Arg::with_name("ASYNC_CALL")
            .long("--async_call")
            .help("Disables wait for transaction to appear in the network after call command."))
        .arg(Arg::with_name("DEBUG_FAIL")
            .long("--debug_fail")
            .help("When enabled tonos-cli executes debug command on fail of run or call command. Can be enabled with values 'full' or 'minimal' which set the trace level for debug run and disabled with value 'none'."))
        .arg(Arg::with_name("OUT_OF_SYNC")
            .long("--out_of_sync")
            .help("Network connection \"out_of_sync_threshold\" parameter in seconds. Mind that it cant exceed half of the \"lifetime\" parameter."))
        .arg(Arg::with_name("IS_JSON")
            .long("--is_json")
            .help("Cli prints output in json format."))
        .arg(Arg::with_name("PROJECT_ID")
            .long("--project_id")
            .help("Project Id in Evercloud (dashboard.evercloud.dev)."))
        .arg(Arg::with_name("ACCESS_KEY")
            .long("--access_key")
            .help("Project secret or JWT in Evercloud (dashboard.evercloud.dev)."));

    let alias_arg = Arg::with_name("ALIAS")
        .required(true)
        .takes_value(true)
        .help("Alias name.");
    let alias_cmd = SubCommand::with_name("alias")
        .about("Commands to work with aliases map")
        .subcommand(SubCommand::with_name("add")
            .about("Add alias to the aliases map.")
            .arg(alias_arg.clone())
            .arg(Arg::with_name("ADDRESS")
                .long("--addr")
                .takes_value(true)
                .help("Contract address."))
            .arg(keys_arg.clone())
            .arg(Arg::with_name("ABI")
                .long("--abi")
                .takes_value(true)
                .help("Path or link to the contract ABI file or pure json ABI data.")))
        .subcommand(SubCommand::with_name("remove")
            .about("Remove alias from the aliases map.")
            .arg(alias_arg.clone()))
        .subcommand(SubCommand::with_name("print")
            .about("Print the aliases map."))
        .subcommand(SubCommand::with_name("reset")
            .about("Clear the aliases map."));

    let url_arg = Arg::with_name("URL")
        .required(true)
        .takes_value(true)
        .help("Url of the endpoints list.");
    let config_endpoint_cmd = SubCommand::with_name("endpoint")
        .about("Commands to work with the endpoints map.")
        .subcommand(SubCommand::with_name("add")
            .about("Add endpoints list.")
            .arg(url_arg.clone())
            .arg(Arg::with_name("ENDPOINTS")
                .required(true)
                .takes_value(true)
                .help("List of endpoints (comma separated).")))
        .subcommand(SubCommand::with_name("remove")
            .about("Remove endpoints list.")
            .arg(url_arg.clone()))
        .subcommand(SubCommand::with_name("reset")
            .about("Reset the endpoints map."))
        .subcommand(SubCommand::with_name("print")
            .about("Print current endpoints map."));

    let config_cmd = SubCommand::with_name("config")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Allows to tune certain default values for options in the config file.")
        .version(version_string)
        .author(author)
        .arg(Arg::with_name("GLOBAL")
            .long("--global")
            .short("-g")
            .help("Change parameters of the global config which contains default values for ordinary configs."))
        .arg(Arg::with_name("URL")
            .long("--url")
            .takes_value(true)
            .help("Url to connect."))
        .arg(Arg::with_name("ABI")
            .long("--abi")
            .takes_value(true)
            .help("Path or link to the contract ABI file or pure json ABI data."))
        .arg(keys_arg.clone())
        .arg(Arg::with_name("ADDR")
            .long("--addr")
            .takes_value(true)
            .help("Contract address."))
        .arg(Arg::with_name("METHOD")
            .long("--method")
            .takes_value(true)
            .help("Method name that can be saved to be used by some commands (runx, callx)."))
        .arg(Arg::with_name("PARAMETERS")
            .long("--parameters")
            .takes_value(true)
            .help("Function parameters that can be saved to be used by some commands (runx, callx)."))
        .arg(Arg::with_name("WALLET")
            .long("--wallet")
            .takes_value(true)
            .help("Multisig wallet address."))
        .arg(Arg::with_name("PUBKEY")
            .long("--pubkey")
            .takes_value(true)
            .help("User public key. Used by DeBot Browser."))
        .arg(Arg::with_name("WC")
            .long("--wc")
            .takes_value(true)
            .help("Workchain id."))
        .arg(Arg::with_name("RETRIES")
            .long("--retries")
            .takes_value(true)
            .help("Number of attempts to call smart contract function if previous attempt was unsuccessful."))
        .arg(Arg::with_name("TIMEOUT")
            .long("--timeout")
            .takes_value(true)
            .help("Network `wait_for` timeout in ms."))
        .arg(Arg::with_name("MSG_TIMEOUT")
            .long("--message_processing_timeout")
            .takes_value(true)
            .help("Network message processing timeout in ms."))
        .arg(Arg::with_name("LIST")
            .long("--list")
            .conflicts_with_all(&["OUT_OF_SYNC", "NO_ANSWER","DEBUG_FAIL", "ASYNC_CALL", "LOCAL_RUN", "BALANCE_IN_TONS", "LIFETIME", "DEPOOL_FEE", "PUBKEY", "URL", "ABI", "KEYS", "ADDR", "RETRIES", "TIMEOUT", "WC", "WALLET"])
            .help("Prints all config parameters."))
        .arg(Arg::with_name("DEPOOL_FEE")
            .long("--depool_fee")
            .takes_value(true)
            .help("Value added to the message sent to depool to cover it's fees (change will be returned)."))
        .arg(Arg::with_name("LIFETIME")
            .long("--lifetime")
            .takes_value(true)
            .help("Period of time in seconds while message is valid. Change of this parameter may affect \"out_of_sync\" parameter, because \"lifetime\" should be at least 2 times greater than \"out_of_sync\"."))
        .arg(Arg::with_name("NO_ANSWER")
            .long("--no-answer")
            .takes_value(true)
            .help("Flag whether to wait for depool answer when calling a depool function."))
        .arg(Arg::with_name("BALANCE_IN_TONS")
            .long("--balance_in_tons")
            .takes_value(true)
            .help("Print balance for account command in tons. If false balance is printed in nanotons."))
        .arg(Arg::with_name("LOCAL_RUN")
            .long("--local_run")
            .takes_value(true)
            .help("Enable preliminary local run before deploy and call commands."))
        .arg(Arg::with_name("ASYNC_CALL")
            .long("--async_call")
            .takes_value(true)
            .help("Disables wait for transaction to appear in the network after call command."))
        .arg(Arg::with_name("DEBUG_FAIL")
            .long("--debug_fail")
            .takes_value(true)
            .help("When enabled tonos-cli executes debug command on fail of run or call command. Can be enabled with values 'full' or 'minimal' which set the trace level for debug run and disabled with value 'none'."))
        .arg(Arg::with_name("OUT_OF_SYNC")
            .long("--out_of_sync")
            .takes_value(true)
            .help("Network connection \"out_of_sync_threshold\" parameter in seconds. Mind that it cant exceed half of the \"lifetime\" parameter."))
        .arg(Arg::with_name("IS_JSON")
            .long("--is_json")
            .takes_value(true)
            .help("Cli prints output in json format."))
        .arg(Arg::with_name("PROJECT_ID")
            .long("--project_id")
            .takes_value(true)
            .help("Project Id in Evercloud (dashboard.evercloud.dev)."))
        .arg(Arg::with_name("ACCESS_KEY")
            .long("--access_key")
            .takes_value(true)
            .help("Project secret or JWT in Evercloud (dashboard.evercloud.dev)."))
        .subcommand(config_clear_cmd)
        .subcommand(config_endpoint_cmd)
        .subcommand(alias_cmd);

    let account_cmd = SubCommand::with_name("account")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Obtains and prints account information.")
        .version(version_string)
        .author(author)
        .arg(boc_flag.clone())
        .arg(Arg::with_name("ADDRESS")
            .takes_value(true)
            .help("List of addresses or file paths (if flag --boc is used).")
            .multiple(true))
        .arg(Arg::with_name("DUMPTVC")
            .long("--dumptvc")
            .short("-d")
            .takes_value(true)
            .conflicts_with("DUMPBOC")
            .help("Dumps account StateInit to the specified tvc file. Works only if one address was given."))
        .arg(Arg::with_name("DUMPBOC")
            .long("--dumpboc")
            .short("-b")
            .takes_value(true)
            .conflicts_with("DUMPTVC")
            .conflicts_with("BOC")
            .help("Dumps the whole account state boc to the specified file. Works only if one address was given. Use 'tonos-cli dump account` to dump several accounts."));

    let account_wait_cmd = SubCommand::with_name("account-wait")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Waits for account change (based on last_trans_lt).")
        .version(version_string)
        .author(author)
        .arg(address_arg.clone())
        .arg(Arg::with_name("TIMEOUT")
            .long("--timeout")
            .takes_value(true)
            .help("Timeout in seconds (default value is 30)."));

    let query_raw = SubCommand::with_name("query-raw")
        .about("Executes a raw GraphQL query.")
        .version(version_string)
        .author(author)
        .arg(Arg::with_name("COLLECTION")
            .required(true)
            .takes_value(true)
            .help("Collection to query."))
        .arg(Arg::with_name("RESULT")
            .required(true)
            .takes_value(true)
            .help("Result fields to print."))
        .arg(Arg::with_name("FILTER")
            .long("--filter")
            .takes_value(true)
            .help("Query filter parameter."))
        .arg(Arg::with_name("LIMIT")
            .long("--limit")
            .takes_value(true)
            .help("Query limit parameter."))
        .arg(Arg::with_name("ORDER")
            .long("--order")
            .takes_value(true)
            .help("Query order parameter."));

    let fee_cmd = SubCommand::with_name("fee")
        .about("Calculates fees for executing message or account storage fee.")
        .subcommand(SubCommand::with_name("storage")
            .setting(AppSettings::AllowLeadingHyphen)
            .about("Gets account storage fee for specified period in nanotons.")
            .version(version_string)
            .author(author)
            .arg(address_arg.clone())
            .arg(Arg::with_name("PERIOD")
                .long("--period")
                .short("-p")
                .takes_value(true)
                .help("Time period in seconds (default value is 1 year).")))
        .subcommand(deploy_cmd.clone()
            .about("Executes deploy locally, calculates fees and prints table of fees in nanotons."))
        .subcommand(call_cmd.clone()
            .about("Executes call locally, calculates fees and prints table of all fees in nanotons."));

    let proposal_cmd = SubCommand::with_name("proposal")
        .help("Proposal control commands.")
        .subcommand(
            SubCommand::with_name("create")
                .about("Submits a proposal transaction in the multisignature wallet with a text comment.")
                .arg(address_arg.clone().help("Address of the multisignature wallet."))
                .arg(Arg::with_name("DEST")
                    .required(true)
                    .takes_value(true)
                    .help("Address of the proposal contract."))
                .arg(Arg::with_name("COMMENT")
                    .required(true)
                    .takes_value(true)
                    .help("Proposal description (max symbols 382)."))
                .arg(keys_arg.clone())
                .arg(Arg::with_name("OFFLINE")
                    .short("-f")
                    .long("--offline")
                    .help("Prints signed message to terminal instead of sending it."))
                .arg(Arg::with_name("LIFETIME")
                    .short("-l")
                    .long("--lifetime")
                    .takes_value(true)
                    .help("Period of time in seconds while message is valid.")))
        .subcommand(
            SubCommand::with_name("vote")
                .about("Confirms a proposal transaction in the multisignature wallet.")
                .arg(address_arg.clone().help("Address of the multisignature wallet."))
                .arg(Arg::with_name("ID")
                    .required(true)
                    .takes_value(true)
                    .help("Proposal transaction id."))
                .arg(keys_arg.clone())
                .arg(Arg::with_name("OFFLINE")
                    .short("-f")
                    .long("--offline")
                    .help("Prints signed message to terminal instead of sending it."))
                .arg(Arg::with_name("LIFETIME")
                    .short("-l")
                    .long("--lifetime")
                    .takes_value(true)
                    .help("Period of time in seconds while message is valid.")))
        .subcommand(
            SubCommand::with_name("decode")
                .about("Prints a comment string from the proposal transaction.")
                .arg(address_arg.clone().help("Address of the multisignature wallet."))
                .arg(Arg::with_name("ID")
                    .required(true)
                    .takes_value(true)
                    .help("Proposal transaction id.")));

    let getconfig_cmd = SubCommand::with_name("getconfig")
        .about("Reads the global configuration parameter with defined index.")
        .arg(Arg::with_name("INDEX")
            .takes_value(true)
            .help("Parameter index. If not specified, command will print all config parameters."));

    let update_config_param_cmd = SubCommand::with_name("update_config")
        .about("Generates message with update of config params.")
        .arg(abi_arg.clone())
        .arg(Arg::with_name("SEQNO")
            .takes_value(true)
            .help("Current seqno from config contract"))
        .arg(Arg::with_name("CONFIG_MASTER_KEY_FILE")
            .takes_value(true)
            .help("path to config-master files"))
        .arg(Arg::with_name("NEW_PARAM_FILE")
            .takes_value(true)
            .help("New config param value"));

    let bcconfig_cmd = SubCommand::with_name("dump")
        .about("Commands to dump network entities.")
        .version(version_string)
        .author(author)
        .subcommand(SubCommand::with_name("config")
            .about("Dumps the blockchain config for the last key block.")
            .arg(Arg::with_name("PATH")
                .required(true)
                .takes_value(true)
                .help("Path to the file where to save the blockchain config.")))
        .subcommand(SubCommand::with_name("account")
            .about("Dumps state of given accounts.")
            .setting(AppSettings::AllowLeadingHyphen)
            .arg(Arg::with_name("ADDRESS")
                .required(true)
                .takes_value(true)
                .help("List of addresses.")
                .multiple(true))
            .arg(Arg::with_name("PATH")
                .takes_value(true)
                .long("--path")
                .short("-p")
                .help("Path to folder where to store the dumped accounts. Default value is \".\".")));

    let nodeid_cmd = SubCommand::with_name("nodeid")
        .about("Calculates node ID from the validator public key")
        .arg(Arg::with_name("KEY")
            .long("--pubkey")
            .takes_value(true)
            .help("Validator public key."))
        .arg(Arg::with_name("KEY_PAIR")
            .long("--keypair")
            .takes_value(true)
            .help("Validator seed phrase or path to the file with keypair."));

    let sendfile_cmd = SubCommand::with_name("sendfile")
        .about("Sends the boc file with an external inbound message to account.")
        .arg(Arg::with_name("BOC")
            .required(true)
            .takes_value(true)
            .help("Message boc file."));

    let fetch_block_cmd = SubCommand::with_name("fetch-block")
        .about("Fetches a block.")
        .arg(Arg::with_name("BLOCKID")
            .required(true)
            .takes_value(true)
            .help("Block ID."))
        .arg(Arg::with_name("OUTPUT")
            .required(true)
            .takes_value(true)
            .help("Output file name"));

    let fetch_cmd = SubCommand::with_name("fetch")
        .about("Fetches account's zerostate and transactions.")
        .setting(AppSettings::AllowLeadingHyphen)
        .arg(address_arg.clone().help("Account address to fetch zerostate and txns for."))
        .arg(Arg::with_name("OUTPUT")
            .required(true)
            .takes_value(true)
            .help("Output file name"));

    let replay_cmd = SubCommand::with_name("replay")
        .about("Replays account's transactions starting from zerostate.")
        .arg(Arg::with_name("CONFIG_TXNS")
            .long("--config")
            .short("-c")
            .takes_value(true)
            .help("File containing zerostate and txns of -1:555..5 account.")
            .conflicts_with("DEFAULT_CONFIG"))
        .arg(Arg::with_name("INPUT_TXNS")
            .required(true)
            .takes_value(true)
            .help("File containing zerostate and txns of the account to replay."))
        .arg(Arg::with_name("TXNID")
            .required(true)
            .takes_value(true)
            .help("Dump account state before this transaction ID and stop replaying."))
        .arg(Arg::with_name("DEFAULT_CONFIG")
            .help("Replay transaction with current network config or default if it is not available.")
            .long("--default_config")
            .short("-e")
            .conflicts_with("CONFIG_TXNS"));

    let version = format!("{}\nCOMMIT_ID: {}\nBUILD_DATE: {}\nCOMMIT_DATE: {}\nGIT_BRANCH: {}",
                          env!("CARGO_PKG_VERSION"),
                          env!("BUILD_GIT_COMMIT"),
                          env!("BUILD_TIME"),
                          env!("BUILD_GIT_DATE"),
                          env!("BUILD_GIT_BRANCH"));
    let plugins = Plugins::new(Path::new("plugins").into());
    let link_names_help = format!("Link plugin name ({})", plugins.link_names().join("|"));
    let matches = App::new("tonos_cli")
        .version(&*version)
        .author(author)
        .about("TONLabs console tool for TON")
        .arg(Arg::with_name("LINK")
            .help(link_names_help.as_str())
            .long("--link")
            .takes_value(true))
        .arg(Arg::with_name("NETWORK")
            .help("Network to connect.")
            .short("-u")
            .long("--url")
            .takes_value(true))
        .arg(Arg::with_name("CONFIG")
            .help("Path to the tonos-cli configuration file.")
            .short("-c")
            .long("--config")
            .takes_value(true))
        .arg(Arg::with_name("JSON")
            .help("Cli prints output in json format.")
            .short("-j")
            .long("--json"))
        .subcommand(version_cmd)
        .subcommand(genphrase_cmd)
        .subcommand(genpubkey_cmd)
        .subcommand(getkeypair_cmd)
        .subcommand(genaddr_cmd)
        .subcommand(deploy_cmd
            .arg(alias_arg_long.clone()))
        .subcommand(deploy_message_cmd)
        .subcommand(call_cmd)
        .subcommand(send_cmd)
        .subcommand(message_cmd)
        .subcommand(body_cmd)
        .subcommand(sign_cmd)
        .subcommand(run_cmd)
        .subcommand(runget_cmd)
        .subcommand(config_cmd)
        .subcommand(account_cmd)
        .subcommand(account_wait_cmd)
        .subcommand(query_raw)
        .subcommand(fee_cmd)
        .subcommand(proposal_cmd)
        .subcommand(create_multisig_command())
        .subcommand(create_depool_command())
        .subcommand(create_decode_command())
        .subcommand(create_debot_command())
        .subcommand(create_debug_command())
        .subcommand(create_test_command())
        .subcommand(getconfig_cmd)
        .subcommand(bcconfig_cmd)
        .subcommand(nodeid_cmd)
        .subcommand(sendfile_cmd)
        .subcommand(fetch_block_cmd)
        .subcommand(fetch_cmd)
        .subcommand(replay_cmd)
        .subcommand(callx_cmd)
        .subcommand(deployx_cmd)
        .subcommand(runx_cmd)
        .subcommand(update_config_param_cmd)
        .setting(AppSettings::SubcommandRequired);
#[cfg(feature = "sold")]
    let matches = matches.subcommand(create_compile_command());

    let matches = matches.get_matches_safe()
        .map_err(|e| match e.kind {
            clap::ErrorKind::VersionDisplayed => { println!(); exit(0); },
            clap::ErrorKind::HelpDisplayed => { println!("{}", e); exit(0); },
            _ => {
                eprintln!("{}", e);
                format!("{:#}", json!({"Error": e.message}))
            }
        })?;

    let is_json = matches.is_present("JSON");

    command_parser(&matches, is_json).await
        .map_err(|e| {
            if e.is_empty() {
                e
            } else if is_json {
                let e = serde_json::from_str(&e).unwrap_or(Value::String(e));
                format!("{:#}", json!({"Error": e}))
            } else {
                format!("Error: {e}")
            }
        })
}

async fn command_parser(matches: &ArgMatches<'_>, is_json: bool) -> Result <(), String> {
    let config_file = matches.value_of("CONFIG").map(|v| v.to_string())
        .or(env::var("TONOSCLI_CONFIG").ok())
        .unwrap_or(default_config_name());

    let mut full_config = FullConfig::from_file(&config_file);

    if let Some(m) = matches.subcommand_matches("config") {
        return config_command(m, full_config, is_json);
    }

    full_config.config.is_json |= is_json;
    let config = &mut full_config.config;

    if let Some(url) = matches.value_of("NETWORK") {
        let resolved_url = resolve_net_name(url).unwrap_or(url.to_owned());
        let empty : Vec<String> = Vec::new();
        config.endpoints = full_config.endpoints_map.get(&resolved_url).unwrap_or(&empty).clone();
        config.url = resolved_url;
    }

    if let Some(link_name) = matches.value_of("LINK") {
        config.link = Some(link_name.to_string());
    }

    if let Some(m) = matches.subcommand_matches("callx") {
        return callx_command(m, &full_config).await;
    }
    if let Some(m) = matches.subcommand_matches("runx") {
        return run_command(m, &full_config, true).await;
    }
    if let Some(m) = matches.subcommand_matches("deployx") {
        return deployx_command(m, &mut full_config).await;
    }
    if let Some(m) = matches.subcommand_matches("call") {
        return call_command(m, config, CallType::Call).await;
    }
    if let Some(m) = matches.subcommand_matches("run") {
        return run_command(m, &full_config, false).await;
    }
    if let Some(m) = matches.subcommand_matches("runget") {
        return runget_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("body") {
        return body_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("sign") {
        return test_sign_command(m, config);
    }
    if let Some(m) = matches.subcommand_matches("message") {
        return call_command(m, config, CallType::Msg).await;
    }
    if let Some(m) = matches.subcommand_matches("send") {
        return send_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("deploy") {
        return deploy_command(m, &mut full_config, DeployType::Full).await;
    }
    if let Some(m) = matches.subcommand_matches("deploy_message") {
        return deploy_command(m, &mut full_config, DeployType::MsgOnly).await;
    }
    if let Some(m) = matches.subcommand_matches("genaddr") {
        return genaddr_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("getkeypair") {
        return getkeypair_command(m, config);
    }
    if let Some(m) = matches.subcommand_matches("account") {
        return account_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("fee") {
        if let Some(m) = m.subcommand_matches("storage") {
            return storage_command(m, config).await;
        }
        if let Some(m) = m.subcommand_matches("deploy") {
            return deploy_command(m, &mut full_config, DeployType::Fee).await;
        }
        if let Some(m) = m.subcommand_matches("call") {
            return call_command(m, config, CallType::Fee).await;
        }
    }
    if let Some(m) = matches.subcommand_matches("genphrase") {
        return genphrase_command(m, config);
    }
    if let Some(m) = matches.subcommand_matches("genpubkey") {
        return genpubkey_command(m, config);
    }
    if let Some(m) = matches.subcommand_matches("proposal") {
        if let Some(m) = m.subcommand_matches("create") {
            return proposal_create_command(m, config).await;
        }
        if let Some(m) = m.subcommand_matches("vote") {
            return proposal_vote_command(m, config).await;
        }
        if let Some(m) = m.subcommand_matches("decode") {
            return proposal_decode_command(m, config).await;
        }
    }
    if let Some(m) = matches.subcommand_matches("multisig") {
        return multisig_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("depool") {
        return depool_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("getconfig") {
        return getconfig_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("update_config") {
        return update_config_command(m, config).await;
    }
    if let Some(matches) = matches.subcommand_matches("dump") {
        if let Some(m) = matches.subcommand_matches("config") {
            return dump_bc_config_command(m, config).await;
        }
        if let Some(m) = matches.subcommand_matches("account") {
            return dump_accounts_command(m, config).await;
        }
    }
    if let Some(m) = matches.subcommand_matches("account-wait") {
        return account_wait_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("query-raw") {
        return query_raw_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("nodeid") {
        return nodeid_command(m, config);
    }
    if let Some(m) = matches.subcommand_matches("sendfile") {
        return sendfile_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("decode") {
        return decode_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("debug") {
        return debug_command(m, &full_config).await;
    }
    if let Some(m) = matches.subcommand_matches("debot") {
        return debot_command(m, config.to_owned()).await;
    }
    if let Some(m) = matches.subcommand_matches("fetch-block") {
        return fetch_block_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("fetch") {
        return fetch_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("replay") {
        return replay_command(m, config).await;
    }
    if let Some(m) = matches.subcommand_matches("test") {
        return test_command(m, &full_config).await;
    }
#[cfg(feature = "sold")]
    if let Some(m) = matches.subcommand_matches("compile") {
        return compile_command(m, &config).await;
    }
    if matches.subcommand_matches("version").is_some() {
        if config.is_json {
            println!("{{");
            println!(r#"  "tonos-cli": "{}","#, env!("CARGO_PKG_VERSION"));
            println!(r#"  "COMMIT_ID": "{}","#, env!("BUILD_GIT_COMMIT"));
            println!(r#"  "BUILD_DATE": "{}","#, env!("BUILD_TIME"));
            println!(r#"  "COMMIT_DATE": "{}","#, env!("BUILD_GIT_DATE"));
            println!(r#"  "GIT_BRANCH": "{}""#, env!("BUILD_GIT_BRANCH"));
            println!("}}");
        } else {
            println!(
                "tonos-cli {}\nCOMMIT_ID: {}\nBUILD_DATE: {}\nCOMMIT_DATE: {}\nGIT_BRANCH: {}",
                env!("CARGO_PKG_VERSION"),
                env!("BUILD_GIT_COMMIT"),
                env!("BUILD_TIME"),
                env!("BUILD_GIT_DATE"),
                env!("BUILD_GIT_BRANCH")
            );
        }
        return Ok(());
    }
    Err("invalid arguments".to_string())
}

fn genphrase_command(matches: &ArgMatches, config: &Config) -> Result<(), String> {
    generate_mnemonic(matches.value_of("DUMP_KEYPAIR"), config)
}

fn genpubkey_command(matches: &ArgMatches, config: &Config) -> Result<(), String> {
    let mnemonic = matches.value_of("PHRASE").unwrap();
    extract_pubkey(mnemonic, config.is_json)
}

fn getkeypair_command(matches: &ArgMatches, config: &Config) -> Result<(), String> {
    let key_file = matches.value_of("KEY_FILE");
    let phrase = matches.value_of("PHRASE");
    if !config.is_json {
        print_args!(key_file, phrase);
    }
    generate_keypair(key_file, phrase, config)
}

async fn send_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let message = matches.value_of("MESSAGE");
    let abi = Some(abi_from_matches_or_config(matches, &config)?);

    if !config.is_json {
        print_args!(message, abi);
    }

    call_contract_with_msg(config, message.unwrap().to_owned(), &abi.unwrap()).await
}

async fn body_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let method = matches.value_of("METHOD");
    let params = matches.value_of("PARAMS");
    let output = matches.value_of("OUTPUT");
    let abi = Some(abi_from_matches_or_config(matches, &config)?);
    let params = Some(load_params(params.unwrap())?);
    if !config.is_json {
        print_args!(method, params, abi, output);
    }

    let params = serde_json::from_str(&params.unwrap())
        .map_err(|e| format!("arguments are not in json format: {}", e))?;

    let client = create_client_local()?;
    let body = ton_client::abi::encode_message_body(
        client.clone(),
        ParamsOfEncodeMessageBody {
            abi: load_abi(abi.as_ref().unwrap(), config).await?,
            call_set: CallSet::some_with_function_and_input(method.unwrap(), params)
                .ok_or("failed to create CallSet with specified parameters.")?,
            is_internal: true,
            ..Default::default()
        },
    ).await
    .map_err(|e| format!("failed to encode body: {}", e))
    .map(|r| r.body)?;

    if !config.is_json {
        println!("Message body: {}", body);
    } else {
        println!("{{");
        println!("  \"Message\": \"{}\"", body);
        println!("}}");
    }

    Ok(())
}

async fn call_command(matches: &ArgMatches<'_>, config: &Config, call: CallType) -> Result<(), String> {
    let address = matches.value_of("ADDRESS");
    let method = matches.value_of("METHOD");
    let params = matches.value_of("PARAMS");
    let lifetime = matches.value_of("LIFETIME");
    let raw = matches.is_present("RAW");
    let output = matches.value_of("OUTPUT");

    let abi = Some(abi_from_matches_or_config(matches, &config)?);

    let keys = matches.value_of("KEYS")
        .or(matches.value_of("SIGN"))
        .map(|s| s.to_string())
        .or(config.keys_path.clone());

    let params = Some(load_params(params.unwrap())?);
    if !config.is_json {
        print_args!(address, method, params, abi, keys, lifetime, output);
    }
    let address = load_ton_address(address.unwrap(), &config)?;

    match call {
        CallType::Call | CallType::Fee => {
            let is_fee = if let CallType::Fee = call { true } else { false };
            call_contract(
                config,
                address.as_str(),
                &abi.unwrap(),
                method.unwrap(),
                &params.unwrap(),
                keys,
                is_fee,
            ).await
        },
        CallType::Msg => {
            let lifetime = lifetime.map(|val| {
                    u32::from_str_radix(val, 10)
                        .map_err(|e| format!("Failed to parse lifetime: {e}"))
                })
                .transpose()?
                .unwrap_or(DEF_MSG_LIFETIME);
            let timestamp = matches.value_of("TIMESTAMP").map(|val| {
                u64::from_str_radix(val, 10)
                    .map_err(|e| format!("Failed to parse timestamp: {e}"))
            }).transpose()?;
            generate_message(
                config,
                address.as_str(),
                &abi.unwrap(),
                method.unwrap(),
                &params.unwrap(),
                keys,
                lifetime,
                raw,
                output,
                timestamp,
            ).await
        },
    }
}

async fn callx_command(matches: &ArgMatches<'_>, full_config: &FullConfig) -> Result<(), String> {
    let config = &full_config.config;
    let method = Some(matches.value_of("METHOD").or(config.method.as_deref())
        .ok_or("Method is not defined. Supply it in the config file or command line.")?);
    let (address, abi, keys) = contract_data_from_matches_or_config_alias(matches, full_config)?;
    let params = unpack_alternative_params(
        matches,
        abi.as_ref().unwrap(),
        method.unwrap(),
        config
    ).await?;
    let params = Some(load_params(&params)?);

    if !config.is_json {
        print_args!(address, method, params, abi, keys);
    }

    let address = load_ton_address(address.unwrap().as_str(), &config)?;

    call_contract(
        config,
        address.as_str(),
        &abi.unwrap(),
        &method.unwrap(),
        &params.unwrap(),
        keys,
        false,
    ).await
}

async fn runget_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let address = matches.value_of("ADDRESS");
    let method = matches.value_of("METHOD");
    let params = matches.values_of("PARAMS");
    let params = params.map(|values| {
        json!(values.collect::<Vec<_>>()).to_string()
    });
    if !config.is_json {
        print_args!(address, method, params);
    }
    let source_type = if matches.is_present("TVC") {
        AccountSource::TVC
    } else if matches.is_present("BOC") {
        AccountSource::BOC
    } else {
        AccountSource::NETWORK
    };
    let address =  if source_type != AccountSource::NETWORK {
        address.unwrap().to_string()
    } else {
        load_ton_address(address.unwrap(), &config)?
    };
    let bc_config = matches.value_of("BCCONFIG");
    run_get_method(config, &address, method.unwrap(), params, source_type, bc_config).await
}


async fn deploy_command(matches: &ArgMatches<'_>, full_config: &mut FullConfig, deploy_type: DeployType) -> Result<(), String> {
    let config = &full_config.config;
    let tvc = matches.value_of("TVC");
    let wc = wc_from_matches_or_config(matches, config)?;
    let raw = matches.is_present("RAW");
    let output = matches.value_of("OUTPUT");
    let abi = Some(abi_from_matches_or_config(matches, config)?);
    let keys = matches.value_of("KEYS")
            .or(matches.value_of("SIGN"))
            .map(|s| s.to_string())
            .or(config.keys_path.clone());
    let alias = matches.value_of("ALIAS");
    let params = Some(unpack_alternative_params(
        matches,
        abi.as_ref().unwrap(),
        "constructor",
        config
    ).await?);
    if !config.is_json {
        let opt_wc = Some(format!("{}", wc));
        print_args!(tvc, params, abi, keys, opt_wc, alias);
    }
    match deploy_type {
        DeployType::Full => deploy_contract(full_config, tvc.unwrap(), &abi.unwrap(), &params.unwrap(), keys, wc, false, alias).await,
        DeployType::MsgOnly => generate_deploy_message(tvc.unwrap(), &abi.unwrap(), &params.unwrap(), keys, wc, raw, output, config).await,
        DeployType::Fee => deploy_contract(full_config, tvc.unwrap(), &abi.unwrap(), &params.unwrap(), keys, wc, true, None).await,
    }
}

async fn deployx_command(matches: &ArgMatches<'_>, full_config: &mut FullConfig) -> Result<(), String> {
    let config = &full_config.config;
    let tvc = matches.value_of("TVC");
    let wc = wc_from_matches_or_config(matches, config)?;
    let abi = Some(abi_from_matches_or_config(matches, &config)?);
    let params = Some(unpack_alternative_params(
        matches,
        abi.as_ref().unwrap(),
        "constructor",
        config
    ).await?);
    let keys = matches.value_of("KEYS")
        .map(|s| s.to_string())
        .or(config.keys_path.clone());

    let alias = matches.value_of("ALIAS");
    if !config.is_json {
        let opt_wc = Some(format!("{}", wc));
        print_args!(tvc, params, abi, keys, opt_wc, alias);
    }
    deploy_contract(full_config, tvc.unwrap(), &abi.unwrap(), &params.unwrap(), keys, wc, false, alias).await
}

fn config_command(matches: &ArgMatches, mut full_config: FullConfig, is_json: bool) -> Result<(), String> {
    let mut result = Ok(());
    if matches.is_present("GLOBAL") {
        full_config = FullConfig::from_file(&global_config_path());
    }
    if !matches.is_present("LIST") {
        if let Some(clear_matches) = matches.subcommand_matches("clear") {
            result = clear_config(&mut full_config, clear_matches, is_json);
        } else if let Some(endpoint_matches) = matches.subcommand_matches("endpoint") {
            if let Some(endpoint_matches) = endpoint_matches.subcommand_matches("add") {
                let url = endpoint_matches.value_of("URL").unwrap();
                let endpoints = endpoint_matches.value_of("ENDPOINTS").unwrap();
                FullConfig::add_endpoint(full_config.path.as_str(), url, endpoints)?;
            } else if let Some(endpoint_matches) = endpoint_matches.subcommand_matches("remove") {
                let url = endpoint_matches.value_of("URL").unwrap();
                FullConfig::remove_endpoint(full_config.path.as_str(), url)?;
            } else if endpoint_matches.subcommand_matches("reset").is_some() {
                FullConfig::reset_endpoints(full_config.path.as_str())?;
            }
            FullConfig::print_endpoints(full_config.path.as_str());
            return Ok(());
        } else if let Some(alias_matches) = matches.subcommand_matches("alias") {
            if let Some(alias_matches) = alias_matches.subcommand_matches("add") {
                full_config.add_alias(
                    alias_matches.value_of("ALIAS").unwrap(),
                    alias_matches.value_of("ADDRESS").map(|s| s.to_string()),
                    alias_matches.value_of("ABI").map(|s| s.to_string()),
                    alias_matches.value_of("KEYS").map(|s| s.to_string()),
                )?
            } else if let Some(alias_matches) = alias_matches.subcommand_matches("remove") {
                full_config.remove_alias(alias_matches.value_of("ALIAS").unwrap())?
            } else if let Some(_) = alias_matches.subcommand_matches("reset") {
                full_config.aliases = BTreeMap::new();
                full_config.to_file(&full_config.path)?;
            }
            full_config.print_aliases();
            return Ok(());
        } else {
            if matches.args.is_empty() {
                return Err("At least one option must be specified".to_string());
            }

            result = set_config(&mut full_config, matches, is_json);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&full_config.config)
            .map_err(|e| format!("failed to print config parameters: {}", e))?
    );
    result
}

async fn genaddr_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let tvc = matches.value_of("TVC");
    let wc = matches.value_of("WC");
    let keys = matches.value_of("GENKEY").or(matches.value_of("SETKEY"));
    let new_keys = matches.is_present("GENKEY");
    let init_data = matches.value_of("DATA");
    let update_tvc = matches.is_present("SAVE");
    let abi = match abi_from_matches_or_config(matches, config) {
        Ok(abi) => Some(abi),
        Err(err) => {
            match load_abi_from_tvc(tvc.clone().unwrap()) {
                Some(abi) => Some(abi),
                None => return Err(err)
            }
        }
    };
    let is_update_tvc = if update_tvc { Some("true") } else { None };
    if !config.is_json {
        print_args!(tvc, abi, wc, keys, init_data, is_update_tvc);
    }
    generate_address(config, tvc.unwrap(), &abi.unwrap(), wc, keys, new_keys, init_data, update_tvc).await
}

async fn account_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let addresses_list = matches.values_of("ADDRESS")
        .map(|val| val.collect::<Vec<_>>())
        .or(config.addr.as_ref().map(|addr| vec![addr.as_str()]))
        .ok_or("Address was not found. It must be specified as option or in the config file."
            .to_string())?;
    if addresses_list.len() > 1 &&
        (matches.is_present("DUMPTVC") || matches.is_present("DUMPTVC")) {
        return Err("`DUMPTVC` and `DUMPBOC` options are not applicable to a list of addresses.".to_string());
    }
    let is_boc = matches.is_present("BOC");
    let mut formatted_list = vec![];
    for address in addresses_list.iter() {
        if !is_boc {
            let formatted = load_ton_address(address, &config)?;
            formatted_list.push(formatted);
        } else {
            if !std::path::Path::new(address).exists() {
                return  Err(format!("File {} doesn't exist.", address));
            }
            formatted_list.push(address.to_string());
        }
    }
    let tvcname = matches.value_of("DUMPTVC");
    let bocname = matches.value_of("DUMPBOC");
    let addresses = Some(formatted_list.join(", "));
    println!("link_name: {}", config.link.clone().unwrap_or("front".to_string()));
    if !config.is_json {
        print_args!(addresses);
    }
    get_account(&config, formatted_list, tvcname, bocname, is_boc).await
}

async fn dump_accounts_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let addresses_list = matches.values_of("ADDRESS").unwrap().collect::<Vec<_>>();
    let mut formatted_list = vec![];
    for address in addresses_list.iter() {
        let formatted = load_ton_address(address, &config)?;
        formatted_list.push(formatted);
    }
    let path = matches.value_of("PATH");
    let addresses = Some(formatted_list.join(", "));
    if !config.is_json {
        print_args!(addresses, path);
    }
    dump_accounts(config, formatted_list, path).await
}

async fn account_wait_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let address = matches.value_of("ADDRESS").unwrap();
    let address = load_ton_address(address, &config)?;
    let timeout = matches.value_of("TIMEOUT").unwrap_or("30").parse::<u64>()
        .map_err(|e| format!("failed to parse timeout: {}", e))?;
    wait_for_change(config, &address, timeout).await
}

async fn query_raw_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let collection = matches.value_of("COLLECTION").unwrap();
    let filter = matches.value_of("FILTER");
    let limit = matches.value_of("LIMIT");
    let order = matches.value_of("ORDER");
    let result = matches.value_of("RESULT").unwrap();
    query_raw(config, collection, filter, limit, order, result).await
}

async fn storage_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let address = matches.value_of("ADDRESS");
    let period = matches.value_of("PERIOD");
    if !config.is_json {
        print_args!(address, period);
    }
    let address = load_ton_address(address.unwrap(), &config)?;
    let period = period.map(|val| {
        u32::from_str_radix(val, 10)
            .map_err(|e| format!("failed to parse period: {}", e))
    })
    .transpose()?
    .unwrap_or(DEF_STORAGE_PERIOD);
    calc_storage(&config, address.as_str(), period).await
}

async fn proposal_create_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let address = matches.value_of("ADDRESS");
    let dest = matches.value_of("DEST");
    let keys = matches.value_of("KEYS");
    let comment = matches.value_of("COMMENT");
    let lifetime = matches.value_of("LIFETIME");
    let offline = matches.is_present("OFFLINE");
    if !config.is_json {
        print_args!(address, comment, keys, lifetime);
    }
    let address = load_ton_address(address.unwrap(), &config)?;
    let lifetime = parse_lifetime(lifetime, config)?;

    create_proposal(
        config,
        address.as_str(),
        keys,
        dest.unwrap(),
        comment.unwrap(),
        lifetime,
        offline
    ).await
}

async fn proposal_vote_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let address = matches.value_of("ADDRESS");
    let keys = matches.value_of("KEYS");
    let id = matches.value_of("ID");
    let lifetime = matches.value_of("LIFETIME");
    let offline = matches.is_present("OFFLINE");
    if !config.is_json {
        print_args!(address, id, keys, lifetime);
    }
    let address = load_ton_address(address.unwrap(), &config)?;
    let lifetime = parse_lifetime(lifetime, config)?;

    vote(config, address.as_str(), keys, id.unwrap(), lifetime, offline).await?;
    println!("{{}}");
    Ok(())
}

async fn proposal_decode_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let address = matches.value_of("ADDRESS");
    let id = matches.value_of("ID");
    if !config.is_json {
        print_args!(address, id);
    }
    let address = load_ton_address(address.unwrap(), &config)?;
    decode_proposal(config, address.as_str(), id.unwrap()).await
}

async fn getconfig_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let index = matches.value_of("INDEX");
    if !config.is_json {
        print_args!(index);
    }
    query_global_config(config, index).await
}

async fn update_config_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let abi = matches.value_of("ABI");
    let seqno = matches.value_of("SEQNO");
    let config_master = matches.value_of("CONFIG_MASTER_KEY_FILE");
    let new_param = matches.value_of("NEW_PARAM_FILE");
    if !config.is_json {
        print_args!(seqno, config_master, new_param);
    }
    gen_update_config_message(abi, seqno, config_master.unwrap(), new_param.unwrap(), config.is_json).await
}

async fn dump_bc_config_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let path = matches.value_of("PATH");
    if !config.is_json {
        print_args!(path);
    }
    dump_blockchain_config(config, path.unwrap()).await
}

fn nodeid_command(matches: &ArgMatches, config: &Config) -> Result<(), String> {
    let key = matches.value_of("KEY");
    let keypair = matches.value_of("KEY_PAIR");
    if !config.is_json {
        print_args!(key, keypair);
    }
    let nodeid = if let Some(key) = key {
        let vec = hex::decode(key)
            .map_err(|e| format!("failed to decode public key: {}", e))?;
        convert::nodeid_from_pubkey(&vec)?
    } else if let Some(pair) = keypair {
        let pair = crypto::load_keypair(pair)?;
        convert::nodeid_from_pubkey(&hex::decode(&pair.public)
            .map_err(|e| format!("failed to decode public key: {}", e))?)?
    } else {
        return Err("Either public key or key pair parameter should be provided".to_owned());
    };
    if !config.is_json {
        println!("{}", nodeid);
    } else {
        println!("{{");
        println!("  \"nodeid\": \"{}\"", nodeid);
        println!("}}");
    }
    Ok(())
}

async fn sendfile_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let boc = m.value_of("BOC");
    if !config.is_json {
        print_args!(boc);
    }
    sendfile::sendfile(config, boc.unwrap()).await
}
