use crate::debot::term_browser::terminal_input;
use crate::helpers::load_ton_address;
use serde_json::{Value, json};
use ton_client::abi::Abi;
use dengine::prelude::{DebotInterface, InterfaceResult};
use super::dinterface::{decode_answer_id, decode_prompt};
use crate::config::Config;

const ID: &str = "d7ed1bd8e6230871116f4522e58df0a93c5520c56f4ade23ef3d8919a984653b";

pub const ABI: &str = r#"
{
    "ABI version": 2,
    "version": "2.2",
    "header": ["time"],
    "functions": [
        {
            "name": "get",
            "id": "0x210da005",
            "inputs": [
                {"name":"answerId","type":"uint32"},
                {"name":"prompt","type":"string"}
            ],
            "outputs": [
                {"name":"value","type":"address"}
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

pub struct AddressInput {
    config: Config
}
impl AddressInput {
    pub fn new(config: Config) -> Self {
        Self {config}
    }
    fn get(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let prompt = decode_prompt(args)?;
        let value = terminal_input(&prompt, |val| {
            let _ = load_ton_address(val, &self.config).map_err(|e| format!("Invalid address: {}", e))?;
            Ok(())
        });
        Ok((answer_id, json!({ "value": value })))
    }
    fn select(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let value = terminal_input("", |val| {
            let _ = load_ton_address(val, &self.config).map_err(|e| format!("Invalid address: {}", e))?;
            Ok(())
        });
        Ok((answer_id, json!({ "value": value })))
    }
}

#[async_trait::async_trait]
impl DebotInterface for AddressInput {
    fn get_id(&self) -> String {
        ID.to_string()
    }

    fn get_abi(&self) -> Abi {
        Abi::Json(ABI.to_owned())
    }

    async fn call(&self, func: &str, args: &Value) -> InterfaceResult {
        match func {
            "get" => self.get(args),
            "select" => self.select(args),
            _ => Err(format!("function \"{}\" is not implemented", func)),
        }
    }
}
