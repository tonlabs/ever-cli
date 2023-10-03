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
use crate::helpers::create_client_verbose;
use crate::config::Config;
use crate::call::send_message_and_wait;

pub async fn sendfile(config: &Config, msg_boc: &str) -> Result<(), String> {
    let ton = create_client_verbose(config)?;
    let boc_vec = std::fs::read(msg_boc)
        .map_err(|e| format!("failed to read boc file: {}", e))?;
    let tvm_msg = ton_sdk::Contract::deserialize_message(&boc_vec[..])
        .map_err(|e| format!("failed to parse message from boc: {}", e))?;
    let dst = tvm_msg.dst()
        .ok_or("failed to parse dst address".to_string())?;

    if !config.is_json {
        println!("Sending message to account {}", dst);
    }
    send_message_and_wait(ton, None, base64::encode(&boc_vec), config).await?;
    if !config.is_json {
        println!("Succeded.");
    }
    Ok(())
}