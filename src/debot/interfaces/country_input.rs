use crate::debot::term_browser::terminal_input;
use serde_json::Value;
use ton_client::abi::Abi;
use ton_client::debot::{DebotInterface, InterfaceResult};
use super::dinterface::{decode_answer_id, decode_prompt, decode_array_strings};

const ID: &'static str = "9b593b1fec84f39a45821a119da78f79af36fe62a64541ba5fd04d5898cf6241";

pub const ABI: &str = r#"
{
	"ABI version": 2,
	"version": "2.2",
	"header": ["time"],
	"functions": [
		{
			"name": "get",
			"inputs": [
				{"name":"answerId","type":"uint32"},
				{"name":"prompt","type":"string"},
				{"name":"permitted","type":"string[]"},
				{"name":"banned","type":"string[]"}
			],
			"outputs": [
				{"name":"value","type":"string"}
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
	],
	"fields": [
		{"name":"_pubkey","type":"uint256"},
		{"name":"_timestamp","type":"uint64"},
		{"name":"_constructorFlag","type":"bool"}
	]
}
"#;

const ALL_COUNTRIES: &'static [&'static str] = &["AF", "AL", "DZ", "AS", "AD", "AO", "AI", "AQ", "AG", "AR", "AM", "AW", "AU", "AT", "AZ", "BS", "BH", "BD", "BB", "BY", "BE", "BZ", "BJ", "BM", "BT", "BO", "BA", "BW", "BV", "BR", "IO", "BN", "BG", "BF", "BI", "KH", "CM", "CA", "CV", "KY", "CF", "TD", "CL", "CN", "CX", "CC", "CO", "KM", "CG", "CD", "CK", "CR", "CI", "HR", "CU", "CY", "CZ", "DK", "DJ", "DM", "DO", "EC", "EG", "SV", "GQ", "ER", "EE", "ET", "FK", "FO", "FJ", "FI", "FR", "GF", "PF", "TF", "GA", "GM", "GE", "DE", "GH", "GI", "GR", "GL", "GD", "GP", "GU", "GT", "GN", "GW", "GY", "HT", "HM", "VA", "HN", "HK", "HU", "IS", "IN", "ID", "IR", "IQ", "IE", "IL", "IT", "JM", "JP", "JO", "KZ", "KE", "KI", "KP", "KR", "KW", "KG", "LA", "LV", "LB", "LS", "LR", "LY", "LI", "LT", "LU", "MO", "MG", "MW", "MY", "MV", "ML", "MT", "MH", "MQ", "MR", "MU", "YT", "MX", "FM", "MD", "MC", "MN", "MS", "MA", "MZ", "MM", "NA", "NR", "NP", "NL", "NC", "NZ", "NI", "NE", "NG", "NU", "NF", "MK", "MP", "NO", "OM", "PK", "PW", "PS", "PA", "PG", "PY", "PE", "PH", "PN", "PL", "PT", "PR", "QA", "RE", "RO", "RU", "RW", "SH", "KN", "LC", "PM", "VC", "WS", "SM", "ST", "SA", "SN", "SC", "SL", "SG", "SK", "SI", "SB", "SO", "ZA", "GS", "ES", "LK", "SD", "SR", "SJ", "SZ", "SE", "CH", "SY", "TW", "TJ", "TZ", "TH", "TL", "TG", "TK", "TO", "TT", "TN", "TR", "TM", "TC", "TV", "UG", "UA", "AE", "GB", "US", "UM", "UY", "UZ", "VU", "VE", "VN", "VG", "VI", "WF", "EH", "YE", "ZM", "ZW", "AX", "BQ", "CW", "GG", "IM", "JE", "ME", "BL", "MF", "RS", "SX", "SS", "XK"];

pub struct CountryInput {}

impl CountryInput {
    pub fn new() -> Self {
        Self {}
    }
    fn get(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let mut prompt = decode_prompt(args)?;
        let permitted = decode_array_strings(args, "permitted")?;
        let banned = decode_array_strings(args, "banned")?;
        if !permitted.is_empty() {
        	prompt.push_str("\nThese country codes are permitted:");
        	for i in &permitted {
        		prompt.push_str(format!(" {}", i).as_str());
        	}
        }
        if !banned.is_empty() {
        	prompt.push_str("\nThese country codes are banned:");
        	for i in &banned {
        		prompt.push_str(format!(" {}", i).as_str());
        	}
        }
        let mut result = String::new();
        let _ = terminal_input(&format!("{}", prompt),|val| {
        	result = val.to_string();
            if !banned.is_empty() && banned.contains(&val) {
            	Err(format!("Invalid enter, no such country permitted"))?;
            };
            if !permitted.is_empty() && !permitted.contains(&val) {
            	Err(format!("Invalid enter, no such country permitted"))?;
            }
            if !ALL_COUNTRIES.contains(&val.as_str()) {
            	Err(format!("Invalid enter, no such country"))?;
            }
            Ok(())
        });
        Ok((answer_id, json!({ "value": result })))
    }
}

#[async_trait::async_trait]
impl DebotInterface for CountryInput {
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
