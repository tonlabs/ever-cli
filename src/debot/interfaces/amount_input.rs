use crate::debot::term_browser::terminal_input;
use serde_json::{Value, json};
use ton_client::abi::Abi;
use dengine::prelude::{DebotInterface, InterfaceResult};
use super::dinterface::{decode_answer_id, decode_num_arg, decode_prompt};
use ton_client::encoding::decode_abi_number;
use crate::convert;

const ID: &str = "a1d347099e29c1624c8890619daf207bde18e92df5220a54bcc6d858309ece84";

pub const ABI: &str = r#"
{
    "ABI version": 2,
    "version": "2.2",
    "header": ["time"],
    "functions": [
        {
            "name": "get",
            "id": "0x16740bd3",
            "inputs": [
                {"name":"answerId","type":"uint32"},
                {"name":"prompt","type":"string"},
                {"name":"decimals","type":"uint8"},
                {"name":"min","type":"uint128"},
                {"name":"max","type":"uint128"}
            ],
            "outputs": [
                {"name":"value","type":"uint128"}
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

pub struct AmountInput {}

impl AmountInput {
    pub fn new() -> Self {
        Self {}
    }
    fn get(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let prompt = decode_prompt(args)?;
        let decimals = decode_num_arg::<usize>(args, "decimals")?;
        if decimals > 255 {
            return Err(format!("too many decimals ({})", decimals));
        }
        let min = decode_num_arg::<u128>(args, "min")?;
        let max = decode_num_arg::<u128>(args, "max")?;
        let mut value = String::new();

        let prompt = format!(
            "{}\n(>= {} and <= {})",
            prompt,
            format_amount(min, decimals),
            format_amount(max, decimals)
        );
        let _ = terminal_input(&prompt, |val| {
            value = convert::convert_amount(val.as_str(), decimals)?;
            let number = decode_abi_number::<u128>(&value)
                .map_err(|e| format!("input is not a valid amount: {}", e))?;
            if number < min || number > max {
                return Err("amount is out of range".to_string());
            }
            Ok(())
        });
        Ok((answer_id, json!({ "value": value })))
    }
}

#[async_trait::async_trait]
impl DebotInterface for AmountInput {
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

fn format_amount(amount: u128, decimals: usize) ->  String {
    if decimals == 0 {
        format!("{}", amount)
    } else {
        let integer = amount / 10u128.pow(decimals as u32);
        let float = amount - integer * 10u128.pow(decimals as u32);
        format!("{}.{:0>width$}", integer, float, width = decimals)
    }
}
