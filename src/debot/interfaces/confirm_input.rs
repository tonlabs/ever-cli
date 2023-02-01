use crate::debot::term_browser::terminal_input;
use serde_json::{Value, json};
use ton_client::abi::Abi;
use dengine::prelude::{DebotInterface, InterfaceResult};
use super::dinterface::{decode_answer_id, decode_prompt};

const ID: &str = "16653eaf34c921467120f2685d425ff963db5cbb5aa676a62a2e33bfc3f6828a";

pub const ABI: &str = r#"
{
    "ABI version": 2,
    "version": "2.2",
    "header": ["time"],
    "functions": [
        {
            "name": "get",
            "id": "0x43490cf2",
            "inputs": [
                {"name":"answerId","type":"uint32"},
                {"name":"prompt","type":"string"}
            ],
            "outputs": [
                {"name":"value","type":"bool"}
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

pub struct ConfirmInput {}

impl ConfirmInput {
    pub fn new() -> Self {
        Self {}
    }
    fn get(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let prompt = decode_prompt(args)?;
        let mut yes_no = false;
        let _ = terminal_input(&format!("{} (y/n)", prompt), |val| {
            yes_no = match val.as_str() {
                "y" => true,
                "n" => false,
                _ => return Err("invalid enter".to_string()),
            };
            Ok(())
        });
        Ok((answer_id, json!({ "value": yes_no })))
    }
}

#[async_trait::async_trait]
impl DebotInterface for ConfirmInput {
    fn get_id(&self) -> String {
        ID.to_string()
    }

    fn get_abi(&self) -> Abi {
        Abi::Json(ABI.to_owned())
    }

    async fn call(&self, func: &str, args: &Value) -> InterfaceResult {
        match func {
            "get" => self.get(args),
            _ => Err(format!("function \"{}\" is not implemented", func)),
        }
    }
}
