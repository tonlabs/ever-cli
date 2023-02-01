use serde_json::{Value, json};
use dengine::prelude::{DebotInterface, InterfaceResult};
use ton_client::abi::Abi;

const STDOUT_ID: &str = "c91dcc3fddb30485a3a07eb7c1e5e2aceaf75f4bc2678111de1f25291cdda80b";

pub const STDOUT_ABI: &str = r#"{
	"ABI version": 2,
	"header": ["time"],
	"functions": [
		{
			"name": "print",
			"inputs": [
				{"name":"message","type":"bytes"}
			],
			"outputs": [
			]
		},
		{
			"name": "constructor",
			"inputs": [
			],
			"outputs": [
			]
		}
	],
	"data": [
	],
	"events": [
	]
}"#;


pub struct Stdout {}
impl Stdout {
	pub fn new() -> Self {
		Self {}
	}
    pub fn print(&self, args: &Value) -> InterfaceResult {
		let text_vec = hex::decode(args["message"].as_str().unwrap()).unwrap();
		let text = std::str::from_utf8(&text_vec).unwrap();
		println!("{}", text);
		Ok((0, json!({})))
    }
}

#[async_trait::async_trait]
impl DebotInterface for Stdout {
    fn get_id(&self) -> String {
        STDOUT_ID.to_string()
    }

    fn get_abi(&self) -> Abi {
        Abi::Json(STDOUT_ABI.to_owned())
    }

    async fn call(&self, func: &str, args: &Value) -> InterfaceResult {
        match func {
            "print" => self.print(args),
            _ => Err(format!("function \"{}\" is not implemented", func)),
        }
    }
}