use super::dinterface::decode_answer_id;
use crate::config::Config;
use crate::debot::term_signing_box::TerminalSigningBox;
use crate::helpers::TonClient;
use serde_json::{Value, json};
use ton_client::abi::Abi;
use dengine::prelude::{DebotInterface, InterfaceResult};

const ID: &str = "a56115147709ed3437efb89460b94a120b7fe94379c795d1ebb0435a847ee580";

const ABI: &str = r#"
{
    "ABI version": 2,
    "version": "2.2",
    "header": ["time"],
    "functions": [
        {
            "name": "getAccount",
            "id": "0x2e4fec08",
            "inputs": [
                {"name":"answerId","type":"uint32"}
            ],
            "outputs": [
                {"name":"value","type":"address"}
            ]
        },
        {
            "name": "getPublicKey",
            "id": "0x2c5b2088",
            "inputs": [
                {"name":"answerId","type":"uint32"}
            ],
            "outputs": [
                {"name":"value","type":"uint256"}
            ]
        },
        {
            "name": "getSigningBox",
            "id": "0x11f1f7db",
            "inputs": [
                {"name":"answerId","type":"uint32"}
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

pub struct UserInfo {
    client: TonClient,
    config: Config,
}
impl UserInfo {
    pub fn new(client: TonClient, config: Config) -> Self {
        Self { client, config }
    }

    fn get_account(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let value = self
            .config
            .wallet
            .clone()
            .unwrap_or_else(|| format!("0:{:064}", 0));
        Ok((answer_id, json!({ "value": value })))
    }

    fn get_public_key(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let value = self
            .config
            .pubkey
            .clone()
            .unwrap_or_else(|| format!("0x{:064}", 0));
        Ok((answer_id, json!({ "value": value })))
    }

    async fn get_signing_box(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let mut signing_box = TerminalSigningBox::new_with_keypath(
            self.client.clone(),
            self.config.keys_path.clone().unwrap_or_default(),
        )
        .await?;
        let handle = signing_box.leak();
        Ok((answer_id, json!({ "handle": handle.0})))
    }
}

#[async_trait::async_trait]
impl DebotInterface for UserInfo {
    fn get_id(&self) -> String {
        ID.to_string()
    }

    fn get_abi(&self) -> Abi {
        Abi::Json(ABI.to_owned())
    }

    async fn call(&self, func: &str, args: &Value) -> InterfaceResult {
        match func {
            "getAccount" => self.get_account(args),
            "getPublicKey" => self.get_public_key(args),
            "getSigningBox" => self.get_signing_box(args).await,
            _ => Err(format!("function \"{}\" is not implemented", func)),
        }
    }
}
