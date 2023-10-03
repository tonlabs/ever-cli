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
use crate::config::Config;
use crate::helpers::{create_client_local, read_keys, load_abi, calc_acc_address, load_abi_str};
use ed25519_dalek::PublicKey;
use serde_json::json;
use std::fs::OpenOptions;

use crate::crypto::{gen_seed_phrase, generate_keypair_from_mnemonic};
use ton_client::utils::{convert_address, ParamsOfConvertAddress, AddressStringFormat};

pub async fn generate_address(
    config: &Config,
    tvc: &str,
    abi_path: &str,
    wc_str: Option<&str>,
    keys_file: Option<&str>,
    new_keys: bool,
    initial_data: Option<&str>,
    update_tvc: bool,
) -> Result<(), String> {
    let contract = std::fs::read(tvc)
        .map_err(|e| format!("failed to read smart contract file: {}", e))?;

    let abi = load_abi(abi_path, config).await?;

    let phrase = if new_keys {
        gen_seed_phrase()?
    } else if keys_file.is_some() &&
        keys_file.unwrap().find(' ').is_some() && !new_keys {
            keys_file.unwrap().to_owned()
    } else {
        "".to_owned()
    };

    let keys = if !phrase.is_empty() {
        Some(generate_keypair_from_mnemonic(&phrase)?)
    } else if keys_file.is_some() {
        Some(read_keys(keys_file.unwrap())?)
    } else {
        None
    };


    let wc = wc_str.map(|wc| i32::from_str_radix(wc, 10))
        .transpose()
        .map_err(|e| format!("failed to parse workchain id: {}", e))?
        .unwrap_or(config.wc);

    let addr = calc_acc_address(
        &contract,
        wc,
        if keys.is_some() {
            Some(keys.clone().unwrap().public)
        } else {
            None
        },
        initial_data,
        abi.clone()
    ).await?;

    if update_tvc {
        let initial_data = initial_data.map(|s| s.to_string());
        let key_bytes = match keys.as_ref() {
            Some(keys) => {
                hex::decode(&keys.public)
                    .map_err(|e| format!("failed to decode public key: {}", e))?
            }
            _ => {
                vec![0; 32]
            }
        };
        let abi_str = load_abi_str(abi_path, config).await?;
        update_contract_state(tvc, &key_bytes, initial_data, &abi_str)?;
        if !config.is_json {
            println!("TVC file updated");
        }
    }

    if new_keys && keys_file.is_some() {
        let keys_json = serde_json::to_string_pretty(&keys.clone().unwrap())
            .map_err(|e| format!("failed to serialize the keypair: {}", e))?;
        std::fs::write(keys_file.unwrap(), keys_json)
            .map_err(|e| format!("failed to save the keypair: {}", e))?;
    }

    if !config.is_json {
        println!();
        if !phrase.is_empty() {
            println!(r#"Seed phrase: "{}""#, phrase);
        }
        println!("Raw address: {}", addr);
        println!("testnet:");
        println!("Non-bounceable address (for init): {}", calc_userfriendly_address(&addr, false, true)?);
        println!("Bounceable address (for later access): {}", calc_userfriendly_address(&addr, true, true)?);
        println!("mainnet:");
        println!("Non-bounceable address (for init): {}", calc_userfriendly_address(&addr, false, false)?);
        println!("Bounceable address (for later access): {}", calc_userfriendly_address(&addr, true, false)?);

        println!("Succeeded");
    } else {
        let mut res = json!({});
        if !phrase.is_empty() {
            res["seed_phrase"] = json!(phrase);
        }
        res["raw_address"] = json!(addr);
        res["testnet"] = json!({
            "non-bounceable": calc_userfriendly_address(&addr, false, true)?,
            "bounceable": calc_userfriendly_address(&addr, true, true)?
        });
        res["mainnet"] = json!({
            "non-bounceable": calc_userfriendly_address(&addr, false, false)?,
            "bounceable": calc_userfriendly_address(&addr, true, false)?
        });
        println!("{:#}", res);
    }
    Ok(())
}

fn calc_userfriendly_address(address: &str, bounce: bool, test: bool) -> Result<String, String> {
    convert_address(
        create_client_local()?,
        ParamsOfConvertAddress {
            address: address.to_owned(),
            output_format: AddressStringFormat::Base64{ url: true, bounce, test },
        }
    )
    .map(|r| r.address)
    .map_err(|e| format!("failed to convert address to base64 form: {}", e))
}

fn update_contract_state(tvc_file: &str, pubkey: &[u8], data: Option<String>, abi: &str) -> Result<(), String> {
    use std::io::{Seek, Write};
    let mut state_init = OpenOptions::new().read(true).write(true).open(tvc_file)
        .map_err(|e| format!("unable to open contract file: {}", e))?;

    let pubkey_object = PublicKey::from_bytes(pubkey)
        .map_err(|e| format!("unable to load public key: {}", e))?;

    let mut contract_image = ton_sdk::ContractImage::from_state_init_and_key(&mut state_init, &pubkey_object)
        .map_err(|e| format!("unable to load contract image: {}", e))?;

    if data.is_some() {
        contract_image.update_data(&data.unwrap(), abi)
            .map_err(|e| format!("unable to update contract image data: {}", e))?;
    }

    let vec_bytes = contract_image.serialize()
        .map_err(|e| format!("unable to serialize contract image: {}", e))?;

    state_init.seek(std::io::SeekFrom::Start(0))
        .map_err(|e| format!("failed to access the tvc file: {}", e))?;
    state_init.write_all(&vec_bytes)
        .map_err(|e| format!("failed to update the tvc file: {}", e))?;

    Ok(())
}