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

use chrono::{Local, TimeZone};
use serde_json::json;
use ton_client::abi::{Abi, CallSet, encode_message, FunctionHeader, ParamsOfEncodeMessage, Signer};
use crate::config::Config;
use crate::helpers::{create_client_local, load_abi, load_ton_address, now, TonClient};
use crate::crypto::load_keypair;

pub struct EncodedMessage {
    pub message_id: String,
    pub message: String,
    pub expire: Option<u32>,
    pub address: String,
}

pub async fn prepare_message(
    ton: TonClient,
    addr: &str,
    abi: Abi,
    method: &str,
    params: &str,
    header: Option<FunctionHeader>,
    keys: Option<String>,
    is_json: bool,
) -> Result<EncodedMessage, String> {
    if !is_json {
        println!("Generating external inbound message...");
    }

    let msg_params = prepare_message_params(addr, abi, method, params, header.clone(), keys)?;

    let msg = encode_message(ton, msg_params).await
        .map_err(|e| format!("failed to create inbound message: {}", e))?;

    Ok(EncodedMessage {
        message: msg.message,
        message_id: msg.message_id,
        expire: header.and_then(|h| h.expire),
        address: addr.to_owned(),
    })
}

pub fn prepare_message_params (
    addr: &str,
    abi: Abi,
    method: &str,
    params: &str,
    header: Option<FunctionHeader>,
    keys: Option<String>,
) -> Result<ParamsOfEncodeMessage, String> {
    let keys = keys.map(|k| load_keypair(&k)).transpose()?;
    let params = serde_json::from_str(params)
        .map_err(|e| format!("arguments are not in json format: {}", e))?;

    let call_set = Some(CallSet {
        function_name: method.into(),
        input: Some(params),
        header: header.clone(),
    });

    Ok(ParamsOfEncodeMessage {
        abi,
        address: Some(addr.to_owned()),
        call_set,
        signer: if let Some(keys) = keys {
            Signer::Keys { keys }
        } else {
            Signer::None
        },
        ..Default::default()
    })
}

pub fn print_encoded_message(msg: &EncodedMessage, is_json:bool) {
    let expire = if msg.expire.is_some() {
        let expire_at = Local.timestamp_opt(msg.expire.unwrap() as i64, 0).single().unwrap();
        expire_at.to_rfc2822()
    } else {
        "unknown".to_string()
    };
    if !is_json {
        println!();
        println!("MessageId: {}", msg.message_id);
        println!("Expire at: {}", expire);
    } else {
        println!("  \"MessageId\": \"{}\",", msg.message_id);
        println!("  \"Expire at\": \"{}\",", expire);
    }
}

pub fn pack_message(msg: &EncodedMessage, method: &str, is_raw: bool) -> Result<Vec<u8>, String> {
    let res = if is_raw {
        base64::decode(&msg.message)
            .map_err(|e| format!("failed to decode message: {}", e))?
    } else {
        let json_msg = json!({
            "msg": {
                "message_id": msg.message_id,
                "message": msg.message,
                "expire": msg.expire,
                "address": msg.address,
            },
            "method": method,
        });
        serde_json::to_string(&json_msg)
            .map_err(|e| format!("failed to serialize message: {}", e))?
            .into_bytes()
    };
    Ok(res)
}

pub fn unpack_message(str_msg: &str) -> Result<(EncodedMessage, String), String> {
    let bytes = hex::decode(str_msg)
        .map_err(|e| format!("couldn't unpack message: {}", e))?;

    let str_msg = std::str::from_utf8(&bytes)
        .map_err(|e| format!("message is corrupted: {}", e))?;

    let json_msg: serde_json::Value = serde_json::from_str(str_msg)
        .map_err(|e| format!("couldn't decode message: {}", e))?;

    let method = json_msg["method"].as_str()
        .ok_or(r#"couldn't find "method" key in message"#)?
        .to_owned();
    let message_id = json_msg["msg"]["message_id"].as_str()
        .ok_or(r#"couldn't find "message_id" key in message"#)?
        .to_owned();
    let message = json_msg["msg"]["message"].as_str()
        .ok_or(r#"couldn't find "message" key in message"#)?
        .to_owned();
    let expire = json_msg["msg"]["expire"].as_u64().map(|x| x as u32);
    let address = json_msg["msg"]["address"].as_str()
        .ok_or(r#"couldn't find "address" key in message"#)?
        .to_owned();

    let msg = EncodedMessage {
        message_id, message, expire, address
    };
    Ok((msg, method))
}

pub async fn generate_message(
    config: &Config,
    addr: &str,
    abi: &str,
    method: &str,
    params: &str,
    keys: Option<String>,
    lifetime: u32,
    is_raw: bool,
    output: Option<&str>,
    timestamp: Option<u64>,
) -> Result<(), String> {
    let ton = create_client_local()?;

    let ton_addr = load_ton_address(addr, config)
        .map_err(|e| format!("failed to parse address: {}", e))?;

    let abi = load_abi(abi, config).await?;

    let expire_at = lifetime + timestamp.map(|ms| (ms / 1000) as u32).unwrap_or(now());
    let header = FunctionHeader {
        expire: Some(expire_at),
        time: timestamp,
        ..Default::default()
    };

    let msg = prepare_message(
        ton.clone(),
        &ton_addr,
        abi,
        method,
        params,
        Some(header),
        keys,
        config.is_json,
    ).await?;

    display_generated_message(&msg, method, is_raw, output, config.is_json)?;

    Ok(())
}

pub fn display_generated_message(
    msg: &EncodedMessage,
    method: &str,
    is_raw: bool,
    output: Option<&str>,
    is_json: bool,
) -> Result<(), String> {
    if is_json {
        println!("{{");
    }
    print_encoded_message(msg, is_json);

    let msg_bytes = pack_message(msg, method, is_raw)?;
    if let Some(out_file) = output {
        std::fs::write(out_file, msg_bytes)
            .map_err(|e| format!("cannot write message to file: {}", e))?;
        if !is_json {
            println!("Message saved to file {}", out_file);
        } else {
            println!("  \"Message\": \"saved to file {}\"", out_file);
        }
    } else {
        let msg_hex = hex::encode(&msg_bytes);
        if !is_json {
            println!("Message: {}", msg_hex);
            println!();
            qr2term::print_qr(msg_hex).map_err(|e| format!("failed to print QR code: {}", e))?;
            println!();
        } else {
            println!("  \"Message\": \"{}\"", msg_hex);
        }
    }
    if is_json {
        println!("}}");
    }
    Ok(())
}

