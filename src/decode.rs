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
use crate::{load_abi, print_args};
use crate::config::Config;
use crate::decode::msg_printer::tree_of_cells_into_base64;
use crate::helpers::{decode_msg_body, print_account, create_client_local, create_client_verbose, query_account_field, abi_from_matches_or_config, load_ton_address, load_ton_abi, create_client, query_message};
use clap::{ArgMatches, SubCommand, Arg, App, AppSettings};
use ton_types::{Cell, SliceData, write_boc, read_single_root_boc};
use ton_block::{Account, Deserializable, Serializable, AccountStatus, StateInit};
use ton_client::abi::{decode_account_data, ParamsOfDecodeAccountData};
use serde::Serialize;
use serde_json::json;

pub fn create_decode_command<'a, 'b>() -> App<'a, 'b> {
    let tvc_cmd = SubCommand::with_name("stateinit")
        .setting(AppSettings::AllowLeadingHyphen)
        .about("Decodes tvc data (including compiler version) from different sources.")
        .arg(Arg::with_name("TVC")
            .long("--tvc")
            .conflicts_with("BOC")
            .help("Contract is passed via path to the TVC file."))
        .arg(Arg::with_name("BOC")
            .long("--boc")
            .conflicts_with("TVC")
            .help("Contract is passed via path to the account BOC file."))
        .arg(Arg::with_name("INPUT")
            .required(true)
            .help("Contract address or path to the file with contract data."));
    SubCommand::with_name("decode")
        .about("Decode commands.")
        .setting(AppSettings::AllowLeadingHyphen)
        .setting(AppSettings::TrailingVarArg)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .subcommand(SubCommand::with_name("body")
            .about("Decodes body base64 string.")
            .arg(Arg::with_name("BODY")
                .required(true)
                .help("Message body encoded as base64."))
            .arg(Arg::with_name("ABI")
                .long("--abi")
                .takes_value(true)
                .help("Path or link to the contract ABI file or pure json ABI data. Can be specified in the config file.")))
        .subcommand(SubCommand::with_name("msg")
            .about("Decodes message file.")
            .arg(Arg::with_name("MSG")
                .required(true)
                .help("Path to the message boc file (with binary data), message in base64 or message id."))
            .arg(Arg::with_name("ABI")
                .long("--abi")
                .takes_value(true)
                .help("Path or link to the contract ABI file or pure json ABI data. Can be specified in the config file."))
            .arg(Arg::with_name("BASE64")
                .long("--base64")
                .help("Flag that changes behavior of the command to work with data in base64 (FLAG IS DEPRECATED).")))
        .subcommand(tvc_cmd)
        .subcommand(SubCommand::with_name("account")
            .about("Top level command of account decode commands.")
            .subcommand(SubCommand::with_name("data")
                .setting(AppSettings::AllowLeadingHyphen)
                .about("Decodes data fields from the contract state.")
                .arg(Arg::with_name("TVC")
                    .long("--tvc")
                    .short("-t")
                    .takes_value(true)
                    .help("Path to the tvc file with contract state.")
                    .conflicts_with("ADDRESS"))
                .arg(Arg::with_name("ADDRESS")
                    .long("--addr")
                    .short("-a")
                    .takes_value(true)
                    .help("Contract address.")
                    .conflicts_with("TVC"))
                .arg(Arg::with_name("ABI")
                    .long("--abi")
                    .takes_value(true)
                    .help("Path or link to the contract ABI file or pure json ABI data. Can be specified in the config file.")))
            .subcommand(SubCommand::with_name("boc")
                .about("Decodes data from the file with boc of the account and saves contract tvc file if needed.")
                .arg(Arg::with_name("BOCFILE")
                    .required(true)
                    .help("Path to the account boc file."))
                .arg(Arg::with_name("DUMPTVC")
                    .long("--dumptvc")
                    .short("-d")
                    .takes_value(true)
                    .help("Path to the TVC file where to save the dump."))))
}

pub async fn decode_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    if let Some(m) = m.subcommand_matches("body") {
        return decode_body_command(m, config).await;
    }
    if let Some(m) = m.subcommand_matches("msg") {
        return decode_message_command(m, config).await;
    }
    if let Some(m) = m.subcommand_matches("stateinit") {
        return decode_tvc_command(m, config).await;
    }
    if let Some(m) = m.subcommand_matches("account") {
        if let Some(m) = m.subcommand_matches("boc") {
            return decode_account_from_boc(m, config).await;
        }
        if let Some(m) = m.subcommand_matches("data") {
            return decode_data_command(m, config).await;
        }
    }
    Err("unknown command".to_owned())
}

async fn decode_data_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    if m.is_present("TVC") {
        return decode_tvc_fields(m, config).await;
    }
    if m.is_present("ADDRESS") {
        return decode_account_fields(m, config).await;
    }
    Err("unknown command".to_owned())
}

async fn decode_body_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let body = m.value_of("BODY");
    let abi = Some(abi_from_matches_or_config(m, config)?);
    if !config.is_json {
        print_args!(body, abi);
    }
    decode_body(body.unwrap(), &abi.unwrap(), config.is_json, config).await?;
    Ok(())
}

async fn decode_account_from_boc(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let boc = m.value_of("BOCFILE");
    let tvc_path = m.value_of("DUMPTVC");

    if !config.is_json {
        print_args!(boc, tvc_path);
    }

    let account = Account::construct_from_file(boc.unwrap())
        .map_err(|e| format!(" failed to load account from the boc file: {}", e))?;

    print_account_data(&account, tvc_path, config, true).await
}

pub async fn print_account_data(account: &Account, tvc_path: Option<&str>, config: &Config, decode_stateinit: bool) -> Result<(), String> {
    if account.is_none() {
        if !config.is_json {
            println!("\nAccount is None");
        } else {
            println!("{{");
            println!("  \"Account\": \"None\"");
            println!("}}");
        }
        return Ok(());
    }
    let state_init = account.state_init();

    let address = match account.get_addr() {
        Some(address) => format!("{}", address),
        _ => "Undefined".to_owned(),
    };

    let state = match account.status() {
        AccountStatus::AccStateUninit => "Uninit".to_owned(),
        AccountStatus::AccStateFrozen => "Frozen".to_owned(),
        AccountStatus::AccStateActive => "Active".to_owned(),
        AccountStatus::AccStateNonexist => "NonExist".to_owned(),
    };

    let balance = match account.balance() {
        Some(balance) => format!("{}", balance.grams.clone()),
        _ => "Undefined".to_owned(),
    };

    let trans_lt = account.last_tr_time()
        .map_or("Undefined".to_owned(), |v| format!("{:#x}", v));
    let paid = format!("{}", account.last_paid());

    let (si, code_hash) = match state_init {
        Some(state_init) => {
            let code = state_init.code.clone()
                .ok_or("failed to obtain code from the StateInit")?;
            let ton = create_client_local()?;
            (
                serde_json::to_string_pretty(
                    &msg_printer::serialize_state_init(state_init, ton)
                        .await?)
                 .map_err(|e| format!("Failed to serialize stateInit: {}", e))?,
                Some(code.repr_hash().to_hex_string())
            )
        },
        _ => ("Undefined".to_owned(), None)
    };

    let data = tree_of_cells_into_base64(account.get_data().as_ref())?;
    let data = hex::encode(base64::decode(data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?);
    print_account(
        config,
        Some(state),
        Some(address),
        Some(balance),
        Some(paid),
        Some(trans_lt),
        Some(data),
        code_hash,
        if decode_stateinit { Some(si) } else { None },
    );

    if tvc_path.is_some() && state_init.is_some() {
        state_init.unwrap().write_to_file(tvc_path.unwrap())
            .map_err(|e| format!("{}", e))?;
    }

    Ok(())
}

async fn decode_message_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let msg = m.value_of("MSG");
    let abi = Some(abi_from_matches_or_config(m, config)?);
    if !config.is_json {
        print_args!(msg, abi);
    }
    if m.is_present("BASE64") && !config.is_json {
        println!("Flag --base64 is deprecated. Command can be used for base64 input without this flag.")
    }
    let input = msg.unwrap();
    let decoded_message = if std::path::Path::new(input).exists() {
        let msg_bytes = std::fs::read(input)
            .map_err(|e| format!(" failed to read msg from file {input}: {}", e))?;
        match decode_message(msg_bytes.clone(), abi.clone()).await {
            Ok(result) => result,
            Err(e) => {
                let message_str = String::from_utf8(msg_bytes)
                    .map_err(|_| format!("Failed to decode message from file: {e}"))?;
                let message_bytes = base64::decode(&message_str)
                    .map_err(|e2| format!("Failed to decode message data: {e2}"))?;
                decode_message(message_bytes, abi).await
                    .map_err(|e2| format!("Failed to decode message from file: {e2}"))?
            }
        }
    } else {
        let base64_decode = base64::decode(input)
            .map_err(|e| format!("{e}"));
        let msg_decode = match base64_decode {
            Ok(base64_decode) => decode_message(base64_decode, abi.clone()).await,
            Err(e) => Err(e)
        };
        match msg_decode {
            Ok(result) => result,
            Err(e) => {
                let ton_client = create_client(config)?;
                let query_boc = query_message(ton_client, input).await
                    .map_err(|e2| format!("Failed to decode message, specify path to the file, message id or message in base64.\nBase64 error: {e}\nQuery error: {e2}"))?;
                let message_bytes = base64::decode(&query_boc)
                    .map_err(|e2| format!("Failed to decode queried message: {e2}"))?;
                decode_message(message_bytes, abi).await
                    .map_err(|e2| format!("Failed to decode queried message: {e2}"))?
            }
        }
    };
    println!("{decoded_message}");
    Ok(())
}

async fn decode_tvc_fields(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let tvc = m.value_of("TVC");
    let abi = Some(abi_from_matches_or_config(m, config)?);
    if !config.is_json {
        print_args!(tvc, abi);
    }
    let abi = load_abi(abi.as_ref().unwrap(), config).await?;
    let state = StateInit::construct_from_file(tvc.unwrap())
        .map_err(|e| format!("failed to load StateInit from the tvc file: {}", e))?;
    let b64 = tree_of_cells_into_base64(state.data.as_ref())?;
    let ton = create_client_local()?;
    let res = decode_account_data(
        ton,
        ParamsOfDecodeAccountData {
                abi,
                data: b64,
                ..Default::default()
            }
        ).map_err(|e| format!("failed to decode data: {}", e))?;
    if !config.is_json {
        println!("TVC fields:");
    }
    println!("{:#}", res.data);
    Ok(())
}

async fn decode_account_fields(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let address = m.value_of("ADDRESS");
    let abi = Some(abi_from_matches_or_config(m, config)?);
    if !config.is_json {
        print_args!(address, abi);
    }
    let abi = load_abi(abi.as_ref().unwrap(), config).await?;

    let ton = create_client_verbose(config)?;
    let address = load_ton_address(address.unwrap(), config)?;
    let data = query_account_field(ton.clone(), &address, "data").await?;

    let res = decode_account_data(
        ton,
        ParamsOfDecodeAccountData {
                abi,
                data,
                ..Default::default()
            }
        ).map_err(|e| format!("failed to decode data: {}", e))?;
    if !config.is_json {
        println!("Account fields:");
    }
    println!("{:#}", res.data);
    Ok(())
}

#[derive(Serialize)]
struct SortedFunctionHeader {
    pubkey: Option<String>,
    time: Option<u64>,
    expire: Option<u32>
}

async fn decode_body(body_base64: &str, abi_path: &str, is_json: bool, config: &Config) -> Result<(), String> {
    let body_vec  = base64::decode(body_base64)
        .map_err(|e| format!("body is not a valid base64 string: {}", e))?;

    let empty_boc = write_boc(&Cell::default())
        .map_err(|e| format!("failed to serialize tree of cells: {}", e))?;
    if body_vec.cmp(&empty_boc) == std::cmp::Ordering::Equal {
        return Err("body is empty".to_string());
    }

    let ton = create_client_local()?;

    let (mut res, is_external) = {
        match decode_msg_body(ton.clone(), abi_path, body_base64, false, config).await {
            Ok(res) => (res, true),
            Err(_) => (decode_msg_body(ton.clone(), abi_path, body_base64, true, config).await?, false),
        }
    };
    let mut signature = None;

    let cell = read_single_root_boc(body_vec)
        .map_err(|e| format!("Failed to create cell: {}", e))?;
    let orig_slice = SliceData::load_cell(cell)
        .map_err(|e| format!("Failed to load cell: {}", e))?;
    if is_external {
        let mut slice = orig_slice.clone();
        let flag = slice.get_next_bit();
        if let Ok(has_sign) = flag {
            if has_sign {
                let signature_bytes = slice.get_next_bytes(64).unwrap();
                signature = Some(hex::encode(signature_bytes));
            }
        }
    }
    let contr = load_ton_abi(abi_path, config).await?;

    let (_, func_id, _) = ton_abi::Function::decode_header(contr.version(), orig_slice.clone(), contr.header(), !is_external)
        .map_err(|e| format!("Failed to decode header: {}", e))?;
    let output = res.value.take().ok_or("failed to obtain the result")?;
    let header = res.header.map(|hdr| {
        SortedFunctionHeader {
            pubkey: hdr.pubkey,
            time: hdr.time,
            expire: hdr.expire
        }
    });
    if is_json {
        let mut result = json!({});
        result["BodyCall"] = json!({res.name: output});
        result["Signature"] = json!(signature.unwrap_or("None".to_string()));
        result["Header"] = json!(header);
        result["FunctionId"] = json!(format!("{:08X}", func_id));
        println!("{:#}", result);
    } else {
        println!("\n\n{}: {:#}", res.name, output);
        println!("Signature: {}", signature.unwrap_or("None".to_string()));
        println!("Header: {:#}", json!(header));
        println!("FunctionId: {:08X}", func_id);
    }
    Ok(())
}

async fn decode_message(msg_boc: Vec<u8>, abi_path: Option<String>) -> Result<String, String> {
    let tvm_msg = ton_sdk::Contract::deserialize_message(&msg_boc[..])
        .map_err(|e| format!("failed to deserialize message boc: {}", e))?;
    let config = Config::default();
    let result = msg_printer::serialize_msg(&tvm_msg, abi_path, &config).await?;
    serde_json::to_string_pretty(&result)
        .map_err(|e| format!("Failed to serialize the result: {}", e))
}

fn load_state_init(m: &ArgMatches<'_>) -> Result<StateInit, String> {
    let input = m.value_of("INPUT").unwrap();
    let stat_init = if m.is_present("BOC") {
        let account = Account::construct_from_file(input)
            .map_err(|e| format!(" failed to load account from the boc file {}: {}", input, e))?;
        account.state_init().ok_or("Failed to load stateInit from the BOC.")?.to_owned()
    } else {
        StateInit::construct_from_file(input)
            .map_err(|e| format!("failed to load StateInit from the tvc file: {}", e))?
    };
    Ok(stat_init)
}

async fn decode_tvc_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let input = m.value_of("INPUT");
    if !config.is_json {
        print_args!(input);
    }
    let is_local = m.is_present("BOC") || m.is_present("TVC");
    let ton = if is_local {
        create_client_local()?
    } else {
        create_client_verbose(config)?
    };
    let input = input.unwrap().to_owned();

    let state = if is_local {
        load_state_init(m)?
    } else {
        let input = if input.contains(':') {
            input
        } else {
            format!("{}:{}", config.wc, input)
        };
        let boc = query_account_field(ton.clone(), &input, "boc").await?;
        let account = Account::construct_from_base64(&boc)
            .map_err(|e| format!("Failed to query account BOC: {}", e))?;
        account.state_init().ok_or("Failed to load stateInit from the BOC.")?.to_owned()
    };

    if !config.is_json {
        println!("Decoded data:");
    }
    let result = msg_printer::serialize_state_init(&state, ton.clone()).await?;
    println!("{}", serde_json::to_string_pretty(&result)
        .map_err(|e| format!("Failed to serialize json: {}", e))?);

    Ok(())
}

pub mod msg_printer {
    use serde_json::{Value, json};
    use ton_block::{CurrencyCollection, StateInit, Message, CommonMsgInfo, Grams};
    use ton_types::write_boc;
    use ton_types::Cell;
    use crate::helpers::{TonClient, create_client_local, decode_msg_body};
    use ton_client::boc::{get_compiler_version, ParamsOfGetCompilerVersion};
    use crate::Config;

    pub fn tree_of_cells_into_base64(root_cell: Option<&Cell>) -> Result<String, String> {
        match root_cell {
            Some(cell) => {
                let bytes = write_boc(cell)
                    .map_err(|e| format!("failed to serialize tree of cells: {}", e))?;
                Ok(base64::encode(bytes))
            }
            None => Ok("".to_string())
        }
    }

    async fn get_code_version(ton: TonClient, code: String) -> String {
        let result = get_compiler_version(
            ton,
            ParamsOfGetCompilerVersion {
                code,
            }
        );

        if let Ok(result) = result {
            if let Some(version) = result.version {
                return version;
            }
        }
        "Undefined".to_owned()
    }

    pub async fn serialize_state_init (state: &StateInit, ton: TonClient) -> Result<Value, String> {
        let code = tree_of_cells_into_base64(state.code.as_ref())?;
        Ok(json!({
            "split_depth" : state.split_depth.as_ref().map(|x| format!("{:?}", (x.as_u32()))).unwrap_or("None".to_string()),
            "special" : state.special.as_ref().map(|x| format!("{:?}", x)).unwrap_or("None".to_string()),
            "data" : tree_of_cells_into_base64(state.data.as_ref())?,
            "code" : code.clone(),
            "code_hash" : state.code.as_ref().map(|code| code.repr_hash().to_hex_string()).unwrap_or("None".to_string()),
            "data_hash" : state.data.as_ref().map(|code| code.repr_hash().to_hex_string()).unwrap_or("None".to_string()),
            "code_depth" : state.code.as_ref().map(|code| code.repr_depth().to_string()).unwrap_or("None".to_string()),
            "data_depth" : state.data.as_ref().map(|code| code.repr_depth().to_string()).unwrap_or("None".to_string()),
            "version" : get_code_version(ton, code).await,
            "lib" : tree_of_cells_into_base64(state.library.root())?,
        }))
    }

    fn serialize_msg_type(header: &CommonMsgInfo) -> Value {
        json!(match header {
            CommonMsgInfo::IntMsgInfo(_) => "internal",
            CommonMsgInfo::ExtInMsgInfo(_) => "external inbound",
            CommonMsgInfo::ExtOutMsgInfo(_) => "external outbound",
        }.to_owned() + " message")
    }

    fn serialize_grams(grams: &Grams) -> Value {
        json!(grams.to_string())
    }

    fn serialize_currency_collection(cc: &CurrencyCollection) -> Value {
        let grams = serialize_grams(&cc.grams);
        if cc.other.is_empty() {
            return grams;
        }
        let mut other = json!({});
        cc.other.iterate_with_keys(|key: u32, value| {
            other[key.to_string()] = json!(value.to_string());
            Ok(true)
        }).ok();
        json!({
            "value" : grams,
            "other" : other,
        })
    }

    fn serialize_msg_header(header: &CommonMsgInfo) -> Value {
        match header {
            CommonMsgInfo::IntMsgInfo(header) => {
                json!({
                    "ihr_disabled": &header.ihr_disabled.to_string(),
                    "bounce" : &header.bounce.to_string(),
                    "bounced" : &header.bounced.to_string(),
                    "source" : &header.src.to_string(),
                    "destination" : &header.dst.to_string(),
                    "value" : &serialize_currency_collection(&header.value),
                    "ihr_fee" : &serialize_grams(&header.ihr_fee),
                    "fwd_fee" : &serialize_grams(&header.fwd_fee),
                    "created_lt" : &header.created_lt.to_string(),
                    "created_at" : &header.created_at.to_string(),
                })
            },
            CommonMsgInfo::ExtInMsgInfo(header) => {
                json!({
                    "source" : &header.src.to_string(),
                    "destination" : &header.dst.to_string(),
                    "import_fee" : &serialize_grams(&header.import_fee),
                })
            },
            CommonMsgInfo::ExtOutMsgInfo(header) => {
                json!({
                    "source" : &header.src.to_string(),
                    "destination" : &header.dst.to_string(),
                    "created_lt" : &header.created_lt.to_string(),
                    "created_at" : &header.created_at.to_string(),
                })
            }
        }
    }

    pub async fn serialize_body(body_vec: Vec<u8>, abi_path: &str, ton: TonClient, config: &Config) -> Result<Value, String> {
        let empty_boc = write_boc(&Cell::default())
            .map_err(|e| format!("failed to serialize tree of cells: {}", e))?;
        if body_vec.cmp(&empty_boc) == std::cmp::Ordering::Equal {
            return Ok(json!("empty"));
        }
        let body_base64 = base64::encode(&body_vec);
        let mut res = {
            match decode_msg_body(ton.clone(), abi_path, &body_base64, false, config).await {
                Ok(res) => res,
                Err(_) => decode_msg_body(ton.clone(), abi_path, &body_base64, true, config).await?,
            }
        };
        let output = res.value.take().ok_or("failed to obtain the result")?;
        let mut decoded = json!({res.name : output});
        if let Some(header) = res.header {
            if header.expire.is_some() || header.pubkey.is_some() || header.time.is_some() {
                decoded["BodyHeader"] = json!({
                    "expire": json!(header.expire.map(|exp| format!("{exp}")).unwrap_or("None".to_string())),
                    "time": json!(header.time.map(|time| format!("{time}")).unwrap_or("None".to_string())),
                    "pubkey": json!(header.pubkey.unwrap_or("None".to_string())),
                })
            }
        }
        Ok(decoded)
    }

    pub async fn serialize_msg(msg: &Message, abi_path: Option<String>, config: &Config) -> Result<Value, String> {
        let mut res = json!({ });
        let ton = create_client_local()?;
        res["Type"] = serialize_msg_type(msg.header());
        res["Header"] = serialize_msg_header(msg.header());
        if msg.state_init().is_some() {
            res["Init"] = json!({"StateInit" : serialize_state_init(msg.state_init().unwrap(), ton.clone()).await?});
        }
        res["Body"] = json!(&tree_of_cells_into_base64(
            msg.body().map(|slice| slice.into_cell()).as_ref()
        )?);
        if abi_path.is_some() && msg.body().is_some() {
            let abi_path = abi_path.unwrap();
            let body_vec = write_boc(&msg.body().unwrap().into_cell())
                .map_err(|e| format!("failed to serialize body: {}", e))?;
            res["BodyCall"] =  match serialize_body(body_vec, &abi_path, ton, config).await {
                Ok(res) => res,
                Err(_) => {
                    json!("Undefined")
                }
            };
        }
        Ok(res)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_decode_msg_json() {
        let msg_boc = std::fs::read("tests/samples/wallet.boc").unwrap();
        let out = decode_message(msg_boc, Some("tests/samples/wallet.abi.json".to_owned())).await.unwrap();
        let _ : serde_json::Value = serde_json::from_str(&out).unwrap();
    }

    #[tokio::test]
    async fn test_decode_body_json() {
        let body = "te6ccgEBAQEARAAAgwAAALqUCTqWL8OX7JivfJrAAzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMQAAAAAAAAAAAAAAAEeGjADA==";
        let config = Config::default();
        let _out = decode_body(body, "tests/samples/wallet.abi.json", true, &config).await.unwrap();
    }
}
