use super::dinterface::{decode_answer_id, decode_prompt, decode_array};

use serde_json::Value;
use ton_client::abi::Abi;
use dengine::prelude::{DebotInterface, InterfaceResult};
use ton_client::encoding::decode_abi_bigint;
use crate::helpers::TonClient;
use crate::debot::term_signing_box::TerminalSigningBox;
use crate::debot::{ChainProcessor, ProcessorError};
use tokio::sync::RwLock;
use std::sync::Arc;
use serde_json::json;

pub const ID: &str = "c13024e101c95e71afb1f5fa6d72f633d51e721de0320d73dfd6121a54e4d40a";

const ABI: &str = r#"
{
    "ABI version": 2,
    "version": "2.2",
    "header": ["time"],
    "functions": [
        {
            "name": "get",
            "id": "0x04895be9",
            "inputs": [
                {"name":"answerId","type":"uint32"},
                {"name":"prompt","type":"string"},
                {"name":"possiblePublicKeys","type":"uint256[]"}
            ],
            "outputs": [
                {"name":"handle","type":"uint32"}
            ]
        },
        {
            "name": "constructor",
            "id": "0x68b55f3f",
            "inputs": [
            ],
            "outputs": [
            ]
        }
    ],
    "data": [
    ],
    "events": [
    ],
    "fields": [
        {"name":"_pubkey","type":"uint256"},
        {"name":"_timestamp","type":"uint64"},
        {"name":"_constructorFlag","type":"bool"}
    ]
}
"#;

pub struct SigningBoxInput {
    handles: RwLock<Vec<TerminalSigningBox>>,
    client: TonClient,
    processor: Arc<RwLock<ChainProcessor>>,
}
impl SigningBoxInput {
    pub fn new(client: TonClient, processor: Arc<RwLock<ChainProcessor>>) -> Self {
        Self { handles: RwLock::new(vec![]), client, processor }
    }

    async fn get(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let prompt = decode_prompt(args)?;
        let possible_keys = decode_array(
            args,
            "possiblePublicKeys",
            |elem| {
                decode_abi_bigint(elem.as_str()?).ok()?;
                Some(elem.as_str().unwrap().to_string())
            }
        )?;
        println!("{}", prompt);
        let result = self.processor.write().await.next_signing_box();
        match result {
            Err(ProcessorError::InterfaceCallNeeded) => {
                let signing_box = TerminalSigningBox::new::<&[u8]>(
                    self.client.clone(), possible_keys, None
                ).await?;
                let handle = signing_box.handle();
                self.handles.write().await.push(signing_box);
                Ok((answer_id, json!({ "handle": handle.0})))
            }
            Err(e) => return Err(format!("{:?}", e)),
            Ok(handle) => {
                Ok((answer_id, json!({ "handle": handle}) ))
            }
        }
    }
}

#[async_trait::async_trait]
impl DebotInterface for SigningBoxInput {
    fn get_id(&self) -> String {
        ID.to_string()
    }

    fn get_abi(&self) -> Abi {
        Abi::Json(ABI.to_owned())
    }

    async fn call(&self, func: &str, args: &Value) -> InterfaceResult {
        match func {
            "get" => self.get(args).await,
            _ => Err(format!("function \"{}\" is not implemented", func)),
        }
    }
}