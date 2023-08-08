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

use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signer};
use num_bigint::BigUint;
use crate::config::Config;
use crate::helpers::{create_client_verbose, query_with_limit, now, now_ms, TonClient};
use serde_json::{json, Value};
use ton_abi::{Contract, Token, TokenValue, Uint};
use ton_block::{ExternalInboundMessageHeader, Grams, Message, MsgAddressInt, MsgAddressExt, Serializable};
use ton_client::net::{OrderBy, SortDirection};
use ton_client::boc::{get_blockchain_config, ParamsOfGetBlockchainConfig};
use ton_types::{BuilderData, Cell, IBitstring, SliceData};

const PREFIX_UPDATE_CONFIG_MESSAGE_DATA: &str = "43665021";

const QUERY_FIELDS: &str = r#"
  p0
  p1
  p2
  p3
  p4
  p6 {
    mint_new_price
    mint_add_price
  }
  p7 {
    currency
    value
  }
  p8 {
    version
    capabilities
  }
  p9
  p10
  p12 {
    workchain_id
    enabled_since
    actual_min_split
    min_split
    max_split
    active
    accept_msgs flags
    zerostate_root_hash
    zerostate_file_hash
    version
    basic
    vm_version
    vm_mode
    min_addr_len
    max_addr_len
    addr_len_step
    workchain_type_id
  }
  p14 {
    masterchain_block_fee
    basechain_block_fee
  }
  p15 {
    validators_elected_for
    elections_start_before
    elections_end_before
    stake_held_for
  }
   p16 {
    max_validators
    max_main_validators
    min_validators
  }
  p17 {
    min_stake(format:DEC)
    max_stake(format:DEC)
    min_total_stake(format:DEC)
    max_stake_factor
  }
  p18 {
    utime_since
    bit_price_ps
    cell_price_ps
    mc_bit_price_ps
    mc_cell_price_ps
  }
  p20 {
    gas_price
    gas_limit
    special_gas_limit
    gas_credit
    block_gas_limit
    freeze_due_limit
    delete_due_limit
    flat_gas_limit
    flat_gas_price
  }
  p21 {
    gas_price
    gas_limit
    special_gas_limit
    gas_credit
    block_gas_limit
    freeze_due_limit
    delete_due_limit
    flat_gas_limit
    flat_gas_price
  }
  p22 {
    bytes {
      underload soft_limit hard_limit
    }
    gas {
      underload soft_limit hard_limit
    }
    lt_delta {
      underload soft_limit hard_limit
    }
  }
  p23 {
    bytes {
      underload soft_limit hard_limit
    }
    gas {
      underload soft_limit hard_limit
    }
    lt_delta {
      underload soft_limit hard_limit
    }
  }
  p24 {
    lump_price bit_price cell_price ihr_price_factor first_frac next_frac
  }
  p25 {
    lump_price bit_price cell_price ihr_price_factor first_frac next_frac
  }
  p28 {
    shuffle_mc_validators
    mc_catchain_lifetime
    shard_catchain_lifetime
    shard_validators_lifetime
    shard_validators_num
  }
  p29 {
    new_catchain_ids
    round_candidates
    next_candidate_delay_ms
    consensus_timeout_ms
    fast_attempts
    attempt_duration
    catchain_max_deps
    max_block_bytes
    max_collated_bytes
  }
  p30 {
    delections_step
    staker_init_code_hash
    validator_init_code_hash
  }
  p31
  p32 {
    utime_since
    utime_until
    total
    total_weight(format:DEC)
    list {
      public_key
      adnl_addr
      weight(format:DEC)
    }
  }
  p33 {
    utime_since
    utime_until
    total
    total_weight(format:DEC)
    list {
      public_key
      adnl_addr
      weight(format:DEC)
    }
  }
  p34 {
    utime_since
    utime_until
    total
    total_weight(format:DEC)
    list {
      public_key
      adnl_addr
      weight(format:DEC)
    }
  }
  p35 {
    utime_since
    utime_until
    total
    total_weight(format:DEC)
    list {
      public_key
      adnl_addr
      weight(format:DEC)
    }
  }
  p36 {
    utime_since
    utime_until
    total
    total_weight(format:DEC)
    list {
      public_key
      adnl_addr
      weight(format:DEC)
    }
  }
  p37 {
    utime_since
    utime_until
    total
    total_weight(format:DEC)
    list {
      public_key
      adnl_addr
      weight(format:DEC)
    }
  }
  p39 {
    adnl_addr
    temp_public_key
    seqno
    valid_until
    signature_r
    signature_s
  }
"#;
const OPTIONAL_CONFIGS: [&str; 3] = [
"p5",
r#"p40 {
    collations_score_weight
    min_samples_count
    min_slashing_protection_score
    resend_mc_blocks_count
    signing_score_weight
    slashing_period_mc_blocks_count
    z_param_numerator
    z_param_denominator
}"#,
r#"p42 {
    threshold
    payouts {
        license_type
        payout_percent
    }
}"#
];

async fn query_config(ton: &TonClient, result: &str) -> Result<Option<Value>, String> {
    let result = format!(r#"master {{ config {{ {} }} }}"#, result);
    match query_with_limit(
        ton.clone(),
        "blocks",
        json!({ "key_block": { "eq": true },
            "workchain_id": { "eq": -1 } }),
        &result,
        Some(vec!(OrderBy{ path: "seq_no".to_string(), direction: SortDirection::DESC })),
        Some(1),
    ).await {
        Ok(result) => {
            if result.is_empty() {
              Ok(None)
            } else {
              Ok(Some(result[0]["master"]["config"].clone()))
            }
        },
        Err(e) => {
            if e.message.contains("Server responded with code 400") {
                Ok(None)
            } else {
                Err(format!("failed to query master block config: {}", e))
            }
        }
    }
}

pub async fn query_global_config(config: &Config, index: Option<&str>) -> Result<(), String> {
    let ton = create_client_verbose(config)?;
    let request = QUERY_FIELDS.to_owned();

    let mut config_value = if let Some(config_value) = query_config(&ton, &request).await? {
        config_value.as_object().unwrap().clone()
    } else {
        return Err("Config was not set".to_string());
    };

    for request in OPTIONAL_CONFIGS {
        if let Ok(Some(opt_config_value)) = query_config(&ton, request).await {
            config_value.append(&mut opt_config_value.as_object().unwrap().clone());
        }
    }

    match index {
        None => {
            if !config.is_json {
                print!("Config: ");
            }
            println!("{:#}", Value::from(config_value));
        },
        Some(index) => {
            index.parse::<i32>()
                .map_err(|e| format!(r#"failed to parse "index": {}"#, e))?;
            let config_name = format!("p{}", index);
            let config_value = if let Some(v) = config_value.get(&config_name) {
                v
            } else {
                return Err("Config was not set".to_string());
            };
            if !config.is_json {
                print!("Config {}: ", config_name);
            }
            println!("{:#}", config_value);
        }
    }
    Ok(())
}

pub async fn gen_update_config_message(
    abi: Option<&str>,
    seqno: Option<&str>,
    config_master_file: &str,
    new_param_file: &str,
    is_json: bool
) -> Result<(), String> {
    let config_master_address = std::fs::read(&*(config_master_file.to_string() + ".addr"))
        .map_err(|e| format!(r#"failed to read "config_master": {}"#, e))?;
    let config_account = ton_types::AccountId::from_raw(config_master_address, 32*8);

    let private_key_of_config_account = std::fs::read(&*(config_master_file.to_string() + ".pk"))
        .map_err(|e| format!(r#"failed to read "config_master": {}"#, e))?;

    let config_str = std::fs::read_to_string(new_param_file)
        .map_err(|e| format!(r#"failed to read "new_param_file": {}"#, e))?;

    let (config_cell, key_number) = serialize_config_param(&config_str)?;
    let message = if let Some(abi) = abi {
        prepare_message_new_config_param_solidity(abi, config_cell, key_number, config_account, &private_key_of_config_account)?
    } else {
        let seqno = seqno.unwrap().parse().map_err(|e| format!(r#"failed to parse "seqno": {}"#, e))?;
        prepare_message_new_config_param(config_cell, seqno, key_number, config_account, &private_key_of_config_account)?
    };

    let msg_bytes = message.write_to_bytes()
        .map_err(|e| format!(r#"failed to serialize message": {}"#, e))?;
    let msg_hex = hex::encode(msg_bytes);

    if is_json {
        println!("{{\"Message\": \"{}\"}}", msg_hex);
    } else {
        println!("Message: {}", msg_hex);
    }

    Ok(())
}

pub fn serialize_config_param(config_str: &str) -> Result<(Cell, u32), String> {
    let config_json: serde_json::Value = serde_json::from_str(config_str)
        .map_err(|e| format!(r#"failed to parse "new_param_file": {}"#, e))?;
    let config_json = config_json.as_object()
        .ok_or(r#""new_param_file" is not json object"#.to_string())?;
    if config_json.len() != 1 {
        Err(r#""new_param_file" is not a valid json"#.to_string())?;
    }

    let mut key_number = None;
    if let Some(key) = config_json.keys().next() {
        if !key.starts_with('p') {
            Err(r#""new_param_file" is not a valid json"#.to_string())?;
        }
        key_number = Some(key.trim_start_matches('p').to_string());
    }

    let key_number = key_number
        .ok_or(r#""new_param_file" is not a valid json"#.to_string())?
        .parse::<u32>()
        .map_err(|e| format!(r#""new_param_file" is not a valid json: {}"#, e))?;

    let config_params = ton_block_json::parse_config(config_json)
        .map_err(|e| format!(r#"failed to parse config params from "new_param_file": {}"#, e))?;

    let config_param = config_params.config(key_number)
        .map_err(|e| format!(r#"failed to parse config params from "new_param_file": {}"#, e))?
        .ok_or(format!(r#"Not found config number {} in parsed config_params"#, key_number))?;

    let mut cell = BuilderData::default();
    config_param.write_to_cell(&mut cell)
        .map_err(|e| format!(r#"failed to serialize config param": {}"#, e))?;
    let config_cell = cell.references()[0].clone();

    Ok((config_cell, key_number))
}

fn prepare_message_new_config_param(
    config_param: Cell,
    seqno: u32,
    key_number: u32,
    config_account: SliceData,
    private_key_of_config_account: &[u8]
) -> Result<Message, String> {
    let prefix = hex::decode(PREFIX_UPDATE_CONFIG_MESSAGE_DATA).unwrap();
    let since_the_epoch = now() + 100; // timestamp + 100 secs

    let mut cell = BuilderData::default();
    cell.append_raw(prefix.as_slice(), 32).unwrap();
    cell.append_u32(seqno).unwrap();
    cell.append_u32(since_the_epoch).unwrap();
    cell.append_i32(key_number as i32).unwrap();
    cell.checked_append_reference(config_param.clone()).unwrap();

    let secret = SecretKey::from_bytes(private_key_of_config_account)
        .map_err(|e| format!(r#"failed to read private key from config-master file": {}"#, e))?;
    let public = PublicKey::from(&secret);
    let keypair = Keypair { secret, public };
        
    let msg_signature = keypair.sign(cell.finalize(0).unwrap().repr_hash().as_slice()).to_bytes();

    let mut cell = BuilderData::default();
    cell.append_raw(&msg_signature, 64*8).unwrap();
    cell.append_raw(prefix.as_slice(), 32).unwrap();
    cell.append_u32(seqno).unwrap();
    cell.append_u32(since_the_epoch).unwrap();
    cell.append_i32(key_number as i32).unwrap();
    cell.checked_append_reference(config_param).unwrap();

    let config_contract_address = MsgAddressInt::with_standart(None, -1, config_account).unwrap();
    let mut header = ExternalInboundMessageHeader::new(MsgAddressExt::AddrNone, config_contract_address);
    header.import_fee = Grams::zero();
    let body = SliceData::load_builder(cell).unwrap();
    let message = Message::with_ext_in_header_and_body(header, body);

    Ok(message)
}

fn prepare_message_new_config_param_solidity(
    abi: &str,
    config_param: Cell,
    key_number: u32,
    config_account: SliceData,
    private_key_of_config_account: &[u8]
) -> Result<Message, String> {
    let secret = SecretKey::from_bytes(private_key_of_config_account)
        .map_err(|e| format!(r#"failed to read private key from config-master file": {}"#, e))?;
    let public = PublicKey::from(&secret);
    let keypair = Keypair { secret, public };
    
    let config_contract_address = MsgAddressInt::with_standart(None, -1, config_account).unwrap();
    let since_the_epoch = now_ms();

    let header = [("time".to_owned(), TokenValue::Time(since_the_epoch))]
        .into_iter()
        .collect();

    let parameters = [
        Token::new("index", convert_to_uint(&key_number.to_be_bytes(), 32)),
        Token::new("data", TokenValue::Cell(config_param)),
    ];

    let abi = std::fs::read(abi)
        .map_err(|err| format!("cannot read abi file {}: {}", abi, err))?;
    let contract = Contract::load(&*abi)
        .map_err(|err| err.to_string())?;
    let function = contract.function("set_config_param")
        .map_err(|err| err.to_string())?;
    let body = function
        .encode_input(&header, &parameters, false, Some(&keypair), Some(config_contract_address.clone()))
        .and_then(SliceData::load_builder)
        .map_err(|err| format!("cannot prepare message body {}", err))?;

    let hdr = ExternalInboundMessageHeader::new(MsgAddressExt::AddrNone, config_contract_address);
    Ok(Message::with_ext_in_header_and_body(hdr, body))
}

fn convert_to_uint(value: &[u8], bits_count: usize) -> TokenValue {
    assert!(value.len() * 8 >= bits_count);
    TokenValue::Uint(Uint {
        number: BigUint::from_bytes_be(value),
        size: bits_count,
    })
}

pub async fn dump_blockchain_config(config: &Config, path: &str) -> Result<(), String> {
    let ton = create_client_verbose(config)?;

    let last_key_block_query = query_with_limit(
        ton.clone(),
        "blocks",
        json!({ "workchain_id": { "eq":-1 }, "key_block": { "eq":true }}),
        "boc",
        Some(vec![OrderBy{ path: "seq_no".to_owned(), direction: SortDirection::DESC }]),
        Some(1),
    ).await.map_err(|e| format!("failed to query last key block: {}", e))?;

    if last_key_block_query.is_empty() {
        return Err("Key block not found".to_string());
    }

    let block = last_key_block_query[0]["boc"].as_str()
        .ok_or("Failed to query last block BOC.")?.to_owned();

    let bc_config = get_blockchain_config(
        ton.clone(),
        ParamsOfGetBlockchainConfig {
            block_boc: block,
        },
    ).await
        .map_err(|e| format!("Failed to get blockchain config: {}", e))?;

    let bc_config = base64::decode(bc_config.config_boc)
        .map_err(|e| format!("Failed to decode BOC: {}", e))?;
    std::fs::write(path, bc_config)
        .map_err(|e| format!("Failed to write data to the file {}: {}", path, e))?;
    if !config.is_json {
        println!("Config successfully saved to {}", path);
    } else {
        println!("{{}}");
    }
    Ok(())
}
